//! Monitor-port heartbeat probe.
//!
//! Per AD-008 + HINT-002 + HINT-020 this module owns:
//!
//! - [`ProbeLoop`] — the bound monitor-port `TcpListener` pair (or single
//!   listener for `-M port:echo` mode).
//! - [`ProbeLoop::bind`] — pre-bind the listener pair before any ssh
//!   spawn (per HINT-011 step 3).
//! - [`ProbeLoop::probe`] — perform one round-trip and observe the result.
//! - [`probe_payload`] — pure function producing the 16-byte ASCII
//!   timestamp + newline (plus optional `AUTOSSH_MESSAGE` suffix) wire
//!   format per HINT-002 + FR-013.
//!
//! `SO_REUSEADDR` is set on each listener via the `socket2` crate (Unix)
//! so a respawn or `-f` self-respawn does not encounter `EADDRINUSE` on the
//! brief overlap window per HINT-020.

use std::io;
use std::net::SocketAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use socket2::{Domain, Protocol, Socket, Type};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::{AutosshError, MonitorMode};

/// Errors surfaced by [`ProbeLoop::probe`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// Round-trip timed out within the configured poll window.
    #[error("probe round-trip timed out")]
    Timeout,
    /// Underlying I/O failure on the listener pair or echo connection.
    #[error("probe io error: {0}")]
    Io(#[from] io::Error),
}

/// Bound monitor-port pair recording the local + remote ports (post-`bind`).
///
/// Useful for tests / library consumers that need the resolved port pair
/// after the listeners were created (e.g. when binding to ephemeral
/// `port = 0`).
#[derive(Debug, Clone, Copy)]
pub struct MonitorPortPair {
    /// Local listener port (`127.0.0.1:<port>`).
    pub port_in: u16,
    /// Remote echo-forward port (`port + 1` for two-listener mode, the
    /// configured echo port otherwise).
    pub port_out: u16,
}

/// Bound monitor-port listener(s) for the supervisor's probe loop.
///
/// - Two-listener mode (`-M <port>`): both `listener_in` (`127.0.0.1:port`)
///   and `listener_out` (`127.0.0.1:port+1`) are bound; the supervisor
///   writes the heartbeat payload to `listener_in` (forwarded via ssh
///   `-L`/`-R` to `listener_out`) and reads it back.
/// - Single-listener mode (`-M port:echo`): only `listener_in` is bound;
///   the remote echo service handles the round-trip.
#[derive(Debug)]
pub struct ProbeLoop {
    /// The local listener bound on `127.0.0.1:<port>`.
    pub listener_in: TcpListener,
    /// Optional second listener bound on `127.0.0.1:<port+1>`. `None` in
    /// `-M port:echo` mode.
    pub listener_out: Option<TcpListener>,
    /// Optional `AUTOSSH_MESSAGE` suffix appended to each heartbeat
    /// payload per FR-013.
    pub message_suffix: Option<String>,
    /// Resolved port pair (set by `bind`).
    pub ports: MonitorPortPair,
}

impl ProbeLoop {
    /// Bind the monitor-port listener(s) per [`MonitorMode`] semantics +
    /// HINT-020 `SO_REUSEADDR` policy.
    ///
    /// - [`MonitorMode::None`] → returns
    ///   [`AutosshError::Internal`] (callers must not invoke `bind` on
    ///   `None`; the supervisor short-circuits the call site).
    /// - [`MonitorMode::Active`] with `echo: None` → binds both
    ///   `127.0.0.1:<port>` and `127.0.0.1:<port+1>`.
    /// - [`MonitorMode::Active`] with `echo: Some(_)` → binds only
    ///   `127.0.0.1:<port>`.
    pub fn bind(mode: &MonitorMode, message: Option<&str>) -> Result<Self, AutosshError> {
        // Defensive: reject embedded newlines in AUTOSSH_MESSAGE — they
        // would corrupt the upstream wire format. Keep this check at
        // construction so the probe loop never has to worry about it.
        if let Some(m) = message {
            if m.contains('\n') {
                return Err(AutosshError::Internal(
                    "AUTOSSH_MESSAGE contains embedded newline",
                ));
            }
        }

        match mode {
            MonitorMode::None => Err(AutosshError::Internal(
                "ProbeLoop::bind called with MonitorMode::None",
            )),
            MonitorMode::Active { port, echo: None } => {
                let listener_in = bind_reuseaddr(*port)?;
                let in_port = listener_in
                    .local_addr()
                    .map_err(|source| AutosshError::MonitorBindFailed {
                        port: *port,
                        source,
                    })?
                    .port();
                // For ephemeral binds we must derive the out-port from the
                // input port. Real autossh uses port+1; tests using port=0
                // need a separate ephemeral allocation for port_out.
                let out_target = if *port == 0 {
                    0
                } else {
                    port.saturating_add(1)
                };
                let listener_out = bind_reuseaddr(out_target)?;
                let out_port = listener_out
                    .local_addr()
                    .map_err(|source| AutosshError::MonitorBindFailed {
                        port: out_target,
                        source,
                    })?
                    .port();
                Ok(Self {
                    listener_in,
                    listener_out: Some(listener_out),
                    message_suffix: message.map(String::from),
                    ports: MonitorPortPair {
                        port_in: in_port,
                        port_out: out_port,
                    },
                })
            }
            MonitorMode::Active {
                port,
                echo: Some(echo),
            } => {
                let listener_in = bind_reuseaddr(*port)?;
                let in_port = listener_in
                    .local_addr()
                    .map_err(|source| AutosshError::MonitorBindFailed {
                        port: *port,
                        source,
                    })?
                    .port();
                Ok(Self {
                    listener_in,
                    listener_out: None,
                    message_suffix: message.map(String::from),
                    ports: MonitorPortPair {
                        port_in: in_port,
                        port_out: *echo,
                    },
                })
            }
        }
    }

    /// Perform one probe round-trip per HINT-002.
    ///
    /// Connects to the local input listener, writes the heartbeat payload
    /// (16-byte ASCII timestamp + optional `AUTOSSH_MESSAGE` suffix + LF),
    /// then awaits an echoed copy on the paired output listener (or the
    /// same connection in echo-mode) within `poll`. Returns
    /// [`ProbeError::Timeout`] on round-trip timeout and
    /// [`ProbeError::Io`] on underlying I/O failure.
    pub async fn probe(&mut self, poll: Duration) -> Result<(), ProbeError> {
        let unix_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let payload = probe_payload(unix_ts, self.message_suffix.as_deref());
        let port_in = self.ports.port_in;

        match self.listener_out.as_mut() {
            Some(out) => {
                // Two-listener mode: connect to listener_in (the supervisor
                // owns this end; in real life ssh -L makes it the remote's
                // forwarded port) AND accept on listener_out (paired
                // -R-forward back).
                let roundtrip = async {
                    let write_payload = payload.clone();
                    let read_len = payload.len();
                    let writer = async move {
                        let mut stream = TcpStream::connect(("127.0.0.1", port_in)).await?;
                        stream.write_all(&write_payload).await?;
                        stream.flush().await?;
                        Ok::<(), io::Error>(())
                    };
                    let reader = async {
                        let (mut sock, _) = out.accept().await?;
                        let mut buf = vec![0u8; read_len];
                        sock.read_exact(&mut buf).await?;
                        Ok::<Vec<u8>, io::Error>(buf)
                    };
                    let (_, bytes) = tokio::try_join!(writer, reader)?;
                    Ok::<Vec<u8>, io::Error>(bytes)
                };
                match tokio::time::timeout(poll, roundtrip).await {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(e)) => Err(ProbeError::Io(e)),
                    Err(_) => Err(ProbeError::Timeout),
                }
            }
            None => {
                // Echo-mode: write + read echo on the same stream.
                let read_len = payload.len();
                let roundtrip = async move {
                    let mut stream = TcpStream::connect(("127.0.0.1", port_in)).await?;
                    stream.write_all(&payload).await?;
                    stream.flush().await?;
                    let mut buf = vec![0u8; read_len];
                    stream.read_exact(&mut buf).await?;
                    Ok::<Vec<u8>, io::Error>(buf)
                };
                match tokio::time::timeout(poll, roundtrip).await {
                    Ok(Ok(_)) => Ok(()),
                    Ok(Err(e)) => Err(ProbeError::Io(e)),
                    Err(_) => Err(ProbeError::Timeout),
                }
            }
        }
    }
}

/// Bind a TCP listener on `127.0.0.1:<port>` with `SO_REUSEADDR` set per
/// HINT-020.
fn bind_reuseaddr(port: u16) -> Result<TcpListener, AutosshError> {
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .map_err(|_| AutosshError::Internal("monitor-port socket address parse failed"))?;

    let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))
        .map_err(|source| AutosshError::MonitorBindFailed { port, source })?;

    // HINT-020: SO_REUSEADDR on Unix avoids EADDRINUSE during the brief
    // TIME_WAIT window on a respawn. Windows treats SO_REUSEADDR
    // differently (it overlaps active binds), so only set it on Unix.
    #[cfg(unix)]
    {
        socket
            .set_reuse_address(true)
            .map_err(|source| AutosshError::MonitorBindFailed { port, source })?;
    }

    socket
        .set_nonblocking(true)
        .map_err(|source| AutosshError::MonitorBindFailed { port, source })?;

    socket
        .bind(&addr.into())
        .map_err(|source| AutosshError::MonitorBindFailed { port, source })?;

    socket
        .listen(128)
        .map_err(|source| AutosshError::MonitorBindFailed { port, source })?;

    let std_listener: std::net::TcpListener = socket.into();
    TcpListener::from_std(std_listener)
        .map_err(|source| AutosshError::MonitorBindFailed { port, source })
}

/// Construct the heartbeat payload bytes per HINT-002 + FR-013.
///
/// Format:
/// - Without `AUTOSSH_MESSAGE`: `format!("{:016}\n", unix_ts)` — exactly
///   17 bytes (16 ASCII digits + `\n`).
/// - With `AUTOSSH_MESSAGE`: `format!("{:016} {}\n", unix_ts, message)` —
///   16 digits + single space + message + `\n`.
///
/// The timestamp is left-padded with zeros to exactly 16 ASCII characters.
/// This payload is byte-identical to upstream `autossh 1.4g` per SC-007.
pub fn probe_payload(unix_ts: u64, message: Option<&str>) -> Vec<u8> {
    match message {
        None => format!("{unix_ts:016}\n").into_bytes(),
        Some(msg) => format!("{unix_ts:016} {msg}\n").into_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_payload_without_message_is_17_bytes() {
        let bytes = probe_payload(1_748_000_000, None);
        assert_eq!(bytes.len(), 17);
        // 16 ASCII digits + newline. `{:016}` left-pads with zeros so
        // 1_748_000_000 (10 digits) becomes `0000001748000000`.
        assert_eq!(&bytes[..16], b"0000001748000000");
        assert_eq!(bytes[16], b'\n');
    }

    #[test]
    fn probe_payload_with_message_uses_single_space_separator() {
        let bytes = probe_payload(1_748_000_000, Some("hello"));
        let expected = b"0000001748000000 hello\n";
        assert_eq!(bytes, expected);
    }

    #[test]
    fn probe_payload_timestamp_left_pads_to_16_chars() {
        let bytes = probe_payload(42, None);
        assert_eq!(&bytes[..16], b"0000000000000042");
        assert_eq!(bytes[16], b'\n');
    }

    #[test]
    fn probe_payload_timestamp_at_16_digit_width_does_not_overflow() {
        // 9_999_999_999_999_999 is exactly 16 digits — no widening.
        let bytes = probe_payload(9_999_999_999_999_999, None);
        assert_eq!(&bytes[..16], b"9999999999999999");
        assert_eq!(bytes.len(), 17);
    }

    #[test]
    fn probe_loop_bind_rejects_embedded_newline_in_message() {
        let err = ProbeLoop::bind(
            &MonitorMode::Active {
                port: 0,
                echo: None,
            },
            Some("with\nnewline"),
        )
        .expect_err("embedded newline must be rejected");
        assert!(matches!(err, AutosshError::Internal(_)));
    }

    #[test]
    fn probe_loop_bind_rejects_monitor_mode_none() {
        let err = ProbeLoop::bind(&MonitorMode::None, None)
            .expect_err("MonitorMode::None must be rejected");
        assert!(matches!(err, AutosshError::Internal(_)));
    }
}
