//! Cross-platform signal source.
//!
//! Per AD-015 + HINT-006 + FR-040..FR-043 this module exposes a single
//! [`spawn_signal_source`] entry that returns an
//! `mpsc::Receiver<SupervisorEvent>` populated by the platform's native
//! signal API.
//!
//! Unix branch installs `tokio::signal::unix::signal` listeners for
//! SIGTERM, SIGINT, SIGUSR1, SIGHUP. Windows branch installs
//! `tokio::signal::ctrl_c` + `ctrl_break` listeners.
//!
//! Per HINT-006 + AD-017: the supervisor requires EXCLUSIVE ownership of
//! SIGCHLD. Library consumers MUST NOT install their own SIGCHLD handler;
//! tokio's automatic SIGCHLD handler reaps the ssh child via
//! `Child::wait()`.

use tokio::sync::mpsc;

use crate::{SignalKind as PubSignalKind, SupervisorEvent};

/// Default channel capacity for the signal source.
///
/// 16 events is sufficient for human-paced signal delivery (an ops
/// engineer cannot realistically send signals faster than the supervisor
/// can drain them on a `select!` tick).
const SIGNAL_CHANNEL_CAPACITY: usize = 16;

/// Spawn the platform-appropriate signal source and return the receiver
/// half of the event channel.
///
/// Platform behavior:
/// - **Unix**: listens for SIGTERM, SIGINT, SIGUSR1, SIGHUP and forwards
///   each via [`SupervisorEvent::SignalReceived`].
/// - **Windows**: listens for Ctrl+C + Ctrl+Break and forwards via
///   [`SupervisorEvent::SignalReceived`].
///
/// The returned receiver is owned by the supervisor's `select!` loop.
pub fn spawn_signal_source() -> mpsc::Receiver<SupervisorEvent> {
    let (tx, rx) = mpsc::channel::<SupervisorEvent>(SIGNAL_CHANNEL_CAPACITY);
    install_listeners(tx);
    rx
}

#[cfg(unix)]
fn install_listeners(tx: mpsc::Sender<SupervisorEvent>) {
    use tokio::signal::unix::{SignalKind, signal};

    // IMPORTANT: `signal(kind)` is called SYNCHRONOUSLY here (outside the
    // `tokio::spawn`) so the OS signal handler is registered BEFORE this
    // function returns. Previously the `signal()` call lived inside the
    // spawned task, which created a race: on slower runners (notably
    // aarch64-apple-darwin CI) the signal could arrive before the listener
    // task had been scheduled, missing the event entirely.
    fn spawn_one(tx: mpsc::Sender<SupervisorEvent>, kind: SignalKind, tag: PubSignalKind) {
        let Ok(mut stream) = signal(kind) else {
            return;
        };
        tokio::spawn(async move {
            while stream.recv().await.is_some() {
                if tx.send(SupervisorEvent::SignalReceived(tag)).await.is_err() {
                    break;
                }
            }
        });
    }

    spawn_one(
        tx.clone(),
        SignalKind::terminate(),
        PubSignalKind::Terminate,
    );
    spawn_one(
        tx.clone(),
        SignalKind::interrupt(),
        PubSignalKind::Interrupt,
    );
    spawn_one(
        tx.clone(),
        SignalKind::user_defined1(),
        PubSignalKind::UserDefined1,
    );
    spawn_one(tx, SignalKind::hangup(), PubSignalKind::Hangup);
}

#[cfg(windows)]
fn install_listeners(tx: mpsc::Sender<SupervisorEvent>) {
    use tokio::signal::windows::{ctrl_break, ctrl_c};

    // Same synchronous-registration pattern as the Unix branch — install
    // each handler before the consumer task is spawned so no signal can
    // arrive in the window between `spawn_signal_source` returning and
    // the consumer task scheduling.
    let tx_c = tx.clone();
    if let Ok(mut stream) = ctrl_c() {
        tokio::spawn(async move {
            while stream.recv().await.is_some() {
                if tx_c
                    .send(SupervisorEvent::SignalReceived(PubSignalKind::Interrupt))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
    }

    if let Ok(mut stream) = ctrl_break() {
        tokio::spawn(async move {
            while stream.recv().await.is_some() {
                if tx
                    .send(SupervisorEvent::SignalReceived(PubSignalKind::CtrlBreak))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
    }
}
