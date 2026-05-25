//! Atomic pidfile writer with RAII cleanup.
//!
//! Per AD-012 + HINT-010 + FR-030 this module exposes
//! [`PidfileGuard`] — a Drop guard that removes the pidfile on supervisor
//! termination (clean exit, SIGTERM, panic via stack unwinding) so a
//! restarted supervisor never sees a stale pidfile from this run.
//!
//! The pidfile contents are written atomically (tempfile + rename) via
//! the `atomicwrites` crate so a process crash mid-write does not leave a
//! truncated/empty pidfile behind.
//!
//! Documented limitation: SIGKILL to the supervisor leaves a stale
//! pidfile (Drop cannot run). Matches upstream `autossh` behavior;
//! enumerated in `COMPATIBILITY.md`.

use std::io::Write;
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};

use crate::AutosshError;

/// RAII guard for the pidfile.
///
/// Removes the pidfile on `Drop`. Cleanup runs on clean exit, SIGTERM
/// (the runtime's shutdown path drops the guard), and panic via stack
/// unwinding. Does NOT run on SIGKILL.
#[derive(Debug)]
pub struct PidfileGuard {
    /// Absolute path to the pidfile being managed.
    pub path: PathBuf,
}

/// Write the supervisor PID atomically to `path`.
///
/// Uses `atomicwrites::AtomicFile` with `AllowOverwrite::Yes` so a stale
/// pidfile from a prior crashed run is replaced cleanly at startup
/// (matches upstream `autossh` — pidfile contention is resolved by
/// overwrite, not by refusal to start).
///
/// Returns a [`PidfileGuard`] whose `Drop` impl removes the file on
/// supervisor termination.
pub fn write_pid(path: PathBuf, pid: u32) -> Result<PidfileGuard, AutosshError> {
    let af = AtomicFile::new(&path, AllowOverwrite);
    af.write(|f| writeln!(f, "{pid}")).map_err(|e| match e {
        atomicwrites::Error::Internal(io_err) => AutosshError::PidfileWrite {
            path: path.clone(),
            source: io_err,
        },
        atomicwrites::Error::User(io_err) => AutosshError::PidfileWrite {
            path: path.clone(),
            source: io_err,
        },
    })?;

    Ok(PidfileGuard { path })
}

impl Drop for PidfileGuard {
    fn drop(&mut self) {
        // Best-effort removal. Failure to remove is non-fatal (e.g. the
        // pidfile was manually deleted between write and drop) — we
        // already documented the SIGKILL stale-pidfile case in
        // COMPATIBILITY.md.
        let _ = std::fs::remove_file(&self.path);
    }
}

impl PidfileGuard {
    /// Borrow the managed path.
    pub fn path(&self) -> &Path {
        &self.path
    }
}
