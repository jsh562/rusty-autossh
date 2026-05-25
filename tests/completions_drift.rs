//! US7 (Shell completions) drift gate (T137 + T138 + T139).
//!
//! Asserts the committed `completions/` artifacts match what
//! `clap_complete` generates today. Regenerate via:
//!
//! ```sh
//! cargo run -- completions bash       > completions/rusty-autossh.bash
//! cargo run -- completions zsh        > completions/_rusty-autossh
//! cargo run -- completions fish       > completions/rusty-autossh.fish
//! cargo run -- completions powershell > completions/rusty-autossh.ps1
//! ```
//!
//! On intentional flag additions, the developer regenerates the four
//! files locally and commits the refresh in the same PR
//! (plan §Shell Completions Drift Gate).

mod common;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use predicates::prelude::*;

/// Read the committed completion file for the given shell from
/// `completions/` and normalize CRLF→LF so the comparison is platform-
/// neutral (Windows checkouts may rewrite EOLs despite the `.gitattributes`
/// `eol=lf` directive when committed via a non-Git-aware editor).
fn committed(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("completions")
        .join(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("missing committed file {path:?}: {e}"));
    normalize(&bytes)
}

/// Invoke the rusty-autossh binary's `completions <shell>` subcommand,
/// capture stdout, and normalize CRLF→LF.
fn generate(shell: &str) -> Vec<u8> {
    let output = common::rusty_autossh_cmd()
        .arg("completions")
        .arg(shell)
        .output()
        .expect("completions subcommand runs");
    assert!(
        output.status.success(),
        "completions {shell} exited non-zero: {:?}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    normalize(&output.stdout)
}

/// Strip `\r` bytes so a Windows-host run that emits CRLF line endings
/// compares cleanly against a `.gitattributes`-enforced LF-only file.
fn normalize(bytes: &[u8]) -> Vec<u8> {
    bytes.iter().copied().filter(|b| *b != b'\r').collect()
}

// ===========================================================================
// T137 — byte-equal drift gate (4 shells)
// ===========================================================================

#[test]
fn drift_bash() {
    assert_eq!(
        committed("rusty-autossh.bash"),
        generate("bash"),
        "bash completion drift — regenerate via `cargo run -- completions bash > completions/rusty-autossh.bash`"
    );
}

#[test]
fn drift_zsh() {
    assert_eq!(
        committed("_rusty-autossh"),
        generate("zsh"),
        "zsh completion drift — regenerate via `cargo run -- completions zsh > completions/_rusty-autossh`"
    );
}

#[test]
fn drift_fish() {
    assert_eq!(
        committed("rusty-autossh.fish"),
        generate("fish"),
        "fish completion drift — regenerate via `cargo run -- completions fish > completions/rusty-autossh.fish`"
    );
}

#[test]
fn drift_powershell() {
    assert_eq!(
        committed("rusty-autossh.ps1"),
        generate("powershell"),
        "powershell completion drift — regenerate via `cargo run -- completions powershell > completions/rusty-autossh.ps1`"
    );
}

// ===========================================================================
// T138 — bash completion structural sanity per US7 AS2
// ===========================================================================
//
// Per US7 AS2 the bash completion script MUST be functionally complete:
// the `complete -F _rusty-autossh` registration MUST be present, every
// long flag MUST be listed under `opts`, and the `completions`
// subcommand MUST list all four supported shells. The assertion below
// codifies that structural contract so an accidental clap-derive
// refactor that silently drops a flag fails CI alongside the byte-equal
// drift gate.

#[test]
fn bash_completion_is_structurally_complete() {
    let bash =
        String::from_utf8(committed("rusty-autossh.bash")).expect("bash completion is valid UTF-8");

    // The `complete -F` registration that hooks the script onto the
    // `rusty-autossh` command name MUST be present (otherwise the script
    // is inert when sourced).
    assert!(
        bash.contains("complete -F _rusty-autossh") && bash.contains(" rusty-autossh"),
        "bash completion missing `complete -F _rusty-autossh … rusty-autossh` registration"
    );

    // Top-level `-M` / `--monitor-port` MUST appear in the opts list so
    // users get tab-completion of the flag itself (per US7 AS2 + spec).
    assert!(
        bash.contains("--monitor-port"),
        "bash completion missing --monitor-port"
    );
    assert!(bash.contains("-M"), "bash completion missing -M");

    // Other primary autossh long flags MUST appear so the completion
    // surface is functionally complete per US7 AS2.
    for flag in [
        "--background",
        "--one-shot",
        "--poll",
        "--first-poll",
        "--gate-time",
        "--max-start",
        "--max-lifetime",
        "--ssh-path",
        "--pid-file",
        "--log-file",
        "--strict",
        "--no-strict",
    ] {
        assert!(
            bash.contains(flag),
            "bash completion missing long flag `{flag}`"
        );
    }

    // The completions subcommand itself MUST be listed and MUST expose
    // all four shells we support (clap_complete may also emit `elvish`;
    // we accept-but-do-not-require it).
    assert!(
        bash.contains("completions"),
        "bash completion missing `completions` subcommand"
    );
    for shell in ["bash", "zsh", "fish", "powershell"] {
        assert!(
            bash.contains(shell),
            "bash completion missing shell name `{shell}` under `completions` subcommand"
        );
    }
}

// ===========================================================================
// T139 — Strict mode rejects `completions` (cross-ref T082)
// ===========================================================================

/// Per Clarifications Q3 + US7 AS3 + FR-053: Strict mode is byte-for-byte
/// upstream `autossh 1.4g` and therefore does NOT recognize the
/// rusty-autossh `completions <shell>` subcommand. The Strict-mode
/// dispatcher in `main::run_strict` translates the token into the
/// upstream-format `unrecognized option` diagnostic and exits 1.
///
/// This test cross-references the Phase 5 T082 wiring; a regression
/// either there or in the strict parser will surface here as well.
#[test]
fn strict_mode_rejects_completions_subcommand() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("completions").arg("bash");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    let output = assert
        .failure()
        .code(1)
        .stderr(predicate::str::contains(
            "autossh: unrecognized option 'completions'",
        ))
        .get_output()
        .clone();

    // No completion script bytes leaked to stdout.
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("complete -F")
            && !stdout.contains("_rusty-autossh()")
            && !stdout.contains("#compdef"),
        "Strict mode must NOT emit a completion script; got stdout: {stdout:?}"
    );
}
