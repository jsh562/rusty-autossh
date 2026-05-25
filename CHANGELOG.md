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

[Unreleased]: https://github.com/jsh562/rusty-autossh/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jsh562/rusty-autossh/releases/tag/v0.1.0
