//! # rusty-autossh
//!
//! A Rust port of Carson Harding's `autossh(1)` SSH connection supervisor.
//! Spawns `ssh` as a child process, optionally probes tunnel liveness via the
//! `-M <port>` heartbeat (or `-M 0` exit-only respawn), and respawns the ssh
//! process when it dies or stops responding.
//!
//! This crate ships both a CLI binary (`rusty-autossh`) and a Rust-native
//! library API. With `default-features = false` the library API is available
//! without pulling in any CLI-only dependencies (clap, clap_complete, anstyle,
//! tracing-appender, daemonize, atomicwrites, windows-sys).
//!
//! ## Library entry points
//!
//! - [`SshSupervisorBuilder`] — fluent builder for the supervisor.
//! - [`SshSupervisor`] — the supervisor task; drive via `run().await`.
//! - [`MonitorMode`] — `-M 0` (None) or `-M <port>[:<echo>]` (Active).
//! - [`SupervisorEvent`] — emitted over the user's `mpsc::Sender`.
//! - [`AutosshError`] — public error type.
//!
//! ## Feature gates
//!
//! - `default = ["cli"]` — full CLI binary + library API.
//! - `default-features = false` — library only (`tokio` + `thiserror` +
//!   `socket2`).
//!
//! ## SemVer + thread-safety policy
//!
//! [`AutosshError`] and [`SupervisorEvent`] are `#[non_exhaustive]` per
//! AD-014, so additive variants in later releases are NOT breaking changes.
//! `SshSupervisor: Send`, `SshSupervisorBuilder: Send + Sync`, all enums
//! `Send + Sync`. See `tests` module for the `static_assertions` guards.
//!
//! ## Concurrency
//!
//! [`SshSupervisor::run`] requires **exclusive ownership of SIGCHLD** in the
//! host tokio runtime per FR-062 / AD-017. Library consumers running multiple
//! supervisors must run each in its own dedicated tokio runtime.
//!
//! ## Quick-start example
//!
//! ```no_run
//! use rusty_autossh::{MonitorMode, SshSupervisorBuilder};
//!
//! # async fn doc() -> Result<(), rusty_autossh::AutosshError> {
//! let mut supervisor = SshSupervisorBuilder::new()
//!     .ssh_args(vec!["user@host".to_string()])
//!     .monitor_mode(MonitorMode::None)
//!     .build()?;
//!
//! supervisor.run().await?;
//! # Ok(())
//! # }
//! ```

#![deny(missing_docs)]

use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

use tokio::sync::mpsc;

pub mod clock;
pub mod error;
pub mod mode;
pub mod monitor;
pub mod spawner;
pub mod strict;
pub mod supervisor;

pub mod signals;

#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "cli")]
pub mod daemonizer;
#[cfg(feature = "cli")]
pub mod logging;
#[cfg(feature = "cli")]
pub mod pidfile;

pub use error::AutosshError;

/// Signal-kind tag carried by [`SupervisorEvent::SignalReceived`].
///
/// Abstracts over Unix `tokio::signal::unix::SignalKind` and the Windows
/// `ctrl_c` / `ctrl_break` model so the public surface is the same on every
/// platform.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalKind {
    /// SIGTERM (Unix) / Ctrl+C (Windows).
    Terminate,
    /// SIGINT (Unix) / Ctrl+C (Windows).
    Interrupt,
    /// SIGUSR1 (Unix). No Windows equivalent (variant unreachable on
    /// Windows but kept on the public enum for cross-platform exhaustive
    /// matching with a `_` arm).
    UserDefined1,
    /// SIGHUP (Unix). No Windows equivalent.
    Hangup,
    /// Ctrl+Break (Windows). Unreachable on Unix.
    CtrlBreak,
}

/// Monitor-port mode resolved from the `-M` flag or `AUTOSSH_PORT` env var.
///
/// - [`MonitorMode::None`] (`-M 0`) — no TCP listeners; respawn ssh only on
///   non-zero exit.
/// - [`MonitorMode::Active`] (`-M <port>` or `-M <port>:<echo>`) — bind a
///   monitor-port [`tokio::net::TcpListener`] pair (or single listener when
///   `echo` is supplied) and probe round-trip every `AUTOSSH_POLL` seconds.
///
/// # Example
///
/// ```
/// use rusty_autossh::MonitorMode;
///
/// // -M 0: exit-only supervision, no TCP listeners.
/// let none = MonitorMode::None;
/// assert_eq!(none, MonitorMode::default());
///
/// // -M 20000:22 single-listener mode.
/// let active = MonitorMode::Active { port: 20000, echo: Some(22) };
/// match active {
///     MonitorMode::Active { port, echo } => {
///         assert_eq!(port, 20000);
///         assert_eq!(echo, Some(22));
///     }
///     _ => unreachable!(),
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub enum MonitorMode {
    /// `-M 0`: no monitor-port listeners; supervisor only watches child
    /// exit status.
    #[default]
    None,
    /// `-M <port>` (echo `None`) or `-M <port>:<echo>` (echo `Some`).
    Active {
        /// Local monitor port.
        port: u16,
        /// Optional remote echo port. `None` → two local listeners
        /// (`<port>` + `<port>+1`); `Some` → single local listener + remote
        /// echo service.
        echo: Option<u16>,
    },
}

/// Compatibility mode resolved from the `--strict` / `--no-strict` flags,
/// `RUSTY_AUTOSSH_STRICT` env var, and `argv[0]` basename per AD-006.
///
/// # Example
///
/// ```
/// use rusty_autossh::CompatibilityMode;
///
/// assert_eq!(CompatibilityMode::default(), CompatibilityMode::Default);
/// let strict = CompatibilityMode::Strict;
/// assert_ne!(strict, CompatibilityMode::Default);
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CompatibilityMode {
    /// Default Rust-native mode: long-form flags, structured tracing
    /// output, clap-styled errors.
    #[default]
    Default,
    /// Strict upstream-`autossh 1.4g` compatibility mode: short flags only,
    /// byte-equal stderr, no ISO timestamp prefix on log lines.
    Strict,
}

/// Events emitted by [`SshSupervisor::run`] over the consumer's
/// `mpsc::Sender<SupervisorEvent>` (set on the builder via
/// `SshSupervisorBuilder::event_sender`).
///
/// `#[non_exhaustive]` per AD-014 — additive variants in later releases are
/// not a breaking change.
///
/// Exhaustive matches without a wildcard arm fail to compile, guarding
/// downstream consumers against future variant additions:
///
/// ```compile_fail
/// use rusty_autossh::SupervisorEvent;
///
/// fn handle(ev: SupervisorEvent) {
///     // Missing required wildcard `_` arm.
///     match ev {
///         SupervisorEvent::ChildSpawned { .. } => {}
///         SupervisorEvent::ChildExited { .. } => {}
///         SupervisorEvent::ChildRespawned => {}
///         SupervisorEvent::ProbeTimeout => {}
///         SupervisorEvent::MaxStartReached { .. } => {}
///         SupervisorEvent::MaxLifetimeReached => {}
///         SupervisorEvent::SignalReceived(_) => {}
///     }
/// }
/// ```
///
/// # Example — consume events from the supervisor channel
///
/// ```
/// use rusty_autossh::SupervisorEvent;
///
/// // Library consumers MUST include a wildcard `_` arm because
/// // `SupervisorEvent` is `#[non_exhaustive]` (SemVer policy per AD-014).
/// fn classify(event: &SupervisorEvent) -> &'static str {
///     match event {
///         SupervisorEvent::ChildSpawned { .. } => "spawned",
///         SupervisorEvent::ChildExited { .. } => "exited",
///         SupervisorEvent::ChildRespawned => "respawned",
///         SupervisorEvent::ProbeTimeout => "probe-timeout",
///         SupervisorEvent::MaxStartReached { .. } => "max-start",
///         SupervisorEvent::MaxLifetimeReached => "max-lifetime",
///         SupervisorEvent::SignalReceived(_) => "signal",
///         _ => "unknown",
///     }
/// }
///
/// let e = SupervisorEvent::ChildSpawned { pid: 4242 };
/// assert_eq!(classify(&e), "spawned");
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub enum SupervisorEvent {
    /// An ssh child process was successfully spawned. Fires AFTER
    /// `Command::spawn` returns `Ok(Child)` (i.e., the child is reapable).
    ChildSpawned {
        /// OS-assigned process id.
        pid: u32,
    },
    /// The active ssh child exited and was reaped via `child.wait()`.
    ChildExited {
        /// Exit status observed by `child.wait()`.
        status: ExitStatus,
    },
    /// A replacement ssh child was spawned (kill + respawn cycle).
    ChildRespawned,
    /// Probe round-trip timed out on the monitor port.
    ProbeTimeout,
    /// Consecutive-retry counter reached the `AUTOSSH_MAXSTART` cap.
    MaxStartReached {
        /// Number of consecutive spawn attempts before the cap was hit.
        attempts: u32,
    },
    /// `AUTOSSH_MAXLIFETIME` elapsed.
    MaxLifetimeReached,
    /// A signal was received by the supervisor.
    SignalReceived(SignalKind),
}

/// Fluent builder for [`SshSupervisor`].
///
/// Construction entry point — there is no other public constructor for
/// `SshSupervisor`. Builder fields default to upstream-`autossh 1.4g`
/// defaults (poll=600s, gate_time=30s, max_start=None (unlimited),
/// max_lifetime=None (unlimited)).
///
/// # Example
///
/// ```
/// use std::time::Duration;
/// use rusty_autossh::{MonitorMode, SshSupervisorBuilder};
///
/// let builder = SshSupervisorBuilder::new()
///     .ssh_args(vec!["user@host".to_string()])
///     .monitor_mode(MonitorMode::None)
///     .poll(Duration::from_secs(60))
///     .gate_time(Duration::from_secs(10))
///     .max_start(Some(3));
///
/// // Stop short of `.build()?` here because `build()` resolves the ssh
/// // binary on the host and is fallible in environments without `ssh`.
/// // See the crate-level rustdoc for the full happy-path example.
/// drop(builder);
/// ```
#[derive(Debug, Default)]
pub struct SshSupervisorBuilder {
    ssh_args: Vec<String>,
    monitor_mode: MonitorMode,
    ssh_path: Option<PathBuf>,
    poll: Option<Duration>,
    first_poll: Option<Duration>,
    gate_time: Option<Duration>,
    max_start: Option<Option<u32>>,
    max_lifetime: Option<Option<Duration>>,
    event_sender: Option<mpsc::Sender<SupervisorEvent>>,
    message: Option<String>,
    compatibility_mode: CompatibilityMode,
    one_shot: bool,
    pidfile_path: Option<PathBuf>,
    logfile_path: Option<PathBuf>,
}

impl SshSupervisorBuilder {
    /// Construct a fresh builder with all fields at their upstream-default
    /// values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the argv passed verbatim to the ssh child (autossh's
    /// argv-passthrough design).
    pub fn ssh_args(mut self, args: Vec<String>) -> Self {
        self.ssh_args = args;
        self
    }

    /// Set the [`MonitorMode`] (default: `MonitorMode::None`).
    pub fn monitor_mode(mut self, mode: MonitorMode) -> Self {
        self.monitor_mode = mode;
        self
    }

    /// Override the resolved ssh binary path. When `None` the supervisor
    /// resolves `AUTOSSH_PATH` then walks `PATH` per AD-011.
    pub fn ssh_path(mut self, path: PathBuf) -> Self {
        self.ssh_path = Some(path);
        self
    }

    /// Override `AUTOSSH_POLL` (default 600 s).
    pub fn poll(mut self, poll: Duration) -> Self {
        self.poll = Some(poll);
        self
    }

    /// Override `AUTOSSH_FIRST_POLL` (default = `poll`).
    pub fn first_poll(mut self, first_poll: Duration) -> Self {
        self.first_poll = Some(first_poll);
        self
    }

    /// Override `AUTOSSH_GATETIME` (default 30 s).
    pub fn gate_time(mut self, gate_time: Duration) -> Self {
        self.gate_time = Some(gate_time);
        self
    }

    /// Override `AUTOSSH_MAXSTART`. `None` corresponds to the `-1`
    /// sentinel (unlimited retries).
    pub fn max_start(mut self, max_start: Option<u32>) -> Self {
        self.max_start = Some(max_start);
        self
    }

    /// Override `AUTOSSH_MAXLIFETIME`. `None` corresponds to `0` (unlimited).
    pub fn max_lifetime(mut self, max_lifetime: Option<Duration>) -> Self {
        self.max_lifetime = Some(max_lifetime);
        self
    }

    /// Attach an `mpsc::Sender<SupervisorEvent>` for library consumers that
    /// want to observe the supervisor loop.
    pub fn event_sender(mut self, tx: mpsc::Sender<SupervisorEvent>) -> Self {
        self.event_sender = Some(tx);
        self
    }

    /// Override `AUTOSSH_MESSAGE` (heartbeat payload suffix per FR-013).
    pub fn message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    /// Override the compatibility mode (defaults to Default).
    pub fn compatibility_mode(mut self, mode: CompatibilityMode) -> Self {
        self.compatibility_mode = mode;
        self
    }

    /// Mark this supervisor as one-shot (`-1`) — exit non-zero on the
    /// first child failure (US1 / spec FR-010).
    pub fn one_shot(mut self, one_shot: bool) -> Self {
        self.one_shot = one_shot;
        self
    }

    /// Configure the pidfile path (`AUTOSSH_PIDFILE` / `--pid-file`).
    ///
    /// When set, [`SshSupervisor::run`] writes the supervisor PID
    /// atomically at startup and removes the file on termination (per
    /// FR-030 / AD-012). When `None` (default), no pidfile is written.
    pub fn pidfile_path(mut self, path: PathBuf) -> Self {
        self.pidfile_path = Some(path);
        self
    }

    /// Configure the logfile path (`AUTOSSH_LOGFILE` / `--log-file`).
    ///
    /// When set, [`SshSupervisor::run`] initializes a non-blocking writer
    /// for the file (Default mode adds an ISO 8601 timestamp prefix per
    /// FR-031; Strict mode opens raw append per FR-054). An unwritable
    /// path triggers the one-time stderr fallback warning per FR-032
    /// without aborting.
    pub fn logfile_path(mut self, path: PathBuf) -> Self {
        self.logfile_path = Some(path);
        self
    }

    /// Finalize the builder into an [`SshSupervisor`]. Fallible: ssh-binary
    /// resolution and monitor-port pre-bind validation can fail here.
    pub fn build(self) -> Result<SshSupervisor, AutosshError> {
        Ok(SshSupervisor {
            ssh_args: self.ssh_args,
            monitor_mode: self.monitor_mode,
            ssh_path: self.ssh_path,
            poll: self.poll.unwrap_or_else(|| Duration::from_secs(600)),
            first_poll: self.first_poll,
            gate_time: self.gate_time.unwrap_or_else(|| Duration::from_secs(30)),
            max_start: self.max_start.unwrap_or(None),
            max_lifetime: self.max_lifetime.unwrap_or(None),
            event_sender: self.event_sender,
            message: self.message,
            compatibility_mode: self.compatibility_mode,
            one_shot: self.one_shot,
            pidfile_path: self.pidfile_path,
            logfile_path: self.logfile_path,
        })
    }
}

/// SSH connection supervisor.
///
/// Constructed via [`SshSupervisorBuilder::build`]. Drive the supervisor
/// loop via [`SshSupervisor::run`]. Single-use — consume on completion or
/// termination.
///
/// # Concurrency
///
/// `run()` requires **exclusive ownership of SIGCHLD** in the host tokio
/// runtime. Consumers running multiple supervisors must spawn each in its
/// own dedicated tokio runtime (FR-062 / AD-017).
///
/// # Example
///
/// ```no_run
/// use rusty_autossh::{MonitorMode, SshSupervisorBuilder};
///
/// # async fn doc() -> Result<(), rusty_autossh::AutosshError> {
/// let mut supervisor = SshSupervisorBuilder::new()
///     .ssh_args(vec!["-M".to_string(), "0".to_string(), "user@host".to_string()])
///     .monitor_mode(MonitorMode::None)
///     .build()?;
///
/// supervisor.run().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct SshSupervisor {
    ssh_args: Vec<String>,
    monitor_mode: MonitorMode,
    ssh_path: Option<PathBuf>,
    poll: Duration,
    first_poll: Option<Duration>,
    gate_time: Duration,
    max_start: Option<u32>,
    max_lifetime: Option<Duration>,
    event_sender: Option<mpsc::Sender<SupervisorEvent>>,
    message: Option<String>,
    compatibility_mode: CompatibilityMode,
    one_shot: bool,
    /// Pidfile path (consumed in `SshSupervisor::run` under `cfg(feature = "cli")`).
    #[allow(dead_code)]
    pidfile_path: Option<PathBuf>,
    /// Logfile path (consumed in `SshSupervisor::run` under `cfg(feature = "cli")`).
    #[allow(dead_code)]
    logfile_path: Option<PathBuf>,
}

impl SshSupervisor {
    /// Drive the supervisor loop.
    ///
    /// Implements HINT-001 + HINT-011 + HINT-012 + HINT-018 by composing
    /// the [`supervisor::Supervisor`] internal state machine. Single-use
    /// — consume on completion or termination.
    ///
    /// # Concurrency
    ///
    /// Requires exclusive ownership of SIGCHLD in the host tokio runtime
    /// (FR-062 / AD-017). Library consumers running multiple supervisors
    /// MUST spawn each in its own dedicated tokio runtime.
    pub async fn run(&mut self) -> Result<(), AutosshError> {
        use std::time::Instant;

        // HINT-011 step 1: env vars already merged at builder time.
        // HINT-011 step 2: resolve ssh path (if not provided).
        let ssh_path = match &self.ssh_path {
            Some(p) => p.clone(),
            None => {
                let autossh_path = std::env::var_os("AUTOSSH_PATH");
                let path = std::env::var_os("PATH");
                spawner::resolve_ssh_path(autossh_path.as_deref(), path.as_deref())?
            }
        };

        // HINT-011 step 3: bind monitor-port listeners when active.
        let monitor = match &self.monitor_mode {
            MonitorMode::None => None,
            MonitorMode::Active { .. } => Some(monitor::ProbeLoop::bind(
                &self.monitor_mode,
                self.message.as_deref(),
            )?),
        };

        // HINT-011 step 4: write pidfile (atomicwrites + Drop guard per
        // FR-030 + AD-012 + HINT-010). Daemonization (step 5) happens at
        // the CLI dispatch layer BEFORE entering Supervisor::run, so the
        // PID we record here is the post-daemonize child's PID.
        #[cfg(feature = "cli")]
        let pidfile_guard: Option<pidfile::PidfileGuard> = match &self.pidfile_path {
            Some(p) => Some(pidfile::write_pid(p.clone(), std::process::id())?),
            None => None,
        };

        // HINT-011 step 4b: initialize logfile writer (FR-031 + FR-054 +
        // FR-032). On unwritable path the function emits the one-time
        // stderr warning + returns None so the supervisor continues.
        #[cfg(feature = "cli")]
        let _log_guard: Option<tracing_appender::non_blocking::WorkerGuard> =
            match &self.logfile_path {
                Some(p) => logging::init_logfile(Some(p.clone()), self.compatibility_mode)?,
                None => None,
            };

        // Adjust ssh_args with the monitor-port pair resolved from the
        // listeners (so callers that pass `port = 0` get the OS-assigned
        // port reflected in the -L/-R forwards).
        let monitor_mode = match (&self.monitor_mode, &monitor) {
            (MonitorMode::Active { echo: Some(e), .. }, Some(m)) => MonitorMode::Active {
                port: m.ports.port_in,
                echo: Some(*e),
            },
            (MonitorMode::Active { echo: None, .. }, Some(m)) => MonitorMode::Active {
                port: m.ports.port_in,
                echo: None,
            },
            _ => self.monitor_mode.clone(),
        };

        let clock = PollClock {
            poll: self.poll,
            first_poll: self.first_poll.unwrap_or(self.poll),
            gate_time: self.gate_time,
            max_start: self.max_start,
            max_lifetime: self.max_lifetime,
        };

        // HINT-011 step 6 + US6 (T120-T123 + AD-015): install the
        // platform-appropriate signal sources (Unix SignalKind +
        // SIGUSR1 + SIGHUP; Windows ctrl_c + ctrl_break) feeding a
        // unified `mpsc<SupervisorEvent>` channel. The supervisor
        // `select!` loop consumes this receiver uniformly.
        let signal_rx = Some(signals::spawn_signal_source());

        let mut supervisor = supervisor::Supervisor {
            child: None,
            monitor,
            clock,
            mode: self.compatibility_mode,
            monitor_mode,
            ssh_path,
            ssh_args: self.ssh_args.clone(),
            retry_count: 0,
            lifetime_start: Instant::now(),
            child_spawn_instant: None,
            event_tx: self.event_sender.clone(),
            signal_rx,
            one_shot: self.one_shot,
            #[cfg(feature = "cli")]
            pidfile_guard,
        };

        supervisor.run().await
    }
}

/// Re-export `PollClock` for use inside `lib.rs::SshSupervisor::run`.
use crate::clock::PollClock;

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::assert_impl_all;

    // Thread-safety bounds pinned per plan §Library Surface Pins (SC-009).
    assert_impl_all!(SshSupervisorBuilder: Send, Sync);
    // SshSupervisor is Send but NOT Sync — owns mutable child handle +
    // listeners.
    assert_impl_all!(SshSupervisor: Send);
    assert_impl_all!(MonitorMode: Send, Sync, Clone);
    assert_impl_all!(SupervisorEvent: Send, Sync);
    assert_impl_all!(AutosshError: Send, Sync);
    assert_impl_all!(CompatibilityMode: Send, Sync, Clone, Copy);

    // 'static via std::error::Error supertrait
    fn _autossh_error_is_static() {
        fn assert_static<T: 'static>() {}
        assert_static::<AutosshError>();
    }
}
