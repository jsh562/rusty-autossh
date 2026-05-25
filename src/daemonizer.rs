//! Background/`-f` daemonization.
//!
//! Per AD-009 + AD-010 + HINT-005 this module exposes:
//!
//! - `daemonize_unix` (Unix-only) — wraps the `daemonize` 0.5 crate.
//! - `detach_windows` (Windows-only) — `CreateProcessW` with
//!   `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` self-respawn.
//!
//! Both entries are CLI-feature-gated. On the inactive platform the
//! function does not exist — callers must use `#[cfg(unix)]` /
//! `#[cfg(windows)]` to dispatch.

#[cfg(unix)]
use std::path::PathBuf;

use crate::AutosshError;

/// Unix: double-fork + setsid + close stdio per FR-020.
///
/// Wraps `daemonize::Daemonize::new()` with the standard configuration:
/// `chdir("/")`, `umask(0o027)`, stdio closed (or redirected to
/// `logfile` when supplied). The pidfile parameter is NOT written by
/// this function — the caller wires `PidfileGuard::write_pid` separately
/// per HINT-011 ordering (pidfile is written BEFORE daemonize on the
/// foreground process).
#[cfg(unix)]
pub fn daemonize_unix(
    _pidfile: Option<PathBuf>,
    logfile: Option<PathBuf>,
) -> Result<(), AutosshError> {
    use daemonize::Daemonize;

    let mut d = Daemonize::new().working_directory("/").umask(0o027);

    if let Some(p) = logfile {
        let stdout = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .map_err(|e| AutosshError::LogfileWrite {
                path: p.clone(),
                source: e,
            })?;
        let stderr = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&p)
            .map_err(|e| AutosshError::LogfileWrite {
                path: p.clone(),
                source: e,
            })?;
        d = d.stdout(stdout).stderr(stderr);
    }

    d.start().map_err(|e| AutosshError::Daemonize {
        reason: e.to_string(),
    })
}

/// Windows: re-spawn the binary as a detached child per FR-021 +
/// HINT-005 + Clarifications Q5.
///
/// Closes the supplied monitor-port `TcpListener`s BEFORE invoking
/// `CreateProcessW` so the detached child can re-bind the same ports
/// without `EADDRINUSE`. Inherits env via `lpEnvironment = null`. The
/// foreground process exits after `CreateProcessW` returns; the caller
/// (typically `main.rs`) returns immediately after this call to surrender
/// the console.
#[cfg(windows)]
pub fn detach_windows(
    listeners_to_close: Vec<tokio::net::TcpListener>,
) -> Result<(), AutosshError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        CREATE_NEW_PROCESS_GROUP, CreateProcessW, DETACHED_PROCESS, PROCESS_INFORMATION,
        STARTUPINFOW,
    };

    // Mark the detached child so it does NOT recurse into detach_windows
    // again per HINT-005 (the inherited env causes the child to see `-f`
    // in its argv; the marker tells the child to skip daemonize).
    // SAFETY: single-threaded boundary at process startup.
    unsafe {
        std::env::set_var("RUSTY_AUTOSSH_DETACHED", "1");
    }

    // Close listeners FIRST so the detached child can re-bind cleanly
    // per HINT-005 / Clarifications Q5.
    drop(listeners_to_close);

    // Resolve the path to the currently-running exe.
    let exe = std::env::current_exe().map_err(|e| AutosshError::Daemonize {
        reason: format!("current_exe failed: {e}"),
    })?;

    // Reconstruct the command line: exe path + argv[1..].
    // CreateProcessW requires a UTF-16 NUL-terminated mutable buffer.
    let mut cmdline_buf: Vec<u16> = Vec::new();
    let exe_wide: Vec<u16> = std::ffi::OsStr::new("\"")
        .encode_wide()
        .chain(exe.as_os_str().encode_wide())
        .chain(std::ffi::OsStr::new("\"").encode_wide())
        .collect();
    cmdline_buf.extend(exe_wide);
    for arg in std::env::args_os().skip(1) {
        cmdline_buf.push(u16::from(b' '));
        cmdline_buf.push(u16::from(b'"'));
        for ch in arg.encode_wide() {
            cmdline_buf.push(ch);
        }
        cmdline_buf.push(u16::from(b'"'));
    }
    cmdline_buf.push(0);

    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };

    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),
            cmdline_buf.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0, // bInheritHandles = FALSE
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut info,
        )
    };

    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(AutosshError::Daemonize {
            reason: format!("CreateProcessW failed: {err}"),
        });
    }

    // Close handles to the detached child — we don't track it.
    unsafe {
        CloseHandle(info.hProcess);
        CloseHandle(info.hThread);
    }

    Ok(())
}
