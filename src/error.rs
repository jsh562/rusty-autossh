//! Public error type for `rusty-autossh`.
//!
//! Defines the [`AutosshError`] enum returned from the library API
//! ([`crate::SshSupervisor::run`], [`crate::SshSupervisorBuilder::build`]) and
//! used internally by all crate modules.
//!
//! # Forward-compatibility (SemVer policy)
//!
//! [`AutosshError`] is `#[non_exhaustive]` per AD-014, so additive variants in
//! later releases are NOT a breaking change. Downstream consumers MUST include
//! a wildcard `_` arm when pattern-matching:
//!
//! ```
//! # use rusty_autossh::AutosshError;
//! # fn handle(e: AutosshError) {
//! match e {
//!     AutosshError::SshNotFound { .. } => { /* ... */ }
//!     AutosshError::Io(_) => { /* ... */ }
//!     _ => { /* required wildcard arm */ }
//! }
//! # }
//! ```
//!
//! Exhaustive matches on `#[non_exhaustive]` types from a different crate are
//! a compile error — this guards downstream consumers from breakage when new
//! variants are added in later releases:
//!
//! ```compile_fail
//! use rusty_autossh::AutosshError;
//!
//! fn handle(e: AutosshError) {
//!     // Missing the required wildcard `_` arm — fails to compile because
//!     // `AutosshError` is `#[non_exhaustive]`.
//!     match e {
//!         AutosshError::SshNotFound { .. } => {}
//!         AutosshError::MonitorBindFailed { .. } => {}
//!         AutosshError::MaxStartReached { .. } => {}
//!         AutosshError::MaxLifetimeReached => {}
//!         AutosshError::PidfileWrite { .. } => {}
//!         AutosshError::LogfileWrite { .. } => {}
//!         AutosshError::Io(_) => {}
//!         AutosshError::Daemonize { .. } => {}
//!         AutosshError::Internal(_) => {}
//!     }
//! }
//! ```

use std::io;
use std::path::PathBuf;

/// Errors returned by the `rusty-autossh` library API.
///
/// `Send + Sync + 'static` per SC-009. `#[non_exhaustive]` per AD-014 so
/// additive variants are not a breaking change.
///
/// `source()` returns the wrapped inner error for wrapping variants (those
/// holding a `source: io::Error` field, the `#[from]` `Io` variant) and
/// `None` for leaf variants (no inner source).
///
/// # Example
///
/// ```
/// use std::io;
/// use rusty_autossh::AutosshError;
///
/// // io::Error converts via `#[from]` (AD-014).
/// let io_err = io::Error::new(io::ErrorKind::NotFound, "boom");
/// let err: AutosshError = io_err.into();
/// match err {
///     AutosshError::Io(_) => {}
///     _ => unreachable!(),
/// }
/// ```
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum AutosshError {
    /// The `ssh` binary could not be resolved from `AUTOSSH_PATH` or any
    /// entry in the host `PATH`. The `searched` field enumerates the
    /// directories probed (in walk order) for diagnostic surfacing.
    #[error("ssh binary not found; searched {} location(s)", searched.len())]
    SshNotFound {
        /// Directories probed during the `PATH` walk (or the verbatim
        /// `AUTOSSH_PATH` value when that env var was set).
        searched: Vec<PathBuf>,
    },

    /// Failed to bind a monitor-port [`tokio::net::TcpListener`] on
    /// `127.0.0.1:<port>` (typically `EADDRINUSE` or a permission error).
    #[error("failed to bind monitor port {port}: {source}")]
    MonitorBindFailed {
        /// The TCP port that failed to bind.
        port: u16,
        /// Underlying OS error.
        #[source]
        source: io::Error,
    },

    /// The consecutive-retry counter reached the `AUTOSSH_MAXSTART` cap
    /// (or `--max-start <n>` CLI override). Maps to upstream's
    /// `autossh: maximum retries reached` stderr.
    #[error("maximum retries reached after {attempts} attempts")]
    MaxStartReached {
        /// Number of consecutive child-spawn attempts performed before the
        /// cap was hit.
        attempts: u32,
    },

    /// `AUTOSSH_MAXLIFETIME` (or `--max-lifetime <secs>`) elapsed. Clean
    /// self-termination; supervisor exits 0.
    #[error("max lifetime reached")]
    MaxLifetimeReached,

    /// Atomic write of the pidfile failed at startup.
    #[error("failed to write pidfile {}: {source}", path.display())]
    PidfileWrite {
        /// Pidfile path that failed to write.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: io::Error,
    },

    /// Failed to open/append to the logfile.
    #[error("failed to write logfile {}: {source}", path.display())]
    LogfileWrite {
        /// Logfile path that failed to write.
        path: PathBuf,
        /// Underlying OS error.
        #[source]
        source: io::Error,
    },

    /// Generic I/O error surfaced from underlying syscalls.
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// The `daemonize` crate or Windows `CreateProcessW` self-respawn
    /// failed during `-f` background mode setup.
    #[error("daemonize failed: {reason}")]
    Daemonize {
        /// Human-readable reason for the daemonize failure.
        reason: String,
    },

    /// An internal invariant was violated. The `&'static str` payload is a
    /// short diagnostic tag (never user-supplied content).
    #[error("internal error: {0}")]
    Internal(&'static str),
}
