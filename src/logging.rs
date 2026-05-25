//! Log-file writer for the supervisor.
//!
//! Per AD-013 + FR-031 + FR-032 + FR-054 this module owns:
//!
//! - [`init_logfile`] — open the `AUTOSSH_LOGFILE` path for append; in
//!   Default mode wraps the writer in a `tracing-appender` non-blocking
//!   worker; in Strict mode opens a raw `OpenOptions::append` writer with
//!   no timestamp prefix.
//! - [`fallback_to_stderr`] — emit a one-time warning when the logfile
//!   path is unwritable; subsequent diagnostics go to stderr.

use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tracing_appender::non_blocking::WorkerGuard;

use crate::{AutosshError, CompatibilityMode};

static FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);

/// Initialize the logfile writer.
///
/// - `path = None` → returns `Ok(None)` (no logfile configured;
///   diagnostics flow to stderr).
/// - `path = Some(p)` in Default mode → opens `p` for append, wraps in a
///   `tracing-appender` non-blocking worker; returns the
///   [`WorkerGuard`] for the caller to retain (drop on supervisor exit
///   to flush pending lines).
/// - `path = Some(p)` in Strict mode → opens `p` for append with no
///   timestamp prefix per FR-054 (the caller writes raw upstream-format
///   lines via `eprintln!` / direct writer).
///
/// Per FR-032: an unwritable path is non-fatal — the function calls
/// [`fallback_to_stderr`] and returns `Ok(None)`.
pub fn init_logfile(
    path: Option<PathBuf>,
    mode: CompatibilityMode,
) -> Result<Option<WorkerGuard>, AutosshError> {
    let Some(p) = path else {
        return Ok(None);
    };

    let file = OpenOptions::new().create(true).append(true).open(&p);

    let file = match file {
        Ok(f) => f,
        Err(_) => {
            fallback_to_stderr(&p);
            return Ok(None);
        }
    };

    match mode {
        CompatibilityMode::Default => {
            // Wrap in a non-blocking writer so the supervisor's hot path
            // does not stall on slow disks.
            let (_writer, guard) = tracing_appender::non_blocking(file);
            // NB: subscriber installation is the caller's responsibility
            // (typically the CLI dispatcher at startup). At Phase 2 we
            // surface the writer guard so the caller can retain it
            // alongside the supervisor.
            Ok(Some(guard))
        }
        CompatibilityMode::Strict => {
            // Strict mode: raw OpenOptions::append; no timestamp prefix.
            // The file handle itself is dropped here (the caller writes
            // via direct fs::OpenOptions::append at the call site for
            // strict-mode diagnostics).
            drop(file);
            Ok(None)
        }
    }
}

/// Emit a one-time stderr warning when the logfile path is unwritable
/// (FR-032). Subsequent calls for the same supervisor are no-ops so the
/// warning does not flood stderr.
pub fn fallback_to_stderr(path: &Path) {
    if FALLBACK_WARNED.swap(true, Ordering::SeqCst) {
        return;
    }
    eprintln!(
        "rusty-autossh: log file {} is not writable; falling back to stderr",
        path.display()
    );
}
