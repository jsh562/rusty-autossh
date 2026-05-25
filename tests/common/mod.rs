//! Shared integration-test helpers.
//!
//! Module rules:
//! - All file paths obtained via `sandbox()` (per-test `tempfile::TempDir`).
//! - No relative-path writes, no `$HOME` writes, no global temp sharing.
//! - `env_guard` RAII restores the prior env-var state on test exit (for
//!   `AUTOSSH_*`, `RUSTY_AUTOSSH_STRICT`, `AUTOSSH_PATH`).
//!
//! Per T037 + plan §Testing Strategy.

#![allow(dead_code)] // Different test files reference different helpers.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use assert_cmd::Command as AssertCommand;
use tempfile::TempDir;

/// Process-global mutex for tests that mutate environment variables.
/// Acquire `env_lock().lock()` at the top of any test that reads env
/// vars through the supervisor (because `std::env` is process-global).
pub fn env_lock() -> &'static Mutex<()> {
    static LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Construct an `assert_cmd::Command` for the `rusty-autossh` binary.
pub fn rusty_autossh_cmd() -> AssertCommand {
    AssertCommand::cargo_bin("rusty-autossh").expect("rusty-autossh bin built by cargo test")
}

/// Absolute path to the dev-only `echo_child` test binary (per HINT-009).
///
/// Used as `AUTOSSH_PATH=<path>` in integration tests that exercise the
/// monitor-port heartbeat without a real ssh server.
pub fn echo_child_path() -> PathBuf {
    // assert_cmd::cargo::cargo_bin honors the same target-dir resolution
    // for any `[[bin]]` declared in Cargo.toml.
    assert_cmd::cargo::cargo_bin("echo_child")
}

/// Per-test sandbox. Returns the `TempDir` (which deletes the directory
/// on Drop) and an owned `PathBuf` to its root.
///
/// Tests obtain ALL paths from this helper.
pub fn sandbox() -> (TempDir, PathBuf) {
    let td = tempfile::tempdir().expect("tempdir creation succeeds");
    let root = td.path().to_path_buf();
    (td, root)
}

/// Pidfile path inside the sandbox.
pub fn temp_pidfile_path(root: &Path) -> PathBuf {
    root.join("autossh.pid")
}

/// Logfile path inside the sandbox.
pub fn temp_logfile_path(root: &Path) -> PathBuf {
    root.join("autossh.log")
}

/// Per HINT-004: no substitution at v0.1.0. FR-051 requires literal
/// `autossh:` prefix preserved on Strict-mode stderr so upstream-peer
/// interop is unaffected. This helper is a passthrough — reserved for
/// future strip rules.
pub fn strip_for_snapshot(raw: &[u8]) -> Vec<u8> {
    raw.to_vec()
}

/// Byte-equality assertion with a friendly diff on failure.
pub fn assert_bytes_equal(actual: &[u8], expected: &[u8]) {
    if actual != expected {
        panic!(
            "byte mismatch\n  actual ({} bytes): {:?}\n  expected ({} bytes): {:?}",
            actual.len(),
            String::from_utf8_lossy(actual),
            expected.len(),
            String::from_utf8_lossy(expected),
        );
    }
}

/// RAII guard that restores the prior env-var state on Drop.
pub struct EnvGuard {
    key: String,
    prior: Option<OsString>,
}

impl EnvGuard {
    /// Set `key` to `val` (or unset if `None`) and capture the prior
    /// state for restoration on Drop.
    pub fn set(key: &str, val: Option<&str>) -> Self {
        let prior = std::env::var_os(key);
        match val {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        Self {
            key: key.to_string(),
            prior,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prior {
            Some(v) => unsafe { std::env::set_var(&self.key, v) },
            None => unsafe { std::env::remove_var(&self.key) },
        }
    }
}

/// Sugar for the common pattern of capturing one env-var override.
pub fn env_guard(key: &str, val: Option<&str>) -> EnvGuard {
    EnvGuard::set(key, val)
}
