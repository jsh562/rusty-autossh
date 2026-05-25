//! T141 — `cargo doc --no-deps` must succeed under both feature configurations,
//! which is the cross-check that `#![deny(missing_docs)]` at the crate root
//! (per FR-090) is being honored without any silent regression in
//! documentation coverage.
//!
//! This test shells out to `cargo doc --no-deps` twice:
//!
//! 1. `--no-default-features` — exercises the pure library surface (no CLI
//!    modules in scope). With `deny(missing_docs)` active, an undocumented
//!    public item would fail `cargo doc` here.
//! 2. `--all-features` — exercises the CLI-feature gated modules
//!    (`cli`, `daemonizer`, `pidfile`, `logging`).
//!
//! Closes SC-010 (every public type carries a rustdoc + at least one doctest;
//! `cargo doc` is the proof-of-coverage).
//!
//! References: spec FR-090, SC-010; tasks.md T141.

use std::env;
use std::process::Command;

/// Locate the `cargo` executable. Honors the `$CARGO` env var that
/// `cargo test` sets, otherwise falls back to PATH lookup.
fn cargo_binary() -> String {
    env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// Run `cargo doc --no-deps <extra>` from the manifest dir and assert it
/// exits 0. On failure, the captured stdout + stderr is dumped into the
/// test failure message so the regression is visible in CI logs.
fn run_cargo_doc(extra: &[&str]) {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let mut cmd = Command::new(cargo_binary());
    cmd.current_dir(manifest_dir)
        .arg("doc")
        .arg("--no-deps")
        .args(extra);

    let output = cmd
        .output()
        .expect("failed to invoke cargo doc; ensure cargo is on PATH");

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        panic!(
            "`cargo doc --no-deps {}` failed with status {:?}\n\
             stdout:\n{}\n\
             stderr:\n{}\n",
            extra.join(" "),
            output.status.code(),
            stdout,
            stderr
        );
    }
}

#[test]
fn cargo_doc_no_deps_succeeds_with_deny_missing_docs() {
    // Library-only surface: no CLI modules in scope. Catches missing docs on
    // the published-library types (SshSupervisor, MonitorMode, etc.).
    run_cargo_doc(&["--no-default-features"]);

    // Full surface: includes CLI-feature-gated modules. Catches missing docs
    // on `cli`, `daemonizer`, `pidfile`, `logging` pub items.
    run_cargo_doc(&["--all-features"]);
}
