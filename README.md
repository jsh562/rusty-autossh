# rusty-autossh

Keep an SSH tunnel alive across network drops. Rust port of [Carson Harding's `autossh 1.4g`](https://www.harding.motd.ca/autossh/) connection supervisor.

[![crates.io](https://img.shields.io/crates/v/rusty-autossh.svg)](https://crates.io/crates/rusty-autossh)
[![docs.rs](https://docs.rs/rusty-autossh/badge.svg)](https://docs.rs/rusty-autossh)
[![CI](https://github.com/jsh562/rusty-autossh/actions/workflows/ci.yml/badge.svg)](https://github.com/jsh562/rusty-autossh/actions/workflows/ci.yml)
[![MSRV](https://img.shields.io/badge/MSRV-1.85-blue.svg)](#msrv)
[![license: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Spawns `ssh(1)` as a child, optionally monitors the tunnel via the upstream-compatible `-M <port>` heartbeat (16-byte ASCII timestamp + newline, byte-equal wire format), & respawns the ssh child on death, probe timeout, or signal-triggered force-restart. Honors the full `AUTOSSH_*` environment surface, daemonizes on Unix via `daemonize` double-fork, & ships a Windows `DETACHED_PROCESS` self-respawn analogue.

Part of the [Rusty portfolio](https://jsh562.github.io/rusty-portfolio).

## Install

```sh
cargo install rusty-autossh
# or, with prebuilt binaries:
cargo binstall rusty-autossh
# or, download directly from GitHub Releases:
# https://github.com/jsh562/rusty-autossh/releases
```

Prebuilt binaries ship for five targets: Linux x86_64/aarch64, macOS x86_64/aarch64, Windows x86_64.

## Usage

```sh
# Classic upstream pattern: heartbeat over a TCP listener pair, respawn on probe failure
rusty-autossh -M 20000 -L 8080:localhost:80 user@host

# Modern 2026 pattern: no TCP listeners, ride on ssh's own keepalives
rusty-autossh -M 0 -o "ServerAliveInterval=30" -o "ServerAliveCountMax=3" user@host

# Daemonize with a PID file & log file (good for systemd ExecStart=)
AUTOSSH_PIDFILE=/tmp/auto.pid AUTOSSH_LOGFILE=/tmp/auto.log \
    rusty-autossh -f -M 0 -L 5432:db.internal:5432 jumpbox

# One-shot mode: exit non-zero on first failure (good for retry-loop wrappers)
rusty-autossh -1 -M 0 user@host

# Override autossh tunables on the command line (Default mode only)
rusty-autossh --poll 30 --gate-time 5 --max-start 10 -M 0 user@host

# Strict autossh-compat mode (drop-in autossh 1.4g replacement)
rusty-autossh --strict -M 20000 user@host
RUSTY_AUTOSSH_STRICT=1 rusty-autossh -M 20000 user@host
autossh -M 20000 user@host                 # via autossh argv[0] symlink

# Shell completions
rusty-autossh completions bash              # > ~/.bash_completion.d/rusty-autossh
rusty-autossh completions zsh               # > ~/.zfunc/_rusty-autossh
rusty-autossh completions fish              # > ~/.config/fish/completions/rusty-autossh.fish
rusty-autossh completions powershell
```

All unrecognized post-`rusty-autossh` tokens (or anything after `--`) pass through to `ssh` verbatim.

### Default-mode long-form aliases

Short flags follow upstream exactly. Long-form Rust-native flags add ergonomic aliases.

| Short        | Long alias        | Effect                                            |
|--------------|-------------------|---------------------------------------------------|
| `-M <P[:E]>` | `--monitor-port`  | Monitor port (or `port:echo` single-listener)     |
| `-f`         | `--background`    | Daemonize (forces `AUTOSSH_GATETIME=0`)           |
| `-1`         | `--one-shot`      | Exit non-zero on first failure                    |
| `-V`         | `--version`       | Print version                                     |
|              | `--poll <S>`      | Override `AUTOSSH_POLL` (poll interval seconds)   |
|              | `--first-poll <S>`| Override `AUTOSSH_FIRST_POLL`                     |
|              | `--gate-time <S>` | Override `AUTOSSH_GATETIME`                       |
|              | `--max-start <N>` | Override `AUTOSSH_MAXSTART` (`-1` = unlimited)    |
|              | `--max-lifetime <S>` | Override `AUTOSSH_MAXLIFETIME`                 |
|              | `--ssh-path <P>`  | Override `AUTOSSH_PATH`                           |
|              | `--pid-file <P>`  | Override `AUTOSSH_PIDFILE`                        |
|              | `--log-file <P>`  | Override `AUTOSSH_LOGFILE`                        |
|              | `--debug`         | Enable debug-level tracing                        |
|              | `--log-level <L>` | Set log level (`trace`/`debug`/`info`/...)        |
|              | `--strict`        | Force Strict-mode upstream-compat                 |
|              | `--no-strict`     | Force Default mode (overrides env + argv[0])      |

### `AUTOSSH_*` environment-variable reference

| Variable                | Default        | Effect                                                  |
|-------------------------|----------------|---------------------------------------------------------|
| `AUTOSSH_POLL`          | `600`          | Probe interval seconds                                  |
| `AUTOSSH_FIRST_POLL`    | `AUTOSSH_POLL` | Initial probe delay                                     |
| `AUTOSSH_GATETIME`      | `30`           | Min lifetime before retry counts as failure             |
| `AUTOSSH_MAXSTART`      | `-1`           | Max retries (`-1` = unlimited)                          |
| `AUTOSSH_MAXLIFETIME`   | `0`            | Self-terminate after N seconds (`0` = unlimited)        |
| `AUTOSSH_DEBUG`         | unset          | Enable debug logging                                    |
| `AUTOSSH_LOGFILE`       | unset          | Append diagnostics to this file                         |
| `AUTOSSH_LOGLEVEL`      | `info`         | Log level                                               |
| `AUTOSSH_PIDFILE`       | unset          | Write supervisor PID atomically                         |
| `AUTOSSH_PATH`          | unset          | Override `ssh` binary path (verbatim, no PATH fallback) |
| `AUTOSSH_PORT`          | unset          | Override `-M <port>` value                              |
| `AUTOSSH_MESSAGE`       | unset          | Append to heartbeat payload (single space separator)    |
| `RUSTY_AUTOSSH_STRICT`  | unset          | `=1` activates Strict mode (overridden by `--no-strict`)|

## Library API

The library exposes the `SshSupervisor` / `SshSupervisorBuilder` / `MonitorMode` / `CompatibilityMode` / `SupervisorEvent` / `SignalKind` / `AutosshError` types without any CLI deps. Use it when you want autossh-style respawn supervision inside a long-running Rust process.

```rust,no_run
use rusty_autossh::{SshSupervisorBuilder, MonitorMode};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), rusty_autossh::AutosshError> {
    let mut supervisor = SshSupervisorBuilder::new()
        .ssh_args(vec!["-L".into(), "8080:localhost:80".into(), "user@host".into()])
        .monitor_mode(MonitorMode::Active { port: 20000, echo: None })
        .build()?;
    supervisor.run().await
}
```

```toml
[dependencies]
rusty-autossh = { version = "0.2", default-features = false }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

For library-only consumers without CLI deps see the [Cargo Features](#cargo-features) section.

## Cargo Features

`default` enables `full`, which (for this tightly-coupled-capability port) resolves to the `cli` umbrella. `autossh-classic` reproduces v0.1.x bare-port behavior matching upstream `autossh 1.4g` 1:1. To strip the CLI surface use `default-features = false` or `--no-default-features` & add the features you want.

rusty-autossh is a **tightly-coupled-capability port**: its one documented job is "keep an ssh process alive across network drops". The CLI sub-capabilities (clap argv parsing, completions subcommand, tracing-based logging, atomic pidfile writer, Unix daemonize double-fork, Windows `DETACHED_PROCESS` self-respawn analogue, Strict-mode argv pre-scanner) are tightly coupled inside the `cli` umbrella. No optional feature leaves are carved beyond the required umbrellas; see [`docs/feature-layout.md`](docs/feature-layout.md) for why.

### Feature matrix

| Feature | Description | Umbrella(s) |
|---|---|---|
| `cli` | All CLI-only dependencies (`clap`, `clap_complete`, `anstyle`, `tracing`, `tracing-subscriber`, `tracing-appender`, `daemonize` (Unix-only via `[target.'cfg(unix)']`), `atomicwrites`, `windows-sys` (Windows-only via `[target.'cfg(windows)']`)) and the binary entry point, clap argument parser, completions subcommand, structured tracing logger, atomic pidfile writer, Unix `-f` daemonize double-fork, Windows `DETACHED_PROCESS` self-respawn analogue, and Strict-mode argv pre-scanner. Library consumers strip via `default-features = false`. | `full`, `autossh-classic`, `autossh-minimal` |

Platform-conditional deps (`daemonize`, `windows-sys`) live under `[target.'cfg(<plat>)'.dependencies]` Cargo tables. They are a compile-time platform gate, NOT a Cargo feature. They're unreachable on the non-applicable platform regardless of feature selection. The `cli` feature wires them up consistently across both platforms.

### Preset bundles

| Bundle | Composition | Use case |
|---|---|---|
| `autossh-classic` | `cli` | Drop-in upstream `autossh 1.4g` replacement. Strict mode is invoked via `--strict`, `RUSTY_AUTOSSH_STRICT`, or `autossh` argv[0] auto-detect. |
| `autossh-minimal` | `cli` | Explicit minimal-CLI alias for users who prefer the `<port>-minimal` naming convention seen across other portfolio ports. Identical composition to `autossh-classic`. |

### Keep-list workaround (Cargo features are union-only)

Cargo features cannot subtract from `default`. To get "everything except a specific feature," disable defaults & enumerate the features you want:

```sh
cargo install rusty-autossh --no-default-features --features "cli"
# → bare CLI. Equivalent to autossh-classic / autossh-minimal.
```

For the common cases the named [preset bundles](#preset-bundles) are usually sufficient.

### Library-only consumers

```toml
[dependencies]
rusty-autossh = { version = "0.2", default-features = false }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

This strips `clap`, `clap_complete`, `anstyle`, `tracing`, `tracing-subscriber`, `tracing-appender`, `daemonize`, `atomicwrites`, & `windows-sys`. The resulting build pulls only the foundational supervisor stack (`tokio` with `process` + `net` + `signal` + `sync` + `time` + `rt` + `macros` + `io-util`, plus `thiserror` for `AutosshError` & `socket2` for monitor-port `SO_REUSEADDR`). The CI `test-no-default` job runs `cargo tree --no-default-features` on every PR & fails the build if any CLI-only dep leaks back in.

### Convention authority

This layout follows the portfolio-wide Cargo Features Convention. The "why" lives in [ADR-0006](https://github.com/jsh562/rustylib/blob/main/specs/adrs/0006-cargo-features-convention-for-portfolio-ports.md); the "what" lives in [`project-instructions.md` §Cargo Feature Surface](https://github.com/jsh562/rustylib/blob/main/project-instructions.md). Every Rusty port from v0.2 onward exposes the same umbrella set (`default` / `full` / `cli` / `<port>-classic`), per-port leaves named in kebab-case, & 2 to 4 preset bundles.

## Compatibility

`rusty-autossh` has two modes:

- **Default mode.** clap-styled flag parser. Long-form Rust-native flags, structured tracing logs, & clap-styled diagnostics are all available.
- **Strict mode** (activated by `--strict`, `RUSTY_AUTOSSH_STRICT=1`, or invoking the binary as `autossh`). Byte-equal stderr against upstream `autossh 1.4g`. `--no-strict` overrides env + argv[0].

### Documented intentional divergences

- **Windows `-f`** uses `CreateProcessW(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)` self-respawn instead of fork (Windows has no fork). The foreground process closes monitor-port listeners before exit; the detached child re-binds.
- **`AUTOSSH_NTSERVICE`** (Cygwin NT-service mode) is NOT implemented. Use `sc.exe create` or NSSM against the foreground `rusty-autossh` for a full Windows Service.
- **SIGUSR1 / SIGHUP** have no Windows equivalents. Documented workaround: `taskkill /PID <ssh-pid>` triggers respawn via the SIGCHLD-equivalent code path.
- **Windows ssh-child termination** uses `TerminateProcess` (immediate) with no 10-second SIGTERM grace window. Unix retains SIGTERM + 10s + SIGKILL.

See [`COMPATIBILITY.md`](COMPATIBILITY.md) for the per-flag matrix.

## What's not shipped

- **Built-in alerting** (Slack/email/PagerDuty). Pipe structured tracing logs to your own alerting stack.
- **`mosh`-style roaming.** autossh's design respawns a fresh ssh on tunnel death, dropping in-flight bytes.
- **SSH protocol implementation.** `rusty-autossh` spawns the system `ssh`; it doesn't speak SSH itself. Crates like `russh` / `ssh2` / `thrussh` cover that niche.
- **`AUTOSSH_NTSERVICE` Cygwin NT-service mode.** Use `sc.exe create` or NSSM against the foreground `rusty-autossh` instead.
- **Windows SIGTERM grace window.** `TerminateProcess` is immediate on Windows; Unix retains the 10-second SIGTERM + SIGKILL sequence.

## MSRV

Rust **1.85** (edition 2024). Pinned via `rust-toolchain.toml`. The portfolio MSRV policy is current stable minus two minor releases at each port's release time, re-verified per release.

## License

Dual-licensed under [MIT](LICENSE) or [Apache-2.0](LICENSE-APACHE) at your option.
