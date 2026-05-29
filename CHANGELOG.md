# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security advisory follow-ups (tracked, allow-listed in `.cargo/audit.toml`)

- **RUSTSEC-2025-0069** — `daemonize` 0.5 flagged unmaintained. Re-evaluate
  per release; migrate when a maintained replacement matures.
- **RUSTSEC-2026-0009** — `time` 0.3.45 DoS via stack exhaustion via
  `tracing-appender`. Fix requires `time` ≥ 0.3.47 which requires Rust
  ≥ 1.88; remove allow-list and bump together at the next MSRV bump.
  Risk mitigated by usage profile (no user-controlled date/time parsing).

## [0.2.0] - 2026-05-26

### Added (additive only — no v0.1.x behavior changed)

- Portfolio-wide [Cargo Features Convention](https://github.com/jsh562/rustylib/blob/main/specs/adrs/0006-cargo-features-convention-for-portfolio-ports.md)
  layout per ADR-0006 + `project-instructions.md` §Cargo Feature Surface. rusty-autossh applies the minimum convention as a **tightly-coupled-capability port** per spec 00011 §Scope Edge Cases.
- New umbrella features (all `["cli"]` composition for this tightly-coupled-cap port):
  - `full` — kitchen-sink umbrella per FR-002
  - `autossh-classic` — required `<port>-classic` umbrella per FR-004; autossh 1.4g drop-in replacement
  - `autossh-minimal` — preset bundle per FR-007; explicit minimal-CLI semantic alias
- `default` now aliases to `full` instead of directly to `cli`. Resolved dependency set is identical (`full = ["cli"]`); no observable change for any consumer.
- See [`docs/feature-layout.md`](docs/feature-layout.md) for the zero-leaf rationale.

All v0.1.x feature names are preserved verbatim with identical compositions. `cli = ["dep:clap", "dep:clap_complete", "dep:anstyle", "dep:tracing", "dep:tracing-subscriber", "dep:tracing-appender", "dep:daemonize", "dep:atomicwrites", "dep:windows-sys"]` is unchanged. Library consumers using `default-features = false` get the same CLI-stripped build. The tokio-based supervisor (`SshSupervisor`, `SshSupervisorBuilder`, `MonitorMode`, `CompatibilityMode`, `SupervisorEvent`, `SignalKind`, `AutosshError`), monitor-port `ProbeLoop`, Strict-mode argv pre-scanner, and ssh-spawn pipeline all remain byte-for-byte unchanged from v0.1.0.

### Notes

- See the new `## Cargo Features` section in `README.md` for the
  feature matrix, preset bundles, keep-list workaround, and convention
  authority citations.
- Reference: [ADR-0006](https://github.com/jsh562/rustylib/blob/main/specs/adrs/0006-cargo-features-convention-for-portfolio-ports.md)
  (why this layout) + [`project-instructions.md` §Cargo Feature Surface](https://github.com/jsh562/rustylib/blob/main/project-instructions.md)
  (what the rules are).
- CI matrix expanded per spec 00011 FR-010..FR-014: now includes
  `test-default` (kitchen sink + cross-compile), `test-no-default`
  (bare library + dep-tree audit per SC-001), `test-autossh-classic`,
  `test-autossh-minimal` (preset bundles per SC-003), `test-keeplist`
  (keep-list workaround per SC-004), and `lint-convention` (vendored
  `tools/feature-lint/run.sh` invocation per FR-052). Tier 4
  (`check-leaf-<leaf>`) is intentionally empty — zero leaves carved
  per docs/feature-layout.md.
- Platform-conditional deps (`daemonize` Unix-only, `windows-sys`
  Windows-only) live under `[target.'cfg(<plat>)'.dependencies]`
  Cargo tables — a compile-time platform gate, NOT a Cargo feature.
  They are unreachable on the non-applicable platform regardless of
  feature selection and don't appear as separate `[features]` entries.
- The lint script is **vendored** into `tools/feature-lint/` (synced
  from the umbrella `jsh562/rustylib` repo) so per-port CI workflows
  do not depend on cross-repo `actions/checkout` of the private
  umbrella. Sync precedent set by rusty-figlet v0.2.0 (E011 Phase 2
  iteration 6), rusty-ts v0.2.0 (E011 Phase 3), rusty-sponge v0.2.0
  (E011 Phase 4), rusty-vipe v0.2.0 (E011 Phase 5), rusty-pee v0.2.0
  (E011 Phase 6), rusty-pwgen v0.2.0 (E011 Phase 7), rusty-detox
  v0.2.0 (E011 Phase 8), rusty-pv v0.2.0 (E011 Phase 9), and
  rusty-pdfgrep v0.2.0 (E011 Phase 10).
- rusty-autossh is the **LAST sibling port** in the E011 Phase 3..11
  rollout; with v0.2.0 publish all 10 portfolio ports converge on the
  portfolio-wide convention shape, satisfying spec 00011 SC-006
  (10/10 ports shipped) and triggering Phase 12 portfolio wrap-up.

## [0.1.0] - 2026-MM-DD

Initial release: Rust port of Carson Harding's `autossh 1.4g` SSH connection
supervisor. Tokio-based async supervisor loop; `-M <port>` monitor-port
heartbeat with byte-identical 16-byte ASCII timestamp + newline wire format;
`-M 0` no-monitor mode; full `AUTOSSH_*` env-var surface incl. `AUTOSSH_MESSAGE`;
Unix `-f` daemonize via `daemonize` 0.5; Windows `DETACHED_PROCESS` self-respawn
analogue via `windows-sys` 0.59; SIGTERM/SIGINT/SIGUSR1/SIGHUP handling on Unix;
Ctrl+C/Ctrl+Break handling on Windows; pre-generated bash/zsh/fish/powershell
completions with drift gate; byte-equal Strict-mode upstream compatibility
(`--strict` / `RUSTY_AUTOSSH_STRICT=1` / `argv[0]=autossh`); typed library
API (`SshSupervisor`, `SshSupervisorBuilder`, `MonitorMode`, `SupervisorEvent`,
`AutosshError`); `default-features = false` strips all CLI-only deps from the
library dep tree.

### Added

- `rusty-autossh` CLI binary with upstream-compatible short flags (`-M`, `-f`,
  `-V`, `-1`) and Rust-native long-form flags (`--monitor-port`, `--background`,
  `--one-shot`, `--poll`, `--first-poll`, `--gate-time`, `--max-start`,
  `--max-lifetime`, `--ssh-path`, `--pid-file`, `--log-file`, `--debug`,
  `--log-level`, `--strict`, `--no-strict`).
- `completions <shell>` subcommand emitting `bash`/`zsh`/`fish`/`powershell`
  scripts via `clap_complete::generate`.
- `rusty_autossh` library crate with `SshSupervisor`, `SshSupervisorBuilder`,
  `MonitorMode`, `SupervisorEvent`, `AutosshError` (`#[non_exhaustive]`).
- Snapshot suite under `tests/snapshots/upstream_v1_4g/` covering ≥20
  fixtures (4 in-scope flags + 8 excluded short + 8 excluded long + argv-passthrough boundary cases).

### BREAKING-CHANGE notes

The following constitute documented divergences from upstream `autossh 1.4g`:

- **(a) Windows `-f` uses `CreateProcessW(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`
  self-respawn** instead of fork. The foreground process closes its monitor-port
  `TcpListener` handles cleanly before invoking `CreateProcessW`; the detached
  child re-binds. Documented in `COMPATIBILITY.md`. Justified by Windows
  having no `fork(2)` equivalent.
- **(b) `AUTOSSH_NTSERVICE`** (Cygwin NT-service mode) is NOT implemented.
  Users wanting Windows Service integration should use `sc.exe create` or
  NSSM against the foreground `rusty-autossh` process.
- **(c) SIGUSR1 has no Windows equivalent.** Documented workaround:
  `taskkill /PID <ssh-pid>` triggers respawn via the SIGCHLD-equivalent code
  path. SIGHUP likewise is Unix-only.
- **(d) Windows ssh-child termination uses `TerminateProcess`** (immediate)
  with NO 10-second SIGTERM grace window (per FR-043). Unix retains the
  upstream-style SIGTERM + 10s + SIGKILL fallback (FR-040). Justified by
  Windows having no SIGTERM-equivalent that `DETACHED_PROCESS` children
  honor uniformly.
- **(e) MSRV is Rust 1.85** (edition 2024). The upstream `autossh` is C and
  has no Rust MSRV; this is a portfolio convention per `project-instructions.md`.
  Per the stable-minus-two policy this is re-verified at each release.
- **(f) Crate name `rusty-autossh` (not `autossh`)** — the canonical
  `autossh` name on crates.io is squatted by an unrelated SSH-credential
  manager. The `rusty-` prefix matches the Rusty portfolio convention and
  resolves the name-collision risk surfaced in spec §Risks. Strict mode
  still emits the literal `autossh:` stderr prefix to preserve byte-equal
  upstream compatibility per FR-051.

Reference baseline: upstream `autossh 1.4g` (Carson Harding,
<https://www.harding.motd.ca/autossh/>).

[Unreleased]: https://github.com/jsh562/rusty-autossh/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/jsh562/rusty-autossh/releases/tag/v0.2.0
[0.1.0]: https://github.com/jsh562/rusty-autossh/releases/tag/v0.1.0
