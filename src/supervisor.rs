//! Supervisor loop — the heart of the rusty-autossh port.
//!
//! Per AD-005 + HINT-001 + HINT-011 + HINT-012 + HINT-013 + HINT-014 +
//! HINT-015 + HINT-018 this module owns the `tokio::select!` three-way
//! race between the ssh-child wait future, the monitor-port probe timeout,
//! and the unified signal-event stream.
//!
//! The respawn decision matrix per HINT-018 is implemented in
//! [`Supervisor::handle_child_exit`]; the lifetime-deadline check per
//! HINT-015 is implemented in [`Supervisor::check_lifetime`].

use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::{Duration, Instant};

use tokio::process::Child;
use tokio::sync::mpsc;

use crate::clock::PollClock;
use crate::monitor::{ProbeError, ProbeLoop};
use crate::spawner;
use crate::{AutosshError, CompatibilityMode, MonitorMode, SignalKind, SupervisorEvent};

#[cfg(feature = "cli")]
use crate::pidfile::PidfileGuard;

/// Internal supervisor state.
///
/// Owns the live ssh child, the bound monitor-port listeners, the poll
/// clock, the retry counter, the lifetime origin, the event sender + the
/// signal receiver, and the pidfile guard (when CLI feature is enabled).
#[derive(Debug)]
pub struct Supervisor {
    /// Currently-active ssh child (if any).
    pub child: Option<Child>,
    /// Bound monitor-port listener pair.
    pub monitor: Option<ProbeLoop>,
    /// Resolved poll clock (post-env-and-flag merge).
    pub clock: PollClock,
    /// Active compatibility mode (Default or Strict).
    pub mode: CompatibilityMode,
    /// Resolved [`MonitorMode`]; copy of what's used to drive monitor-port
    /// argv injection.
    pub monitor_mode: MonitorMode,
    /// Resolved ssh binary path.
    pub ssh_path: PathBuf,
    /// Argv passed verbatim to ssh.
    pub ssh_args: Vec<String>,
    /// Consecutive-retry counter (resets on gate-exceeding lifetime).
    pub retry_count: u32,
    /// `Instant` at which `Supervisor::run` was first entered (for
    /// AUTOSSH_MAXLIFETIME accounting per HINT-015).
    pub lifetime_start: Instant,
    /// `Instant` of the last `Command::spawn` Ok (for gate-time
    /// accounting per HINT-013).
    pub child_spawn_instant: Option<Instant>,
    /// Event sender for library consumers (None in pure-CLI mode).
    pub event_tx: Option<mpsc::Sender<SupervisorEvent>>,
    /// Unified signal-event receiver from `signals::spawn_signal_source`.
    pub signal_rx: Option<mpsc::Receiver<SupervisorEvent>>,
    /// One-shot (`-1`) flag — first failure exits non-zero.
    pub one_shot: bool,
    /// Pidfile RAII guard (when AUTOSSH_PIDFILE is set).
    #[cfg(feature = "cli")]
    pub pidfile_guard: Option<PidfileGuard>,
}

/// Respawn decision returned by the supervisor's exit-or-respawn logic.
#[derive(Debug, PartialEq, Eq)]
pub enum RespawnDecision {
    /// Spawn a replacement ssh after the configured gate-time backoff.
    Respawn,
    /// Terminate the supervisor with the given exit reason.
    Exit(ExitReason),
}

/// Reason the supervisor terminated.
#[derive(Debug, PartialEq, Eq)]
pub enum ExitReason {
    /// Clean exit (status 0, MaxLifetime, SIGTERM, etc.).
    Ok,
    /// `AUTOSSH_MAXSTART` cap reached.
    MaxStartReached {
        /// Number of consecutive spawn attempts.
        attempts: u32,
    },
    /// `AUTOSSH_MAXLIFETIME` deadline reached.
    MaxLifetimeReached,
    /// `-1` one-shot mode and the first child failed.
    OneShotFailed,
}

impl Supervisor {
    /// Compute the respawn decision after observing an ssh-child exit.
    ///
    /// Implements the HINT-018 decision matrix (top-to-bottom, first
    /// match wins) restricted to the post-exit rows; signal-driven rows
    /// are dispatched in [`Supervisor::run`].
    pub fn handle_child_exit(&mut self, status: ExitStatus, lifetime: Duration) -> RespawnDecision {
        // One-shot: any failure → exit non-zero immediately.
        if self.one_shot && !status.success() {
            return RespawnDecision::Exit(ExitReason::OneShotFailed);
        }

        // Clean exit (status 0):
        //   * MonitorMode::None → exit 0 (US2 AS3, HINT-018 row 2).
        //   * MonitorMode::Active → reset counter + respawn.
        if status.success() {
            return match self.monitor_mode {
                MonitorMode::None => RespawnDecision::Exit(ExitReason::Ok),
                _ => {
                    self.retry_count = 0;
                    RespawnDecision::Respawn
                }
            };
        }

        // Non-zero exit: gate-time check decides counter behavior.
        if lifetime >= self.clock.gate_time {
            // Gate-exceeding lifetime → reset counter.
            self.retry_count = 0;
        } else {
            // Short-lifetime exit → increment counter (HINT-013).
            self.retry_count = self.retry_count.saturating_add(1);
            if let Some(cap) = self.clock.max_start {
                if self.retry_count >= cap {
                    return RespawnDecision::Exit(ExitReason::MaxStartReached {
                        attempts: self.retry_count,
                    });
                }
            }
        }

        RespawnDecision::Respawn
    }

    /// Check the AUTOSSH_MAXLIFETIME deadline per HINT-015.
    ///
    /// Returns `Some(MaxLifetimeReached)` when the deadline is reached.
    /// The supervisor's `run` loop polls this between iterations so the
    /// lifetime check wins over `MaxStartReached` per HINT-015.
    pub fn check_lifetime(&self) -> Option<RespawnDecision> {
        let max = self.clock.max_lifetime?;
        if self.lifetime_start.elapsed() >= max {
            Some(RespawnDecision::Exit(ExitReason::MaxLifetimeReached))
        } else {
            None
        }
    }

    /// Emit a SupervisorEvent to the consumer's channel (best-effort).
    async fn emit(&self, ev: SupervisorEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(ev).await;
        }
    }

    /// Spawn (or respawn) the ssh child per HINT-011 step 7 + HINT-012.
    ///
    /// Injects monitor-port forwards into the argv per HINT-003. On
    /// success emits [`SupervisorEvent::ChildSpawned`] (T050 first
    /// spawn) — callers wanting respawn events should call
    /// [`Supervisor::emit_respawned`] before the next spawn.
    async fn spawn_child(&mut self) -> Result<(), AutosshError> {
        // HINT-003 injection (no-op when MonitorMode::None).
        let args = spawner::inject_monitor_forwards(&self.monitor_mode, &self.ssh_args);
        let child = spawner::spawn_ssh(&self.ssh_path, &args).await?;
        let pid = child.id().unwrap_or(0);
        self.child = Some(child);
        self.child_spawn_instant = Some(Instant::now());
        self.emit(SupervisorEvent::ChildSpawned { pid }).await;
        Ok(())
    }

    /// SIGTERM (Unix) or TerminateProcess (Windows) the active child and
    /// wait for `wait()` to resolve, with the HINT-015 10-second grace on
    /// Unix.
    async fn terminate_child(&mut self) -> Result<Option<ExitStatus>, AutosshError> {
        let Some(mut child) = self.child.take() else {
            return Ok(None);
        };

        #[cfg(unix)]
        {
            // SIGTERM via libc::kill on the child's pid (process_group=0
            // means the pid is its own group leader). We use `child.kill`
            // followed by a wait with grace; tokio doesn't expose SIGTERM
            // directly, but the test child responds to drop/kill identical
            // to SIGTERM for our purposes.
            if let Some(pid) = child.id() {
                unsafe {
                    libc_kill(pid as i32, 15 /* SIGTERM */);
                }
            }
            match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
                Ok(Ok(status)) => Ok(Some(status)),
                Ok(Err(e)) => Err(AutosshError::Io(e)),
                Err(_) => {
                    // Grace expired — escalate to SIGKILL.
                    let _ = child.kill().await;
                    let status = child.wait().await.map_err(AutosshError::Io)?;
                    Ok(Some(status))
                }
            }
        }
        #[cfg(windows)]
        {
            // Per FR-043 + HINT-016: TerminateProcess immediately, NO 10s
            // grace on Windows (BREAKING-CHANGE).
            let _ = child.kill().await;
            let status = child.wait().await.map_err(AutosshError::Io)?;
            Ok(Some(status))
        }
    }

    /// Handle SIGTERM/SIGINT per FR-040 + HINT-018 row "SIGTERM/SIGINT".
    ///
    /// T124: SIGTERM the ssh child (Unix), wait up to 10s for it to exit
    /// via `tokio::time::timeout(Duration::from_secs(10), child.wait())`,
    /// then SIGKILL on timeout; drop `PidfileGuard` implicitly via
    /// `Supervisor::drop` after `run()` returns; return clean exit so the
    /// supervisor exits with status 0 per FR-040.
    pub async fn handle_term_int_signal(&mut self) -> Result<(), AutosshError> {
        let _ = self.terminate_child().await;
        Ok(())
    }

    /// Handle SIGUSR1 per FR-041 + HINT-018 row "SIGUSR1".
    ///
    /// T125: SIGTERM the current ssh child, await reap, spawn a fresh ssh
    /// with identical argv per HINT-012 single-child invariant; emit
    /// [`SupervisorEvent::ChildRespawned`]; retry counter UNCHANGED per
    /// FR-041 + HINT-018 (distinct from SIGHUP).
    pub async fn handle_usr1_signal(&mut self) -> Result<(), AutosshError> {
        let _ = self.terminate_child().await;
        self.spawn_child().await?;
        self.emit(SupervisorEvent::ChildRespawned).await;
        Ok(())
    }

    /// Handle SIGHUP per FR-042 + Clarifications Q2 + HINT-018 row "SIGHUP".
    ///
    /// T126: RESET retry counter to 0 FIRST; SIGTERM the current ssh
    /// child, await reap, spawn a fresh ssh per HINT-012; emit
    /// [`SupervisorEvent::ChildRespawned`]; ops engineer can use this to
    /// "credit" the supervisor with a fresh retry budget per FR-042.
    pub async fn handle_hup_signal(&mut self) -> Result<(), AutosshError> {
        self.retry_count = 0;
        let _ = self.terminate_child().await;
        self.spawn_child().await?;
        self.emit(SupervisorEvent::ChildRespawned).await;
        Ok(())
    }

    /// Handle Windows Ctrl+C / Ctrl+Break per FR-043 + HINT-016.
    ///
    /// T127: invoke `TerminateProcess(child_handle, 1)` (via
    /// `tokio::process::Child::kill`) immediately — NO 10s grace on
    /// Windows per HINT-016 BREAKING-CHANGE; drop `PidfileGuard`
    /// implicitly; return clean exit per FR-043.
    #[cfg(windows)]
    pub async fn handle_ctrl_c_break_signal(&mut self) -> Result<(), AutosshError> {
        let _ = self.terminate_child().await;
        Ok(())
    }

    /// Unix stub for the Windows-only Ctrl+C/Ctrl+Break handler.
    ///
    /// `CtrlBreak` is unreachable on Unix (the variant exists in
    /// [`SignalKind`] for cross-platform exhaustive matching only). This
    /// stub keeps the call-site in `Supervisor::run` cfg-free.
    #[cfg(unix)]
    pub async fn handle_ctrl_c_break_signal(&mut self) -> Result<(), AutosshError> {
        let _ = self.terminate_child().await;
        Ok(())
    }

    /// Drive the supervisor loop per HINT-001 + HINT-011 + HINT-012.
    ///
    /// The startup ordering steps 1-6 (env resolve / ssh-path / bind /
    /// pidfile / daemonize / signals) happen at the call-site
    /// (`lib::run`/`SshSupervisor::run`) BEFORE `Supervisor::run` is
    /// invoked. This method executes step 7 (initial spawn) onwards.
    pub async fn run(&mut self) -> Result<(), AutosshError> {
        // HINT-011 step 7: initial spawn.
        self.spawn_child().await?;

        let mut next_probe_at = if self.monitor.is_some() {
            Some(Instant::now() + self.clock.first_poll)
        } else {
            None
        };

        loop {
            // HINT-015: check lifetime deadline BEFORE the retry-cap
            // check (lifetime wins per HINT-018 row 1).
            if let Some(RespawnDecision::Exit(ExitReason::MaxLifetimeReached)) =
                self.check_lifetime()
            {
                self.emit(SupervisorEvent::MaxLifetimeReached).await;
                let _ = self.terminate_child().await;
                return Ok(());
            }

            // HINT-001: three-way select! over child.wait /
            // monitor-timeout / signal-stream.
            let decision = self.select_one_tick(&mut next_probe_at).await?;

            match decision {
                LoopOutcome::ProbeOk => {
                    // Probe succeeded — continue the loop. next_probe_at
                    // is re-armed inside select_one_tick.
                    continue;
                }
                LoopOutcome::ChildExited { status, lifetime } => {
                    self.emit(SupervisorEvent::ChildExited { status }).await;
                    self.child = None;
                    match self.handle_child_exit(status, lifetime) {
                        RespawnDecision::Respawn => {
                            // AUTOSSH_GATETIME backoff before the next
                            // spawn. (For `-f` mode gate_time = 0.)
                            tokio::time::sleep(self.clock.gate_time).await;
                            self.spawn_child().await?;
                            self.emit(SupervisorEvent::ChildRespawned).await;
                            next_probe_at = if self.monitor.is_some() {
                                Some(Instant::now() + self.clock.first_poll)
                            } else {
                                None
                            };
                        }
                        RespawnDecision::Exit(ExitReason::Ok)
                        | RespawnDecision::Exit(ExitReason::MaxLifetimeReached) => {
                            return Ok(());
                        }
                        RespawnDecision::Exit(ExitReason::MaxStartReached { attempts }) => {
                            self.emit(SupervisorEvent::MaxStartReached { attempts })
                                .await;
                            return Err(AutosshError::MaxStartReached { attempts });
                        }
                        RespawnDecision::Exit(ExitReason::OneShotFailed) => {
                            return Err(AutosshError::MaxStartReached { attempts: 1 });
                        }
                    }
                }
                LoopOutcome::ProbeTimeout => {
                    self.emit(SupervisorEvent::ProbeTimeout).await;
                    // HINT-014: probe-timeout branch wins → SIGTERM
                    // child + reap; treat as non-zero exit for counter
                    // accounting.
                    let status = match self.terminate_child().await? {
                        Some(s) => s,
                        None => continue,
                    };
                    self.emit(SupervisorEvent::ChildExited { status }).await;
                    let lifetime = self
                        .child_spawn_instant
                        .map(|i| i.elapsed())
                        .unwrap_or(Duration::ZERO);

                    // For probe-timeout we synthesize a non-zero status
                    // for the decision matrix (the child may have exited
                    // 0 if it was the test's `exit_zero` stand-in).
                    let synthetic_status = make_failed_status();
                    match self.handle_child_exit(synthetic_status, lifetime) {
                        RespawnDecision::Respawn => {
                            tokio::time::sleep(self.clock.gate_time).await;
                            self.spawn_child().await?;
                            self.emit(SupervisorEvent::ChildRespawned).await;
                            next_probe_at = Some(Instant::now() + self.clock.first_poll);
                        }
                        RespawnDecision::Exit(ExitReason::MaxStartReached { attempts }) => {
                            self.emit(SupervisorEvent::MaxStartReached { attempts })
                                .await;
                            return Err(AutosshError::MaxStartReached { attempts });
                        }
                        RespawnDecision::Exit(_) => return Ok(()),
                    }
                }
                LoopOutcome::Signal(sig) => {
                    self.emit(SupervisorEvent::SignalReceived(sig)).await;
                    match sig {
                        SignalKind::Terminate | SignalKind::Interrupt => {
                            // HINT-018 row "SIGTERM/SIGINT": clean exit.
                            // T124 + FR-040 — Unix grace inside `terminate_child`.
                            return self.handle_term_int_signal().await;
                        }
                        SignalKind::CtrlBreak => {
                            // T127 + FR-043 + HINT-016: Windows
                            // TerminateProcess (no 10s grace).
                            return self.handle_ctrl_c_break_signal().await;
                        }
                        SignalKind::UserDefined1 => {
                            // T125 + FR-041 + HINT-018 row "SIGUSR1": force
                            // respawn, retry counter UNCHANGED.
                            self.handle_usr1_signal().await?;
                            next_probe_at = if self.monitor.is_some() {
                                Some(Instant::now() + self.clock.first_poll)
                            } else {
                                None
                            };
                        }
                        SignalKind::Hangup => {
                            // T126 + FR-042 + HINT-018 row "SIGHUP":
                            // reset counter to 0 + force respawn.
                            self.handle_hup_signal().await?;
                            next_probe_at = if self.monitor.is_some() {
                                Some(Instant::now() + self.clock.first_poll)
                            } else {
                                None
                            };
                        }
                    }
                }
            }
        }
    }

    /// Run one `select!` tick. Returns a [`LoopOutcome`] indicating which
    /// branch fired.
    async fn select_one_tick(
        &mut self,
        next_probe_at: &mut Option<Instant>,
    ) -> Result<LoopOutcome, AutosshError> {
        // Compute the probe sleep duration (or use a far-future sleep
        // when no monitor is configured).
        let probe_sleep = match *next_probe_at {
            Some(t) => {
                let now = Instant::now();
                if t > now { t - now } else { Duration::ZERO }
            }
            None => Duration::from_secs(60 * 60 * 24 * 365), // 1 year sentinel
        };

        // Ensure we have a child to wait on. Without one, the select
        // would block forever on the wait branch — but we always have a
        // child here because `run` spawns before entering the loop.
        let Some(child) = self.child.as_mut() else {
            return Err(AutosshError::Internal(
                "supervisor entered select with no live child",
            ));
        };

        // Take the signal receiver (it's optional for headless tests).
        let signal_recv = async {
            match self.signal_rx.as_mut() {
                Some(rx) => rx.recv().await,
                None => std::future::pending::<Option<SupervisorEvent>>().await,
            }
        };

        tokio::select! {
            biased;

            // HINT-014: child.wait() vs probe-timeout — when both could
            // fire we prefer child.wait() (natural exit dominates).
            res = child.wait() => {
                let status = res.map_err(AutosshError::Io)?;
                let lifetime = self
                    .child_spawn_instant
                    .map(|i| i.elapsed())
                    .unwrap_or(Duration::ZERO);
                Ok(LoopOutcome::ChildExited { status, lifetime })
            }

            _ = tokio::time::sleep(probe_sleep), if next_probe_at.is_some() => {
                // Probe sleep elapsed — perform a probe round-trip.
                let monitor = match self.monitor.as_mut() {
                    Some(m) => m,
                    None => {
                        // Should not happen (next_probe_at is None when
                        // no monitor). Re-arm and loop.
                        *next_probe_at = None;
                        return Ok(LoopOutcome::Signal(SignalKind::Terminate));
                    }
                };
                match monitor.probe(self.clock.poll).await {
                    Ok(()) => {
                        // Probe succeeded — schedule the next one.
                        *next_probe_at = Some(Instant::now() + self.clock.poll);
                        Ok(LoopOutcome::ProbeOk)
                    }
                    Err(ProbeError::Timeout) | Err(ProbeError::Io(_)) => {
                        *next_probe_at = Some(Instant::now() + self.clock.poll);
                        Ok(LoopOutcome::ProbeTimeout)
                    }
                }
            }

            sig = signal_recv => {
                match sig {
                    Some(SupervisorEvent::SignalReceived(k)) => {
                        Ok(LoopOutcome::Signal(k))
                    }
                    _ => Ok(LoopOutcome::Signal(SignalKind::Terminate)),
                }
            }
        }
        .and_then(|outcome| {
            // Convert ProbeOk into a "continue" loop tick by recursing
            // into the next iteration via a tail-call style result.
            match outcome {
                LoopOutcome::ProbeOk => {
                    // Sentinel — caller treats by looping again. We
                    // map to Signal(Terminate) here only as a degenerate
                    // case; the outer loop re-tests `check_lifetime`
                    // and re-enters select_one_tick.
                    Ok(LoopOutcome::ProbeOk)
                }
                other => Ok(other),
            }
        })
    }
}

/// Outcome of one `select!` tick inside [`Supervisor::run`].
#[derive(Debug)]
enum LoopOutcome {
    ChildExited {
        status: ExitStatus,
        lifetime: Duration,
    },
    ProbeOk,
    ProbeTimeout,
    Signal(SignalKind),
}

/// Construct a "failed" ExitStatus for probe-timeout synthesized
/// accounting. The actual exit code matters only as `!= 0`.
fn make_failed_status() -> ExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(1 << 8)
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(1)
    }
}

#[cfg(unix)]
unsafe extern "C" {
    // Re-export `kill` to avoid pulling in the `libc` crate (keeps the
    // always-on dep tree minimal per FR-061 / SC-008).
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_supervisor(monitor_mode: MonitorMode) -> Supervisor {
        Supervisor {
            child: None,
            monitor: None,
            clock: PollClock::default(),
            mode: CompatibilityMode::Default,
            monitor_mode,
            ssh_path: PathBuf::from("/bin/true"),
            ssh_args: Vec::new(),
            retry_count: 0,
            lifetime_start: Instant::now(),
            child_spawn_instant: Some(Instant::now()),
            event_tx: None,
            signal_rx: None,
            one_shot: false,
            #[cfg(feature = "cli")]
            pidfile_guard: None,
        }
    }

    #[test]
    fn clean_exit_in_monitor_none_terminates_supervisor() {
        let mut sup = mk_supervisor(MonitorMode::None);
        let dec = sup.handle_child_exit(ok_status(), Duration::from_secs(60));
        assert_eq!(dec, RespawnDecision::Exit(ExitReason::Ok));
    }

    #[test]
    fn clean_exit_in_monitor_active_respawns_and_resets_counter() {
        let mut sup = mk_supervisor(MonitorMode::Active {
            port: 20000,
            echo: None,
        });
        sup.retry_count = 5;
        let dec = sup.handle_child_exit(ok_status(), Duration::from_secs(60));
        assert_eq!(dec, RespawnDecision::Respawn);
        assert_eq!(sup.retry_count, 0);
    }

    #[test]
    fn short_lifetime_nonzero_exit_increments_counter() {
        let mut sup = mk_supervisor(MonitorMode::None);
        sup.clock.gate_time = Duration::from_secs(30);
        let dec = sup.handle_child_exit(fail_status(), Duration::from_secs(1));
        assert_eq!(dec, RespawnDecision::Respawn);
        assert_eq!(sup.retry_count, 1);
    }

    #[test]
    fn gate_exceeding_nonzero_exit_resets_counter() {
        let mut sup = mk_supervisor(MonitorMode::None);
        sup.clock.gate_time = Duration::from_secs(30);
        sup.retry_count = 4;
        let dec = sup.handle_child_exit(fail_status(), Duration::from_secs(60));
        assert_eq!(dec, RespawnDecision::Respawn);
        assert_eq!(sup.retry_count, 0);
    }

    #[test]
    fn max_start_cap_returns_max_start_reached() {
        let mut sup = mk_supervisor(MonitorMode::None);
        sup.clock.gate_time = Duration::from_secs(30);
        sup.clock.max_start = Some(3);
        sup.retry_count = 2;
        let dec = sup.handle_child_exit(fail_status(), Duration::from_secs(1));
        assert_eq!(
            dec,
            RespawnDecision::Exit(ExitReason::MaxStartReached { attempts: 3 })
        );
    }

    #[test]
    fn one_shot_first_failure_exits() {
        let mut sup = mk_supervisor(MonitorMode::None);
        sup.one_shot = true;
        let dec = sup.handle_child_exit(fail_status(), Duration::from_secs(1));
        assert_eq!(dec, RespawnDecision::Exit(ExitReason::OneShotFailed));
    }

    #[test]
    fn check_lifetime_returns_none_when_no_cap() {
        let sup = mk_supervisor(MonitorMode::None);
        assert!(sup.check_lifetime().is_none());
    }

    #[test]
    fn check_lifetime_returns_exit_when_deadline_passed() {
        let mut sup = mk_supervisor(MonitorMode::None);
        sup.clock.max_lifetime = Some(Duration::from_millis(1));
        sup.lifetime_start = Instant::now() - Duration::from_secs(10);
        let dec = sup.check_lifetime().expect("deadline passed");
        assert_eq!(dec, RespawnDecision::Exit(ExitReason::MaxLifetimeReached));
    }

    fn ok_status() -> ExitStatus {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(0)
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(0)
        }
    }

    fn fail_status() -> ExitStatus {
        make_failed_status()
    }
}
