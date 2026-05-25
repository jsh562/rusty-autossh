//! Dev-only test binary used as an `ssh` stand-in via `AUTOSSH_PATH`.
//!
//! Per HINT-009 + T038. The supervisor invokes this binary in place of
//! real ssh; the binary opens TCP listeners on ports supplied via
//! `RUSTY_AUTOSSH_TEST_ECHO_LISTEN` (comma-separated list) and echoes
//! payloads received on the `-L`-forwarded port back through the
//! `-R`-forwarded port.
//!
//! `RUSTY_AUTOSSH_TEST_BEHAVIOR` controls the simulated ssh behavior:
//! - `ok` (default): accept connections + echo payloads indefinitely.
//! - `drop_after_n`: read `RUSTY_AUTOSSH_TEST_DROP_AFTER` payloads then
//!   close the listener (simulates tunnel drop).
//! - `exit_zero`: immediately exit with status 0.
//! - `exit_nonzero`: immediately exit with status 1.
//! - `hang`: sleep forever; never accept connections (simulates frozen
//!   child).
//! - `ignore_sigterm` (Unix): install a SIG_IGN handler for SIGTERM then
//!   sleep forever; used by T130 to exercise the 10s grace + SIGKILL
//!   fallback path in `Supervisor::terminate_child`.
//! - `segfault`: panic to simulate crash.
//!
//! Built as `[[bin]]` `echo_child` per Cargo.toml T007; `test = false`
//! prevents inclusion in the lib-test scope.

use std::env;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    let behavior = env::var("RUSTY_AUTOSSH_TEST_BEHAVIOR").unwrap_or_else(|_| "ok".to_string());

    match behavior.as_str() {
        "exit_zero" => return ExitCode::from(0),
        "exit_nonzero" => return ExitCode::from(1),
        "hang" => loop {
            thread::sleep(Duration::from_secs(60));
        },
        "ignore_sigterm" => {
            // Unix-only: install SIG_IGN for SIGTERM so the supervisor's
            // 10s grace window in `Supervisor::terminate_child` must
            // escalate to SIGKILL (HINT-015 path d). On Windows the
            // process has no SIGTERM concept; the same env-var falls
            // through to a hang for symmetry.
            #[cfg(unix)]
            unsafe {
                let _ = libc_signal(15 /* SIGTERM */, 1 /* SIG_IGN */);
            }
            loop {
                thread::sleep(Duration::from_secs(60));
            }
        }
        "segfault" => {
            panic!("echo_child segfault behavior requested");
        }
        _ => {}
    }

    let drop_after: usize = env::var("RUSTY_AUTOSSH_TEST_DROP_AFTER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let listen_spec = env::var("RUSTY_AUTOSSH_TEST_ECHO_LISTEN").unwrap_or_default();
    if listen_spec.is_empty() {
        // Nothing to do — sleep so the supervisor can observe the child
        // running. Useful for tests that only care about the bin being
        // spawned, not the heartbeat round-trip.
        thread::sleep(Duration::from_secs(60));
        return ExitCode::from(0);
    }

    let ports: Vec<u16> = listen_spec
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    let mut listeners = Vec::new();
    for port in &ports {
        match TcpListener::bind(format!("127.0.0.1:{port}")) {
            Ok(l) => listeners.push(l),
            Err(e) => {
                eprintln!("echo_child: failed to bind 127.0.0.1:{port}: {e}");
                return ExitCode::from(2);
            }
        }
    }

    let mut handled = 0usize;
    for listener in listeners {
        let drop_local = drop_after;
        thread::spawn(move || {
            let mut local_count = 0usize;
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut buf = [0u8; 1024];
                let n = match s.read(&mut buf) {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                if n == 0 {
                    continue;
                }
                let _ = s.write_all(&buf[..n]);
                let _ = s.flush();
                local_count += 1;
                if local_count >= drop_local {
                    break;
                }
            }
        });
        handled = handled.saturating_add(1);
    }

    if behavior == "drop_after_n" && handled > 0 {
        thread::sleep(Duration::from_secs(60));
    } else {
        thread::sleep(Duration::from_secs(3600));
    }
    ExitCode::from(0)
}

#[cfg(unix)]
unsafe extern "C" {
    // Reuse the libc `signal(2)` entrypoint to avoid pulling in the
    // `libc` crate solely for the `ignore_sigterm` test behavior.
    // Signature: `void (*signal(int signum, void (*handler)(int)))(int)`;
    // we only need the `SIG_IGN` (1) sentinel as the handler value, so
    // the return type is unused.
    #[link_name = "signal"]
    fn libc_signal(signum: i32, handler: usize) -> usize;
}
