//! SSH child-process spawner.
//!
//! Per AD-011 + HINT-017 + HINT-003 this module owns:
//!
//! - [`resolve_ssh_path`] — `AUTOSSH_PATH` verbatim override, falling back
//!   to a left-to-right `PATH` walk.
//! - [`inject_monitor_forwards`] — prepend `-L`/`-R` to the user's ssh
//!   argv when `-M <port>` is supplied.
//! - [`spawn_ssh`] — build a `tokio::process::Command` and spawn the ssh
//!   child with a fresh process group on Unix.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use tokio::process::{Child, Command};

use crate::{AutosshError, MonitorMode};

/// Resolve the path to the ssh binary.
///
/// 1. When `autossh_path_env` is `Some(p)` → use that value VERBATIM (no
///    `PATH` fallback, no `PATHEXT` expansion). Matches upstream behavior.
/// 2. Otherwise walk `path_env` left-to-right, first-match-wins. On Unix
///    probe `<dir>/ssh`; on Windows probe `<dir>/ssh.exe` first, then
///    `<dir>/ssh` (MSYS/Cygwin compat per HINT-017).
/// 3. On miss return [`AutosshError::SshNotFound`] with the searched
///    directories enumerated.
pub fn resolve_ssh_path(
    autossh_path_env: Option<&OsStr>,
    path_env: Option<&OsStr>,
) -> Result<PathBuf, AutosshError> {
    if let Some(p) = autossh_path_env {
        let candidate = PathBuf::from(p);
        // Verbatim — no validation that it exists. Spawn-time errors map
        // to AutosshError::Io via the `#[from]` conversion if the path is
        // bad. This matches upstream `autossh` behavior (AUTOSSH_PATH is
        // trusted by the user).
        return Ok(candidate);
    }

    let Some(path) = path_env else {
        return Err(AutosshError::SshNotFound {
            searched: Vec::new(),
        });
    };

    let mut searched = Vec::new();
    for dir in std::env::split_paths(path) {
        searched.push(dir.clone());

        #[cfg(windows)]
        {
            let with_exe = dir.join("ssh.exe");
            if with_exe.is_file() {
                return Ok(with_exe);
            }
            let bare = dir.join("ssh");
            if bare.is_file() {
                return Ok(bare);
            }
        }

        #[cfg(unix)]
        {
            let bare = dir.join("ssh");
            if bare.is_file() {
                return Ok(bare);
            }
        }
    }

    Err(AutosshError::SshNotFound { searched })
}

/// Prepend monitor-port forward flags to the ssh argv per FR-006 + HINT-003.
///
/// - [`MonitorMode::Active`] with `echo: None` → prepend
///   `-L <port>:127.0.0.1:<port+1>` and `-R <port>:127.0.0.1:<port+1>`
///   (4 tokens total).
/// - [`MonitorMode::Active`] with `echo: Some(echo)` → prepend
///   `-L <port>:127.0.0.1:<echo>` only (no `-R`) per FR-004.
/// - [`MonitorMode::None`] → return `ssh_args` unchanged.
///
/// The user's argv-passthrough tokens remain after the injected forwards
/// in their original order.
pub fn inject_monitor_forwards(mode: &MonitorMode, ssh_args: &[String]) -> Vec<String> {
    match mode {
        MonitorMode::None => ssh_args.to_vec(),
        MonitorMode::Active { port, echo: None } => {
            let pair = format!("{port}:127.0.0.1:{}", port.saturating_add(1));
            let mut out = Vec::with_capacity(ssh_args.len() + 4);
            out.push("-L".to_string());
            out.push(pair.clone());
            out.push("-R".to_string());
            out.push(pair);
            out.extend(ssh_args.iter().cloned());
            out
        }
        MonitorMode::Active {
            port,
            echo: Some(echo),
        } => {
            let pair = format!("{port}:127.0.0.1:{echo}");
            let mut out = Vec::with_capacity(ssh_args.len() + 2);
            out.push("-L".to_string());
            out.push(pair);
            out.extend(ssh_args.iter().cloned());
            out
        }
    }
}

/// Spawn the ssh child with the given resolved path + argv.
///
/// On Unix the child is placed in a fresh process group via
/// `process_group(0)` so SIGTERM to the supervisor does not cascade
/// uncontrolled — the supervisor signals the child explicitly. On Windows
/// the child is spawned with `CREATE_NEW_PROCESS_GROUP` so
/// `GenerateConsoleCtrlEvent` can be targeted.
pub async fn spawn_ssh(ssh_path: &Path, args: &[String]) -> Result<Child, AutosshError> {
    let mut cmd = Command::new(ssh_path);
    cmd.args(args);

    #[cfg(unix)]
    {
        cmd.process_group(0);
    }

    #[cfg(windows)]
    {
        // CREATE_NEW_PROCESS_GROUP = 0x00000200 per
        // windows-sys::Win32::System::Threading. Use the literal to avoid
        // pulling windows-sys into the always-on dep tree.
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }

    let child = cmd.spawn()?;
    Ok(child)
}
