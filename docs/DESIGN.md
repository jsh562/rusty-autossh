# rusty-autossh — Design Notes

This document holds architectural notes that are too detailed for the
README + COMPATIBILITY matrix but too small for a standalone ADR. See
[`specs/00010-autossh-port/plan.md`](../../rusty/specs/00010-autossh-port/plan.md)
in the umbrella repo for the full plan + AD-001..AD-017 decisions +
HINT-001..HINT-020 implementation hints.

## Upstream Dependency Status

**E003 (reusable port-ci.yml workflow): does NOT exist** at the time of this
feature. Inline CI is duplicated from `rusty-figlet/.github/workflows/ci.yml`
(already carrying the post-figlet patches — `Unicode-3.0` deny allowance,
`taiki-e/install-action@v2` for `cargo-audit`, `rustup target add --toolchain
1.85` cross-target fix) as a **pragmatic-path decision** per T013 +
`tasks.md` §Upstream Dependency Caveat. Tracked as tech debt for back-port
when E003 ships; ci.yml + release.yml top-of-file comments reference the
back-port plan.

Upstream `autossh 1.4g` binary access for SC-005 + SC-007 byte-equal
Strict-mode snapshot capture is **NOT available on the Windows dev
environment**. T067 + T086 + T087 + T088 + T089 are DEFERRED pending
Linux-host snapshot capture. T019 + T021 snapshot stubs are likewise
DEFERRED in Phase 1.

## Repository Status (T001)

T001 (GitHub repo creation + `CARGO_REGISTRY_TOKEN` secret + branch
protection) is a **manual user action** and is DEFERRED in Phase 1. The
expected configuration is documented here for the maintainer:

- Repo: `https://github.com/jsh562/rusty-autossh` (org-level, public).
- Default branch: `main`; require CI green before merge; squash-merge only.
- Workflow permissions: `contents: read` default; `contents: write` only
  on the `github-release` job in `release.yml`.
- Secrets: `CARGO_REGISTRY_TOKEN` scoped to publish-only, rotated annually
  per `project-instructions.md` §Security & Supply Chain.

## Single-binary policy

Per Constitution III + AD-001, rusty-autossh ships **exactly one** CLI
binary (`rusty-autossh`). The dev-only `tests/bin/echo_child.rs` `[[bin]]`
entry is built only under `cargo test` (`test = false, bench = false,
doc = false`) and is NEVER shipped in release archives.

## SemVer bump policy

The crate ships a CLI binary + a library API as a single Cargo crate with
**lockstep SemVer**: any breaking change to either surface (CLI flag removal,
public type change) bumps the major version together.

- **Additive variants** on `AutosshError` and `SupervisorEvent` are MINOR
  via `#[non_exhaustive]`. Downstream consumers MUST use a wildcard `_` arm
  per the rustdoc note on each enum.
- **Compile-fail doctests** are committed for both enums that pattern-match
  exhaustively from outside the crate, asserting the compiler rejects the
  match without a `_ => ` arm. These doctests run under `cargo test --doc`
  and gate the library-SemVer contract.

## Supervisor `select!` skeleton (HINT-001)

The supervisor loop is a single async task running a three-way `tokio::select!`:

```rust,ignore
tokio::select! {
    exit = child.wait() => handle_exit(exit),
    _    = monitor.timeout_at(deadline) => handle_probe_timeout(),
    sig  = signal_rx.recv() => handle_signal(sig),
}
```

Each branch resolves to one of: `respawn()` (kill outgoing + reap + spawn
replacement per HINT-012), `terminate()` (clean exit), or `continue` (no-op
probe success). The respawn decision matrix is HINT-018; the gate-time +
retry-counter rule is HINT-013; the probe-vs-natural-exit race is HINT-014.

## Startup ordering (HINT-011)

Deterministic preconditions before the first `Command::spawn`:

1. Resolve env vars (`AUTOSSH_*`) and merge CLI flag overrides into
   `PollClock` + `MonitorMode`.
2. Resolve `ssh` binary path (`AUTOSSH_PATH` env, then `PATH` walk per
   AD-011 / HINT-017).
3. Bind monitor-port `TcpListener` pair (when `MonitorMode::Active`).
   Failure here aborts before any child spawn.
4. Write pidfile atomically + install `PidfileGuard` Drop (when
   `AUTOSSH_PIDFILE` set).
5. Daemonize (when `-f`). Unix double-fork; Windows self-respawn closes
   listeners cleanly before `CreateProcessW` per HINT-005.
6. Install signal sources (Unix `SignalKind`; Windows `ctrl_c` /
   `ctrl_break`) feeding the `mpsc<SupervisorEvent>` channel.
7. Spawn the first ssh child via `tokio::process::Command::spawn` with
   `process_group(0)` on Unix.
8. Emit `SupervisorEvent::ChildSpawned { pid }` AFTER `Command::spawn`
   returns `Ok(Child)` (child is reapable; tokio's SIGCHLD handler wired).
9. Start the monitor probe loop (when `MonitorMode::Active`) with the
   `AUTOSSH_FIRST_POLL` initial delay.

On respawn the cycle restarts at step (7); steps (1)–(6) do NOT re-run.

## Library default-features discipline

The crate ships a single `[features]` matrix: `default = ["cli"]`.
With `default-features = false`, the library surface (`SshSupervisor`,
`SshSupervisorBuilder`, `MonitorMode`, `SupervisorEvent`, `AutosshError`,
`CompatibilityMode`) is buildable on top of tokio + thiserror + socket2
alone — no clap, no clap_complete, no anstyle, no tracing-*, no
daemonize, no atomicwrites, no windows-sys. The discipline is enforced
by `tests/library_api.rs::default_features_off_excludes_cli_deps` which
shells out to `cargo tree --no-default-features` and asserts none of the
CLI-only crates appear. `cargo check --no-default-features` and
`cargo build --no-default-features --lib` are both also gated jobs in
CI per `.github/workflows/ci.yml` (FR-061).

## Tokio SIGCHLD exclusive ownership (AD-017 / HINT-006)

`tokio::process::Child` relies on the tokio runtime's own SIGCHLD handler
to wake `child.wait()` futures. Installing a competing user SIGCHLD
handler — or running a second `SshSupervisor` in the same tokio runtime —
breaks this invariant in undefined ways (lost reaps, leaked zombies,
indefinitely-pending futures). The library rustdoc on `SshSupervisor::run`
documents this as a hard contract per FR-062 + Clarifications Q7.
Consumers needing multiple supervisors MUST run each in its own
dedicated tokio runtime (one process per runtime is fine; one runtime
per thread via `tokio::runtime::Builder::new_current_thread()` is fine).

## Cross-compile target verification (T149)

From the Windows dev host, `cargo build` natively targets only
`x86_64-pc-windows-msvc`. The remaining four DDR-003 targets
(`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`,
`x86_64-apple-darwin`, `aarch64-apple-darwin`) are exercised by the CI
matrix declared in `.github/workflows/ci.yml`. Cross-building from
Windows would require either a Linux/macOS-targeted toolchain plus
linker (`cargo-zigbuild` or per-target sysroot) or a `cross` container
runtime; per portfolio precedent both are deferred to CI. Local dev
validation on the Windows host is limited to the native target plus the
`--no-default-features` library-only build.

## CI status (E003 dependency)

`E003` (the umbrella's reusable `port-ci.yml@v1.0.0` workflow) does NOT
exist yet. `.github/workflows/ci.yml` + `release.yml` are inline,
duplicated from `rusty-figlet/.github/workflows/` post-fix versions
(carrying the `Unicode-3.0` deny allowance + `taiki-e/install-action@v2`
audit replacement + `rustup target add --toolchain 1.85` cross-target
patches). Top-of-file comments in both workflow files track the
back-port plan for when E003 ships. The pragmatic-path decision is
recorded in `specs/00010-autossh-port/tasks.md` §Upstream Dependency
Caveat.

## Test isolation policy (T149)

Every integration test under `tests/` owns a freshly-constructed
`tempfile::TempDir` via the `sandbox()` helper in `tests/common/mod.rs`.
Tests MUST NOT:

- write to relative paths (always anchored under the sandbox `TempDir`);
- write under `$HOME` or `%USERPROFILE%`;
- share a global mutable temp directory with sibling tests.

Per-test `env_guard` RAII wrappers in `tests/common/mod.rs` isolate
mutations to `AUTOSSH_*` / `RUSTY_AUTOSSH_STRICT` / `AUTOSSH_PATH` so
parallel test threads (`cargo test -- --test-threads=N` for any N)
remain deterministic. Signal-sensitive tests use
`#[tokio::test(flavor = "current_thread")]` and tokio time mocking via
`tokio::time::pause()` to keep CI wall-time small (no real 600 s
`AUTOSSH_POLL` waits).
