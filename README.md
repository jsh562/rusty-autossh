# rusty-autossh

[![crates.io](https://img.shields.io/crates/v/rusty-autossh.svg)](https://crates.io/crates/rusty-autossh)
[![docs.rs](https://docs.rs/rusty-autossh/badge.svg)](https://docs.rs/rusty-autossh)
[![CI](https://github.com/jsh562/rusty-autossh/actions/workflows/ci.yml/badge.svg)](https://github.com/jsh562/rusty-autossh/actions/workflows/ci.yml)
[![MSRV 1.85](https://img.shields.io/badge/MSRV-1.85-blue.svg)](#minimum-supported-rust-version-msrv)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Keep an SSH tunnel alive across network drops — a Rust port of Carson Harding's
[`autossh 1.4g`](https://www.harding.motd.ca/autossh/) connection supervisor.

`rusty-autossh` spawns `ssh(1)` as a child process, optionally monitors the
tunnel via the upstream-compatible `-M <port>` heartbeat (16-byte ASCII
timestamp + newline; byte-identical wire format), and respawns the ssh child
on death, probe timeout, or signal-triggered force-restart. Honors the full
`AUTOSSH_*` environment-variable surface, supports Unix daemonization (`-f`
double-fork via the `daemonize` crate), and ships a Windows `DETACHED_PROCESS`
self-respawn analogue.

## Install

### From crates.io

```sh
cargo install rusty-autossh
```

### Pre-built binaries (cargo binstall)

```sh
cargo binstall rusty-autossh
```

### Direct download

GitHub Releases ship pre-built archives for the five DDR-003 targets:

- Linux x86_64-unknown-linux-gnu
- Linux aarch64-unknown-linux-gnu
- macOS x86_64-apple-darwin
- macOS aarch64-apple-darwin
- Windows x86_64-pc-windows-msvc

## Usage

### Monitor-port heartbeat (the classic upstream pattern)

```sh
rusty-autossh -M 20000 -L 8080:localhost:80 user@host
```

Opens TCP listeners on `127.0.0.1:20000` and `127.0.0.1:20001`, sends a
16-byte ASCII timestamp + newline every `AUTOSSH_POLL` seconds (default 600),
respawns ssh if the round-trip fails or ssh exits non-zero.

### `-M 0` no-monitor mode (the modern 2026 pattern)

```sh
rusty-autossh -M 0 -o "ServerAliveInterval=30" -o "ServerAliveCountMax=3" user@host
```

No TCP listeners; respawn ssh only on non-zero exit. Pair with ssh's own
keepalives in `~/.ssh/config`. Clean exit (status 0) terminates the supervisor.

### Daemonize with `-f` + write PID + log files

```sh
AUTOSSH_PIDFILE=/tmp/auto.pid AUTOSSH_LOGFILE=/tmp/auto.log \
    rusty-autossh -f -M 0 -L 5432:db.internal:5432 jumpbox
```

Foreground exits cleanly; daemon child runs detached. `-f` unconditionally
forces `AUTOSSH_GATETIME=0` to match upstream (silences password prompts in
detached mode; non-zero gate can cause the detached child to abort on its own
quick exit).

### One-shot mode

```sh
rusty-autossh -1 -M 0 user@host
```

Exit non-zero on first failure instead of entering the retry loop. Useful for
systemd-supervised wrappers.

### Flag reference (Default mode)

Short flags follow upstream exactly. Long-form Rust-native flags add
ergonomic aliases:

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

All unrecognized post-`rusty-autossh` tokens (or anything after `--`) are
passed through to `ssh` verbatim.

## Library API

`rusty-autossh` ships a typed library API. Add to your `Cargo.toml`:

```toml
[dependencies]
rusty-autossh = { version = "0.1", default-features = false }
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

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

## Cargo Features

The CLI-only dependencies (clap, clap_complete, anstyle, tracing-*,
daemonize, atomicwrites, windows-sys) are gated behind a default-on `cli`
feature. Library consumers depending with `default-features = false` get only
the tokio-based supervisor core + `thiserror` + `socket2`. The dep-tree
discipline is enforced by an integration test
(`tests/library_api.rs::default_features_off_excludes_cli_deps`).

## Compatibility statement

`rusty-autossh` aims for byte-equal stderr against upstream `autossh 1.4g`
under **Strict mode** (activated by `--strict`, `RUSTY_AUTOSSH_STRICT=1`, or
`argv[0]=autossh` via symlink). Default mode adds Rust-native long-form flags,
structured tracing logs, and clap-styled diagnostics.

### Intentional divergences vs upstream

- **Windows `-f`** uses `CreateProcessW(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`
  self-respawn instead of fork (Windows has no fork). The foreground process
  closes monitor-port listeners before exit; the detached child re-binds.
- **`AUTOSSH_NTSERVICE`** (Cygwin NT-service mode) is NOT implemented. Use
  `sc.exe create` or NSSM against the foreground `rusty-autossh` if you want
  a full Windows Service.
- **SIGUSR1 / SIGHUP** have no Windows equivalents. Documented workaround:
  `taskkill /PID <ssh-pid>` triggers respawn via the SIGCHLD-equivalent code path.
- **Windows ssh-child termination** uses `TerminateProcess` (immediate) with
  NO 10-second SIGTERM grace window. Unix retains SIGTERM + 10s + SIGKILL.
- **No built-in alerting** (Slack/email/PagerDuty) — pipe structured tracing
  logs to your own alerting stack.
- **No `mosh`-style roaming** — autossh's design respawns a fresh ssh on
  tunnel death, dropping in-flight bytes.
- **`ssh` binary is delegated** — rusty-autossh spawns the system `ssh`; it
  does not implement the SSH protocol. Crates like `russh`/`ssh2`/`thrussh`
  cover that niche.

See [`COMPATIBILITY.md`](COMPATIBILITY.md) for the per-flag matrix.

## Lockstep SemVer

The CLI binary and the library API ship from a single crate with **lockstep
SemVer**: any breaking change to either surface (CLI flag removal, library
type change) bumps the major version together. Additive variants on
`AutosshError` and `SupervisorEvent` are MINOR via `#[non_exhaustive]`.

## `AUTOSSH_*` environment-variable reference

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

## License

Dual-licensed under either:

- MIT License ([LICENSE](LICENSE) or <http://opensource.org/licenses/MIT>)
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual-licensed as above, without any additional terms or
conditions.

## Minimum Supported Rust Version (MSRV)

Rust **1.85** (edition 2024). Pinned via `rust-toolchain.toml`. The portfolio
MSRV policy is current stable minus two minor releases at each port's release
time; this is re-verified per release per portfolio §VI of `project-instructions.md`.
