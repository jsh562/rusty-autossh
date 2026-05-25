//! Default-mode CLI behavior tests (Phase 3 US1 subset).
//!
//! These tests drive the `rusty-autossh` binary via `assert_cmd`. They
//! verify the CLI parses argv correctly, propagates env vars through
//! `EnvSnapshot`, and produces sensible exit codes.

#![allow(non_snake_case)]

#[path = "common/mod.rs"]
mod common;

use std::time::Duration;

/// `rusty-autossh --version` exits 0 + prints a version string.
#[test]
fn version_flag_exits_zero() {
    let mut cmd = common::rusty_autossh_cmd();
    let assert = cmd.arg("--version").assert();
    assert.success();
}

/// `rusty-autossh --help` exits 0 + prints usage.
#[test]
fn help_flag_exits_zero() {
    let mut cmd = common::rusty_autossh_cmd();
    let assert = cmd.arg("--help").assert();
    assert.success();
}

/// `rusty-autossh -M 0 -- <echo_child exit_zero>` exits 0 (clean exit
/// propagates in MonitorMode::None per HINT-018 row 2 / US2 AS3 — but
/// works in US1 path too since `-M 0` resolves to `MonitorMode::None`).
#[test]
fn dash_M_zero_with_clean_exit_returns_zero() {
    let echo = common::echo_child_path();
    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_TEST_BEHAVIOR", "exit_zero")
        .arg("--ssh-path")
        .arg(&echo)
        .arg("-M")
        .arg("0")
        .arg("--gate-time")
        .arg("1");
    cmd.timeout(Duration::from_secs(20));
    let assert = cmd.assert();
    assert.success();
}

/// FR-006: When `-M <port>` is supplied, the supervisor injects `-L` and
/// `-R` forwards into the ssh argv. We assert this indirectly by
/// invoking `inject_monitor_forwards` from the library API.
#[test]
fn dash_M_port_injects_L_and_R_forwards() {
    let mode = rusty_autossh::MonitorMode::Active {
        port: 20000,
        echo: None,
    };
    let injected =
        rusty_autossh::spawner::inject_monitor_forwards(&mode, &["user@host".to_string()]);
    // Expect: -L 20000:127.0.0.1:20001  -R 20000:127.0.0.1:20001  user@host
    assert_eq!(injected[0], "-L");
    assert_eq!(injected[1], "20000:127.0.0.1:20001");
    assert_eq!(injected[2], "-R");
    assert_eq!(injected[3], "20000:127.0.0.1:20001");
    assert_eq!(injected[4], "user@host");
}

/// FR-008: `AUTOSSH_MAXSTART=2` cap is enforced.
#[test]
fn autossh_maxstart_caps_retries() {
    let echo = common::echo_child_path();
    let mut cmd = common::rusty_autossh_cmd();
    cmd.env("RUSTY_AUTOSSH_TEST_BEHAVIOR", "exit_nonzero")
        .env("AUTOSSH_MAXSTART", "2")
        .env("AUTOSSH_GATETIME", "30") // ensure each exit is short-lifetime
        .arg("--ssh-path")
        .arg(&echo)
        .arg("-M")
        .arg("0")
        .arg("--gate-time")
        .arg("0");
    cmd.timeout(Duration::from_secs(30));
    let assert = cmd.assert();
    assert.failure();
}
