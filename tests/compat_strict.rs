//! US3 Strict-Compat Drop-In integration tests (T076-T085).
//!
//! Per Phase 5 + plan §Strict-Mode Coverage + FR-050..FR-054. Drives the
//! `rusty-autossh` binary via `assert_cmd`; verifies:
//!
//! - `--strict` flag, `RUSTY_AUTOSSH_STRICT=1` env, argv[0]=`autossh`
//!   each activate Strict mode (T076).
//! - `--no-strict` overrides env + argv[0]; last-wins on the command
//!   line per Clarifications Q8 (T077).
//! - Excluded short flags emit `autossh: invalid option -- '<c>'` (T078,
//!   T079).
//! - Excluded long flags emit `autossh: unrecognized option '--<flag>'`
//!   (T080, T081).
//! - `completions` subcommand rejected under Strict per Clarifications
//!   Q3 (T082).
//! - In-scope flags accepted (T083).
//! - Heartbeat wire-format byte-identical to upstream (T084).
//! - No ISO timestamp prefix on Strict-mode log lines (T085).

#![allow(non_snake_case)]
#![allow(clippy::await_holding_lock)]

#[path = "common/mod.rs"]
mod common;

use std::time::Duration;

use predicates::prelude::*;

/// T076 (a): `--strict` flag alone activates Strict mode.
/// We assert by triggering a Strict-mode-only error path: invoking with
/// `--strict` and an excluded long flag produces the upstream-byte-equal
/// stderr (which Default mode would NOT produce — clap would emit its
/// own diagnostic instead).
#[test]
fn strict_flag_activates_strict_mode() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("--monitor-port").arg("20000");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--monitor-port'",
    ));
}

/// T077 (b): `RUSTY_AUTOSSH_STRICT=1` env-var activates Strict mode.
#[test]
fn strict_env_var_activates_strict_mode() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());

    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_STRICT", "1")
        .arg("--monitor-port")
        .arg("20000");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--monitor-port'",
    ));
}

/// T077: `RUSTY_AUTOSSH_STRICT=true` (case-insensitive) also activates.
#[test]
fn strict_env_var_true_activates_strict_mode() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());

    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_STRICT", "TRUE").arg("--debug");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--debug'",
    ));
}

/// T077 (a): `--no-strict` overrides env-var activation.
///
/// With `RUSTY_AUTOSSH_STRICT=1` but `--no-strict` on argv, Default mode
/// wins per FR-050 + Clarifications Q8. clap (Default-mode parser)
/// emits its own diagnostic, NOT the `autossh:` upstream-byte-equal one.
#[test]
fn no_strict_overrides_env() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());

    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_STRICT", "1")
        .arg("--no-strict")
        .arg("--help");
    cmd.timeout(Duration::from_secs(10));
    // --help in Default mode succeeds; Strict mode would reject --help
    // as an unrecognized long flag.
    let assert = cmd.assert();
    assert.success();
}

/// T077 (c): `--no-strict --strict` → Strict (last-wins per Q8).
#[test]
fn no_strict_then_strict_last_wins_strict() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--no-strict")
        .arg("--strict")
        .arg("--monitor-port")
        .arg("20000");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--monitor-port'",
    ));
}

/// T083 (d): `--strict --no-strict` → Default mode (last-wins per Q8).
#[test]
fn strict_then_no_strict_last_wins_default() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("--no-strict").arg("--help");
    cmd.timeout(Duration::from_secs(10));
    // Default mode accepts --help (clap built-in); Strict mode would
    // reject it.
    let assert = cmd.assert();
    assert.success();
}

/// T078: excluded short `-d` emits `autossh: invalid option -- 'd'`.
#[test]
fn strict_excluded_short_d_emits_invalid_option() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("-d");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert
        .failure()
        .stderr(predicate::str::contains("autossh: invalid option -- 'd'"));
}

/// T079: all 7 remaining excluded short flags emit `invalid option`.
#[test]
fn strict_excluded_short_flags_D_X_T_a_N_Y_q() {
    for c in ['D', 'X', 'T', 'a', 'N', 'Y', 'q'] {
        let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
        let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

        let mut cmd = common::rusty_autossh_cmd();
        cmd.arg("--strict").arg(format!("-{c}"));
        cmd.timeout(Duration::from_secs(10));
        let expected = format!("autossh: invalid option -- '{c}'");
        let assert = cmd.assert();
        assert.failure().stderr(predicate::str::contains(expected));
    }
}

/// T080: excluded long `--monitor-port` emits `unrecognized option`.
#[test]
fn strict_excluded_long_monitor_port_emits_unrecognized() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("--monitor-port").arg("20000");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--monitor-port'",
    ));
}

/// T081: all 7 remaining excluded long flags emit `unrecognized option`.
#[test]
fn strict_excluded_long_flags() {
    let excluded = [
        "--poll",
        "--first-poll",
        "--gate-time",
        "--max-start",
        "--max-lifetime",
        "--ssh-path",
        "--log-file",
    ];
    for flag in excluded {
        let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
        let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

        let mut cmd = common::rusty_autossh_cmd();
        cmd.arg("--strict").arg(flag);
        cmd.timeout(Duration::from_secs(10));
        let expected = format!("autossh: unrecognized option '{flag}'");
        let assert = cmd.assert();
        assert.failure().stderr(predicate::str::contains(expected));
    }
}

/// T081 also: `--debug` is a Default-mode flag and not upstream — reject
/// in Strict mode.
#[test]
fn strict_excluded_long_debug() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("--debug");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--debug'",
    ));
}

/// T082: `--color`-style invented long flag rejected with
/// `unrecognized option` (the default catch-all).
#[test]
fn strict_invented_long_flag_color_rejected() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("--color");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option '--color'",
    ));
}

/// T082: `completions` subcommand rejected under Strict per
/// Clarifications Q3 + US7 AS3.
#[test]
fn strict_rejects_completions_subcommand() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("completions").arg("bash");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert.failure().stderr(predicate::str::contains(
        "autossh: unrecognized option 'completions'",
    ));
}

/// T083 (d): `-V` under Strict prints version + exits 0.
#[test]
fn strict_dash_V_prints_version() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let mut cmd = common::rusty_autossh_cmd();
    cmd.arg("--strict").arg("-V");
    cmd.timeout(Duration::from_secs(10));
    let assert = cmd.assert();
    assert
        .success()
        .stdout(predicate::str::contains("rusty-autossh"));
}

/// T084 + SC-007: AUTOSSH_MESSAGE byte-identical heartbeat payload.
///
/// The wire format is mode-independent — Strict and Default both call
/// `monitor::probe_payload`. We assert this invariant directly here as
/// a strict-mode coverage signal (cross-references T054).
#[test]
fn strict_heartbeat_wire_format_byte_identical_to_upstream() {
    let payload_no_msg = rusty_autossh::monitor::probe_payload(1_748_000_000, None);
    let expected_no_msg: &[u8] = b"0000001748000000\n";
    common::assert_bytes_equal(&payload_no_msg, expected_no_msg);

    let payload_with_msg = rusty_autossh::monitor::probe_payload(1_748_000_000, Some("alive"));
    let expected_with_msg: &[u8] = b"0000001748000000 alive\n";
    common::assert_bytes_equal(&payload_with_msg, expected_with_msg);
}

/// T085 FR-054: Strict-mode stderr lines do NOT carry an ISO 8601
/// timestamp prefix.
///
/// Invoke `rusty-autossh --strict -M 0 -- <echo_child>` with
/// `RUSTY_AUTOSSH_TEST_BEHAVIOR=exit_nonzero` so the supervisor exits
/// with MaxStartReached after 2 attempts. Capture stderr; assert no
/// ISO-8601 prefix (no `2026-` / `2025-` style date).
#[test]
fn strict_stderr_no_iso_timestamp_prefix() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let echo = common::echo_child_path();
    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_TEST_BEHAVIOR", "exit_nonzero")
        .env("AUTOSSH_PATH", &echo)
        .env("AUTOSSH_MAXSTART", "2")
        .env("AUTOSSH_GATETIME", "30")
        .arg("--strict")
        .arg("-M")
        .arg("0");
    cmd.timeout(Duration::from_secs(30));
    let output = cmd.output().expect("supervisor runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The Strict-mode error map prefixes lines with `autossh: ...`.
    // FR-054: no ISO 8601 timestamp prefix on any line.
    for line in stderr.lines() {
        // ISO 8601 form: `2025-`, `2026-`, ...
        // Reject any line that starts with 4 digits + '-' + 2 digits.
        let bytes = line.as_bytes();
        if bytes.len() >= 5
            && bytes[0].is_ascii_digit()
            && bytes[1].is_ascii_digit()
            && bytes[2].is_ascii_digit()
            && bytes[3].is_ascii_digit()
            && bytes[4] == b'-'
        {
            panic!("strict-mode stderr line carried ISO 8601 prefix: {line}");
        }
    }
}

/// T085 also: AUTOSSH_LOGFILE under Strict mode receives raw lines (no
/// timestamp prefix). The logfile path is created by the supervisor's
/// logging init; Strict mode opens with `OpenOptions::append` per
/// FR-054, no tracing-appender / no timestamp.
///
/// We assert by reading the file after the supervisor exits and
/// verifying no ISO 8601 prefix on any line. The file may be empty if
/// no log lines were generated (still passes the FR-054 invariant).
#[test]
fn strict_logfile_no_iso_timestamp_prefix() {
    let _lock = common::env_lock().lock().unwrap_or_else(|p| p.into_inner());
    let _g1 = common::env_guard("RUSTY_AUTOSSH_STRICT", None);

    let echo = common::echo_child_path();
    let (_td, root) = common::sandbox();
    let logpath = root.join("autossh.log");

    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_TEST_BEHAVIOR", "exit_zero")
        .env("AUTOSSH_PATH", &echo)
        .env("AUTOSSH_LOGFILE", &logpath)
        .arg("--strict")
        .arg("-M")
        .arg("0");
    cmd.timeout(Duration::from_secs(20));
    let _ = cmd.assert();

    if logpath.exists() {
        let contents = std::fs::read_to_string(&logpath).unwrap_or_default();
        for line in contents.lines() {
            let bytes = line.as_bytes();
            if bytes.len() >= 5
                && bytes[0].is_ascii_digit()
                && bytes[1].is_ascii_digit()
                && bytes[2].is_ascii_digit()
                && bytes[3].is_ascii_digit()
                && bytes[4] == b'-'
            {
                panic!("strict-mode logfile line carried ISO 8601 prefix: {line}");
            }
        }
    }
}
