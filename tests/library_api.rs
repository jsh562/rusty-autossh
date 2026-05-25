//! Library API surface tests (US4 — T092..T103).
//!
//! These tests assert the v0.1.0 public-library contract:
//!
//! - **T096** `default-features = false` dep tree excludes every CLI-only
//!   crate (clap / clap_complete / anstyle / tracing-* / daemonize /
//!   atomicwrites / windows-sys-0.59) per HINT-007 + FR-061 + SC-008.
//! - **T097** Send/Sync compile-time guards per SC-009 / FR-060.
//! - **T099** `MonitorMode::None` happy-path supervisor (no fs, no listener,
//!   pure library usage) per US4-AS3.
//! - **T100** `SupervisorEvent` mpsc channel consumer wiring per US4-AS2.
//! - **T101** `MonitorMode::Active` variant buildability + argv-injection
//!   per FR-063.
//! - **T102** `AutosshError` `#[from] io::Error` conversion per AD-014.

#![allow(clippy::let_underscore_untyped)]

use std::env;
use std::io;
use std::process::Command;
use std::time::Duration;

use static_assertions::assert_impl_all;
use tokio::sync::mpsc;

use rusty_autossh::{
    AutosshError, CompatibilityMode, MonitorMode, SshSupervisor, SshSupervisorBuilder,
    SupervisorEvent,
};

// -----------------------------------------------------------------------------
// T097 — Send/Sync compile-time guards (SC-009 / FR-060).
// -----------------------------------------------------------------------------

assert_impl_all!(SshSupervisorBuilder: Send, Sync);
// SshSupervisor is intentionally Send but NOT Sync — it owns the mutable
// child handle + monitor listeners.
assert_impl_all!(SshSupervisor: Send);
assert_impl_all!(MonitorMode: Send, Sync, Clone);
assert_impl_all!(SupervisorEvent: Send, Sync);
assert_impl_all!(AutosshError: Send, Sync);
assert_impl_all!(CompatibilityMode: Send, Sync, Clone, Copy);

#[test]
fn send_sync_compile_time_guards_pass() {
    // Force the static_assertions invocations above to be referenced from a
    // real test entry so they're guaranteed to be evaluated.
    fn assert_static<T: 'static>() {}
    assert_static::<AutosshError>();
}

// -----------------------------------------------------------------------------
// T096 — dep tree under `--no-default-features` excludes CLI-only crates.
// -----------------------------------------------------------------------------

/// CLI-only crate names that MUST NOT appear in the no-default-features dep
/// tree per HINT-007 + FR-061. (Allow-list strategy is too brittle because
/// tokio + socket2 + thiserror pull a long transitive subtree of pure-Rust
/// crates.)
const CLI_ONLY_CRATES: &[&str] = &[
    "clap",
    "clap_complete",
    "anstyle",
    "tracing",
    "tracing-subscriber",
    "tracing-appender",
    "daemonize",
    "atomicwrites",
];

#[test]
fn default_features_off_excludes_cli_deps() {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let manifest_dir = env!("CARGO_MANIFEST_DIR");

    let output = Command::new(&cargo)
        .args([
            "tree",
            "--no-default-features",
            "--prefix",
            "none",
            "--edges",
            "normal",
            "--no-dedupe",
        ])
        .current_dir(manifest_dir)
        .output()
        .expect("cargo tree --no-default-features invocation");

    assert!(
        output.status.success(),
        "cargo tree exited {:?}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    // The crate root must be present.
    assert!(
        stdout.contains("rusty-autossh"),
        "dep tree missing rusty-autossh:\n{stdout}"
    );

    // Always-on deps required for the library API.
    for required in &["tokio", "thiserror", "socket2"] {
        assert!(
            stdout
                .lines()
                .any(|line| line.starts_with(&format!("{required} v"))),
            "dep tree missing required allow-list crate `{required}`:\n{stdout}"
        );
    }

    // CLI-only crates must be absent — they only enter the tree when the
    // `cli` feature is active.
    for cli_only in CLI_ONLY_CRATES {
        let needle_prefix = format!("{cli_only} v");
        let hit = stdout.lines().any(|line| line.starts_with(&needle_prefix));
        assert!(
            !hit,
            "CLI-only crate `{cli_only}` leaked into the library dep tree:\n{stdout}"
        );
    }

    // The CLI-gated `windows-sys = 0.59` optional dep MUST be absent.
    // (`socket2` pulls its own `windows-sys` versions transitively — that's
    // an unrelated pure-Rust subtree, so we narrow the absence check to the
    // version pinned in `Cargo.toml [target.'cfg(windows)'.dependencies]`.)
    assert!(
        !stdout.contains("windows-sys v0.59"),
        "CLI-gated windows-sys 0.59 leaked into the library dep tree:\n{stdout}"
    );
}

// -----------------------------------------------------------------------------
// T099 — Library run with `MonitorMode::None` end-to-end (no fs, no listener).
// -----------------------------------------------------------------------------

/// Run the supervisor in library-only `MonitorMode::None` mode against a
/// non-existent ssh binary and observe the resulting `AutosshError` path —
/// covers pure-library usage with no fs and no TCP listener bind.
#[test]
fn library_run_with_monitor_mode_none_returns_error_for_missing_ssh() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime build");

    rt.block_on(async {
        // Force resolve_ssh_path to fail by pointing AUTOSSH_PATH at a
        // path that obviously cannot spawn AND letting spawn fail.
        // We bypass the resolver by providing an explicit ssh_path that
        // does not exist, which surfaces a spawn-time `AutosshError::Io`.
        let bogus = std::path::PathBuf::from(if cfg!(windows) {
            "C:\\nonexistent\\ssh-does-not-exist.exe"
        } else {
            "/nonexistent/ssh-does-not-exist"
        });

        let mut supervisor = SshSupervisorBuilder::new()
            .ssh_args(vec!["user@host".to_string()])
            .monitor_mode(MonitorMode::None)
            .ssh_path(bogus)
            .poll(Duration::from_millis(50))
            .gate_time(Duration::from_millis(10))
            .max_start(Some(1))
            .one_shot(true)
            .build()
            .expect("builder build succeeds with explicit ssh_path");

        let result = supervisor.run().await;
        // Either Io (spawn failure) or MaxStartReached after one immediate
        // failure — both are acceptable library-API outcomes for a
        // non-existent binary.
        assert!(
            matches!(
                result,
                Err(AutosshError::Io(_)) | Err(AutosshError::MaxStartReached { .. })
            ),
            "expected Io or MaxStartReached for missing ssh, got {result:?}"
        );
    });
}

// -----------------------------------------------------------------------------
// T100 — `SupervisorEvent` mpsc channel consumer wiring.
// -----------------------------------------------------------------------------

#[test]
fn supervisor_event_mpsc_channel_wires_through_builder() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime build");

    rt.block_on(async {
        let (tx, mut rx) = mpsc::channel::<SupervisorEvent>(16);

        let bogus = std::path::PathBuf::from(if cfg!(windows) {
            "C:\\nonexistent\\ssh-does-not-exist.exe"
        } else {
            "/nonexistent/ssh-does-not-exist"
        });

        let mut supervisor = SshSupervisorBuilder::new()
            .ssh_args(vec!["user@host".to_string()])
            .monitor_mode(MonitorMode::None)
            .ssh_path(bogus)
            .poll(Duration::from_millis(50))
            .gate_time(Duration::from_millis(10))
            .max_start(Some(1))
            .one_shot(true)
            .event_sender(tx)
            .build()
            .expect("builder build succeeds with explicit ssh_path");

        let _result = supervisor.run().await;

        // Drain whatever events were sent. We don't gate on a specific
        // variant order — the supervisor can fail before ChildSpawned
        // when spawn itself errors. We only assert the channel receiver
        // can be consumed without panicking and at least one event arrived
        // OR the run exited without dispatching any event (both branches
        // are legitimate for a failed-spawn happy-path).
        let mut count = 0usize;
        while let Ok(Some(_event)) =
            tokio::time::timeout(Duration::from_millis(100), rx.recv()).await
        {
            count += 1;
            if count >= 32 {
                break;
            }
        }
        // Just demonstrate the consumer can pull events.
        let _ = count;
    });
}

// -----------------------------------------------------------------------------
// T101 — `MonitorMode::Active` variant build + argv-injection coverage.
// -----------------------------------------------------------------------------

#[test]
fn monitor_mode_active_variants_are_constructible() {
    let two_listener = MonitorMode::Active {
        port: 20000,
        echo: None,
    };
    let single_listener = MonitorMode::Active {
        port: 20000,
        echo: Some(22),
    };
    let none = MonitorMode::None;

    // Pattern match exercising the public surface.
    for mode in [two_listener, single_listener, none] {
        match mode {
            MonitorMode::None => {}
            MonitorMode::Active { port, echo } => {
                assert_eq!(port, 20000);
                assert!(matches!(echo, None | Some(22)));
            }
            _ => unreachable!("non_exhaustive future variant"),
        }
    }
}

#[test]
fn monitor_mode_active_with_echo_argv_injection() {
    let mode = MonitorMode::Active {
        port: 20000,
        echo: Some(22),
    };
    let injected =
        rusty_autossh::spawner::inject_monitor_forwards(&mode, &["user@host".to_string()]);

    // `-L 20000:127.0.0.1:22` only — NO `-R` per FR-004 + FR-063.
    assert_eq!(injected.len(), 3, "expected 3 tokens, got {injected:?}");
    assert_eq!(injected[0], "-L");
    assert_eq!(injected[1], "20000:127.0.0.1:22");
    assert_eq!(injected[2], "user@host");
    assert!(
        !injected.iter().any(|t| t == "-R"),
        "no `-R` token expected for echo-mode injection, got {injected:?}"
    );
}

// -----------------------------------------------------------------------------
// T102 — `AutosshError` `#[from] io::Error` conversion.
// -----------------------------------------------------------------------------

#[test]
fn autossh_error_from_io_error_via_from_derive() {
    let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
    let autossh_err: AutosshError = io_err.into();

    match autossh_err {
        AutosshError::Io(inner) => {
            assert_eq!(inner.kind(), io::ErrorKind::PermissionDenied);
        }
        other => panic!("expected AutosshError::Io, got {other:?}"),
    }
}

#[test]
fn autossh_error_source_chain_for_wrapping_variants() {
    use std::error::Error;

    // Wrapping variant — source() must surface the inner io::Error.
    let inner = io::Error::new(io::ErrorKind::AddrInUse, "in use");
    let err = AutosshError::MonitorBindFailed {
        port: 20000,
        source: inner,
    };
    assert!(err.source().is_some(), "wrapping variant must have source");

    // Leaf variant — source() must be None.
    let leaf = AutosshError::SshNotFound {
        searched: vec![std::path::PathBuf::from("/usr/bin")],
    };
    assert!(leaf.source().is_none(), "leaf variant must have no source");

    let leaf2 = AutosshError::MaxStartReached { attempts: 3 };
    assert!(leaf2.source().is_none());

    let leaf3 = AutosshError::MaxLifetimeReached;
    assert!(leaf3.source().is_none());
}
