# Success-Criterion Coverage Matrix

Maps each `SC-###` from `specs/00010-autossh-port/spec.md` to the
integration / unit / library-API test that exercises it. Rows are
emitted by each user-story closure task (T061 / T069 / T091 / T103 /
T119 / T133 / T140 per `tasks.md`).

## Test isolation policy

- Every integration test owns a freshly-constructed `tempfile::TempDir`
  via the `sandbox()` helper in `tests/common/mod.rs`.
- Tests MUST NOT write to relative paths, MUST NOT write under `$HOME`
  (`%USERPROFILE%` on Windows), MUST NOT share a global mutable temp
  directory.
- Per-test `env_guard` RAII isolates mutations to `AUTOSSH_*`,
  `RUSTY_AUTOSSH_STRICT`, and `AUTOSSH_PATH` so `cargo test -- --test-threads=N`
  remains deterministic for any `N`.
- Signal-sensitive tests use `#[tokio::test(flavor = "current_thread")]`
  and `tokio::time::pause()` for poll-interval assertions (avoids real
  600 s `AUTOSSH_POLL` waits).
- Snapshot-bearing tests under `tests/compat_strict.rs` consume
  `tests/snapshots/upstream_v1_4g/` fixtures (capture itself DEFERRED
  pending Linux host with upstream `autossh 1.4g` installed).

## Coverage matrix

| Criterion | User Story | Closure Task | Test(s) |
|-----------|------------|--------------|---------|
| SC-001 | US1 | T061 | `tests/monitor_port.rs::dash_M_binds_both_listeners` + `dash_M_with_echo_port_uses_single_listener`; cross-target validation per `.github/workflows/ci.yml` matrix |
| SC-002 | US1 | T061 | `tests/supervisor.rs::short_lifetime_increments_retry_counter` + `gate_exceeding_lifetime_resets_retry_counter` (probe round-trip via real ssh tunnel covered by unit + CI integration paths; T053 deferred) |
| SC-003 | US2 | T069 | `tests/monitor_port.rs::dash_M_zero_clean_exit_zero_propagates` |
| SC-004 | US2 | T069 | `tests/monitor_port.rs::dash_M_zero_respawn_after_gatetime_backoff` + `dash_M_zero_respawns_until_max_start` |
| SC-005 | US3 | T091 | `tests/compat_strict.rs::strict_byte_equal_*_snapshots` (DEFERRED — upstream snapshot capture T086 pending Linux host) |
| SC-006 | US3 | T091 | `tests/compat_strict.rs::strict_resolution_*` + `strict_last_wins_with_no_strict` |
| SC-007 | US3 | T091 | `tests/monitor_port.rs::heartbeat_payload_matches_upstream_format` + `heartbeat_payload_with_autossh_message`; `tests/compat_strict.rs::strict_heartbeat_wire_format_byte_identical_to_upstream` |
| SC-008 | US4 | T103 | `tests/library_api.rs::default_features_off_excludes_cli_deps` |
| SC-009 | US4 | T103 | `tests/library_api.rs::send_sync_compile_time_guards_pass` (static_assertions) |
| SC-010 | US4 | T103 + T141 | `tests/missing_docs.rs::cargo_doc_no_deps_succeeds_with_deny_missing_docs` + doctests in `src/lib.rs` + `src/error.rs` (8 doctests passing) |
| SC-020 | US5 | T119 | `tests/daemonize.rs::dash_f_pidfile_lifecycle_unix` (cfg unix) |
| SC-021 | US5 | T119 | `tests/daemonize_windows.rs::dash_f_pidfile_lifecycle_windows` (cfg windows) + `detached_child_rebinds_monitor_port` |
| SC-030 | US6 | T133 | `tests/signals.rs::sigusr1_force_respawns_ssh_child` (cfg unix) + `sigusr1_leaves_retry_counter_unchanged` |
| SC-031 | US6 | T133 | `tests/signals.rs::sigterm_clean_exit_removes_pidfile` (cfg unix) + `sigterm_grace_timeout_escalates_to_sigkill_smoke` |
| SC-040 | US7 | T140 | `tests/completions_drift.rs::drift_bash` + `drift_zsh` + `drift_fish` + `drift_powershell` |
| SC-041 | US7 | T140 | `tests/completions_drift.rs::dash_M_appears_in_bash_completion` + `release.yml` archive step (T136) |
| SC-050 | US7 | T158 | Post-publish smoke install (DEFERRED — release-day operation per tasks.md T158) |

## US6 FR Closures (T133)

| Functional Requirement | Test(s) |
|------------------------|---------|
| FR-040 (SIGTERM/SIGINT clean exit) | `tests/signals.rs::sigterm_clean_exit_removes_pidfile` + `sigint_handled_identically_to_sigterm` + `sigterm_sigint_map_to_clean_exit_decision` |
| FR-041 (SIGUSR1 force-respawn) | `tests/signals.rs::sigusr1_force_respawns_ssh_child` + `sigusr1_leaves_retry_counter_unchanged` |
| FR-042 (SIGHUP reset + respawn) | `tests/signals.rs::sighup_resets_retry_counter_to_zero` + `sighup_resets_retry_budget_smoke` |
| FR-043 (Windows Ctrl+C / Ctrl+Break) | `tests/signals_windows.rs::ctrl_break_handler_terminates_child_and_exits` + `sigusr1_unavailable_on_windows_documented` |

## Polish-phase (T141..T150) cross-cutting coverage

| Concern | Test / Doc |
|---------|------------|
| `#![deny(missing_docs)]` enforcement (FR-090) | `tests/missing_docs.rs` (T141) |
| Library default-features dep-tree discipline | `tests/library_api.rs::default_features_off_excludes_cli_deps` (T096) |
| Compatibility matrix (Default / Strict per flag) | `COMPATIBILITY.md` per-flag matrix (T147) |
| BREAKING-CHANGE enumeration | `CHANGELOG.md` `[0.1.0]` + `COMPATIBILITY.md` (T143, T147) |
| `cargo audit` allow-list rationale | `.cargo/audit.toml` (T147) |
| `cargo publish --dry-run --all-features` | Manual gate run in Polish (T148) — passes |
