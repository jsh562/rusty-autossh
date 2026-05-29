# rusty-autossh — v0.2.0 Feature Layout

**Status**: implementation draft for the v0.2.0 Cargo features convention
backfill (spec 00011, Phase 11 — rusty-autossh; the LAST sibling port in
the portfolio-wide rollout).

**Authority**:
- `specs/adrs/0006-cargo-features-convention-for-portfolio-ports.md` (why)
- `project-instructions.md` §Cargo Feature Surface (what)
- This document — the per-port carving + WHY for each leaf, per HINT-003
  + HINT-009 of spec 00011.

**Reference port**: rusty-figlet v0.2.0 — see `../../rusty-figlet/docs/feature-layout.md`
(FROZEN reference port) for the format anchor. rusty-autossh conforms to the
same shape with the minimum-convention surface dictated by its
tightly-coupled-capability scope. The companion sibling ports rusty-sponge
v0.2.0, rusty-vipe v0.2.0, rusty-pee v0.2.0, rusty-pwgen v0.2.0, rusty-detox
v0.2.0, rusty-pv v0.2.0, and rusty-pdfgrep v0.2.0 established the zero-leaf
precedent for ports whose entire surface lives behind a single CLI umbrella.

**Iteration model**: v0.2.0 is a **purely additive** SemVer-minor release.
Every v0.1.x feature name and composition is preserved verbatim; new
umbrellas (`full`, `autossh-classic`, `autossh-minimal`) are layered on top
without renaming or narrowing the existing `cli` / `default` features.
Library and binary API surfaces are unchanged.

## Tightly-coupled-capability port — spec 00011 §Scope Edge Cases

The documented capability is "keep an ssh process alive across network
drops" — a Rust port of Carson Harding's `autossh 1.4g`. The
implementation requires:

1. **Tokio-based supervisor runtime** (`tokio` with `process` + `net` +
   `signal` + `sync` + `time` + `rt` + `macros` + `io-util` features) —
   foundational, always-on. Every supervisor cycle (spawn ssh, watch
   monitor port, reap on exit, respawn) flows through this runtime.
2. **Public error type** (`thiserror`) — foundational, always-on.
   `AutosshError` variants are part of the library API contract.
3. **Monitor-port socket configuration** (`socket2`) — foundational,
   always-on. SO_REUSEADDR on the monitor-port listener pair is required
   by HINT-020 to survive fast respawns.
4. **CLI surface** (`clap` + `clap_complete` + `anstyle` + `tracing` +
   `tracing-subscriber` + `tracing-appender` + `atomicwrites` +
   platform-conditional `daemonize` (Unix) and `windows-sys` (Windows))
   — already gated by the v0.1.x `cli` umbrella.

Carving the CLI sub-capabilities (clap argument parsing, completions,
tracing-based logging, pidfile management, Unix daemonize double-fork,
Windows `DETACHED_PROCESS` self-respawn analogue) into separate leaves
would either (a) violate the additive-v0.2.0 contract by narrowing the
existing `cli` composition, or (b) introduce artificial splits that no
consumer benefits from since the binary always-on flag set ships all of
them together. Per the spec guidance and the established sibling-port
precedent, rusty-autossh adopts the zero-leaf path with the
minimum-convention surface.

## Source-tree walk

`src/` modules (v0.1.0, post-Phase-1 baseline):

| Module          | Always-on?    | CLI-only deps                                                              | Notes                                                          |
|-----------------|---------------|----------------------------------------------------------------------------|----------------------------------------------------------------|
| `lib.rs`        | yes           | (tokio + thiserror — always-on; mpsc::Sender on the public Builder)        | Public API (`SshSupervisor`, `SshSupervisorBuilder`, `MonitorMode`, `CompatibilityMode`, `SupervisorEvent`, `SignalKind`, `AutosshError`). |
| `error.rs`      | yes           | (thiserror — always-on)                                                    | `AutosshError` enum; library + binary need it.                 |
| `clock.rs`      | yes           | none                                                                       | `PollClock` poll/first-poll/gate-time state.                   |
| `mode.rs`       | yes           | none                                                                       | `CompatibilityMode` resolution from argv + env + argv[0].      |
| `monitor.rs`    | yes           | (tokio + socket2 — always-on)                                              | `ProbeLoop` monitor-port TCP listener pair + 16-byte heartbeat.|
| `spawner.rs`    | yes           | (tokio + process — always-on)                                              | ssh binary resolution + `Command::spawn`.                      |
| `strict.rs`     | yes           | none                                                                       | Strict-mode argv pre-scanner + byte-equal upstream stderr.     |
| `supervisor.rs` | yes           | (tokio — always-on)                                                        | Core `Supervisor` state machine + `select!` driver loop.       |
| `signals.rs`    | yes           | (tokio::signal — always-on; `#[cfg(unix)]` / `#[cfg(windows)]` internal)   | Unified `mpsc<SupervisorEvent>` signal channel.                |
| `cli.rs`        | no — `cli`    | clap, clap_complete, anstyle                                               | Binary entry argv parser; gated by `#[cfg(feature = "cli")]` in `lib.rs`. |
| `daemonizer.rs` | no — `cli`    | daemonize (Unix), windows-sys (Windows)                                    | `-f` double-fork (Unix) / `DETACHED_PROCESS` self-respawn (Windows). Layered under `cli` + `#[cfg(unix)]` / `#[cfg(windows)]`. |
| `logging.rs`    | no — `cli`    | tracing, tracing-subscriber, tracing-appender                              | Structured logger init; gated by `cli`.                        |
| `pidfile.rs`    | no — `cli`    | atomicwrites                                                               | Atomic pidfile write + Drop guard; gated by `cli`.             |
| `main.rs`       | no — `cli`    | (all CLI deps above)                                                       | Binary entry; gated by `required-features = ["cli"]`.          |

## Leaf-carving criteria (HINT-009)

A capability becomes a leaf when ALL of the following hold:

1. It is **self-containable** — gated cleanly via `#[cfg(feature = "<leaf>")]`
   at the module or top-level item boundary (HINT-004).
2. Either (a) it has a **sole optional dependency** that no other leaf needs
   (HINT-005), OR (b) it is a pure-cfg-gate of an internal module worth
   exposing as a knob.
3. Disabling it does NOT break any always-on library/CLI surface.

A capability does NOT become a leaf when:

- It is foundational (`tokio`, `thiserror`, `socket2` — each is part of
  the always-on library contract; the `SshSupervisor` / `SshSupervisorBuilder`
  / `MonitorMode` / `SupervisorEvent` types cannot compile without them).
- It is part of the tightly-coupled CLI bundle already gated by the v0.1.x
  `cli` umbrella (clap argv parsing, completions subcommand, structured
  tracing, pidfile, Unix daemonize, Windows `DETACHED_PROCESS`, Strict-mode
  argv pre-scanner).
- Carving it would require splitting an existing v0.1.x dep out of the
  `cli` umbrella (violates additive-v0.2.0 contract per spec 00011
  Phase 4-10 precedent).

## v0.2.0 Carved Leaves

**ZERO new leaves carved at v0.2.0**. Every capability inside rusty-autossh
is either:

1. Foundational always-on library code (tokio supervisor runtime,
   `AutosshError` / `SshSupervisor` / `SshSupervisorBuilder` / `MonitorMode`
   / `CompatibilityMode` / `SupervisorEvent` / `SignalKind` types, monitor
   port `ProbeLoop`, ssh spawner, signal source, Strict-mode argv
   pre-scanner) — cannot be stripped without breaking the public library
   surface.
2. Already gated by the v0.1.x `cli` umbrella (clap-derived argument
   parsing, completions subcommand, tracing-based logging, atomic
   pidfile management, Unix daemonize double-fork, Windows
   `DETACHED_PROCESS` self-respawn analogue).

### Leaves intentionally NOT carved

The following candidate leaves were considered + rejected:

- **`daemonize` (Unix `-f` double-fork)**: The `daemonize` crate is a
  platform-conditional optional dependency that is ALREADY part of the
  v0.1.x `cli` composition. The `daemonizer.rs` module is gated by
  `#[cfg(feature = "cli")]` AND `#[cfg(unix)]` — the latter is a
  compile-time platform gate, NOT a Cargo feature. Carving `daemonize`
  out of `cli` would require splitting the `daemonize` dep away from
  `cli` AND renaming the existing `cli` feature — violates the
  additive-v0.2.0 contract per spec 00011 §Brownfield Notes. The Unix
  daemonize capability survives untouched inside the existing `cli`
  umbrella; on non-Unix platforms the dep is simply unreachable per the
  `[target.'cfg(unix)'.dependencies]` table.

- **`windows-detach` (Windows `DETACHED_PROCESS` self-respawn)**: Same
  rejection reasoning as `daemonize`. `windows-sys` is platform-conditional
  via `[target.'cfg(windows)'.dependencies]` AND already part of the
  v0.1.x `cli` composition. The Windows self-respawn analogue lives in
  `daemonizer.rs` behind `#[cfg(feature = "cli")] #[cfg(windows)]`
  layered gates. Carving it out as a separate Cargo feature leaf would
  duplicate the surface and confuse the additive contract.

- **`tracing-file`**: `tracing-appender` is a CLI-only dep already in
  the v0.1.x `cli` umbrella. Carving it out would require splitting
  `tracing-appender` away from `cli` and renaming — violates the
  additive contract. The capability survives untouched inside `cli`.

- **`pidfile`**: `atomicwrites` is a CLI-only dep already in the v0.1.x
  `cli` umbrella. Same rejection reasoning as `tracing-file`. The
  atomic pidfile writer lives in `pidfile.rs` behind
  `#[cfg(feature = "cli")]` and survives unchanged inside `cli`.

- **`completions`**: Could be carved as `["dep:clap_complete"]`, but
  `clap_complete` is bundled into the v0.1.x `cli` umbrella verbatim.
  Carving it would either rename `cli` (breaking SemVer additivity) or
  duplicate the surface.

- **`strict-compat`**: rusty-autossh's Strict-mode dispatcher lives in
  `strict.rs` (always-on; library API exposes the
  `CompatibilityMode::Strict` enum variant) AND inline argv pre-scan in
  `main.rs`. The always-on portion cannot be carved out (public type).
  The CLI portion is gated by `cli` already. Carving a separate
  `strict-compat` leaf would either duplicate the surface or split the
  inline dispatcher out of `main.rs` — both violate the additive
  contract.

## Preset bundles (FR-007 — 2 required for tightly-coupled-capability ports)

Per spec 00011 §Scope Edge Cases + FR-007, even tightly-coupled-capability
ports declare 2 preset bundles to give the keep-list workaround
documentation something concrete to point at.

### `autossh-classic` (REQUIRED — bare port, 1:1 with upstream autossh 1.4g)

```toml
autossh-classic = ["cli"]
```

- Includes `cli` so the binary exists.
- Tightly-coupled-capability port; the `cli` umbrella IS the bare-port
  surface (clap argv parsing, completions, tracing logger, pidfile
  writer, Unix daemonize, Windows `DETACHED_PROCESS`, Strict-mode
  dispatcher are all in `cli`).
- Use case: `cargo install rusty-autossh --no-default-features --features autossh-classic`
  for an autossh 1.4g drop-in replacement (Strict mode is invoked via
  the `--strict` flag, `RUSTY_AUTOSSH_STRICT` env var, or `autossh`
  argv[0] auto-detect — none of these require additional features).

### `autossh-minimal`

```toml
autossh-minimal = ["cli"]
```

- Same composition as `autossh-classic` (tightly-coupled-capability
  port has no smaller subset to carve).
- Use case: explicit "minimal CLI install" alias for users who prefer
  the `<port>-minimal` naming convention seen across other Rusty ports
  (figlet-minimal, ts-minimal, sponge-minimal, vipe-minimal,
  pee-minimal, pwgen-minimal, detox-minimal, pv-minimal,
  pdfgrep-minimal). Documented as an intentional semantic alias rather
  than a distinct composition.

## Cross-port glossary candidates

No leaves carved → no cross-port glossary contributions from rusty-autossh
in this iteration. If a future minor release adds an orthogonal capability
(e.g., a `metrics-exporter` leaf emitting Prometheus-style supervisor
metrics, an `alert-webhook` leaf with HTTP POST on probe failure, or a
`ssh-protocol-native` leaf swapping the spawned `ssh(1)` for a Rust-native
implementation like `russh`), the leaf would be a candidate for promotion
to `docs/feature-vocabulary.md` per FR-053.

## CI matrix shape (FR-010..FR-014)

Per plan §Per-Port v0.2.0 CI Matrix, scaled to a zero-leaf port:

- **Tier 1 — `test-default`**: full DDR-003 cross-compile matrix
  (5 targets). Post-v0.2.0 `default = ["full"]` and `full = ["cli"]`,
  so the kitchen-sink test resolves to the same set as v0.1.0
  `default = ["cli"]` — no regression in coverage.
- **Tier 2 — `test-no-default`**: Linux x86_64 only. `cargo test
  --no-default-features --lib` + dep-tree audit (SC-001 evidence).
- **Tier 3 — `test-<bundle>`**: one job per preset bundle. Linux only.
  - `test-autossh-classic`
  - `test-autossh-minimal`
- **Tier 4 — `check-leaf-<leaf>`**: SKIPPED. Zero leaves → no
  per-leaf compile-check jobs. A placeholder comment in `ci.yml`
  documents why this tier is empty.
- **Tier 5 — `lint-convention`**: single Linux job invoking the
  vendored `tools/feature-lint/run.sh` script.

Per FR-014, bundle/lint jobs are constrained to Linux x86_64.

## Vendored feature-lint

Per spec 00011 §Phase 2 iteration 6 precedent (rusty-figlet vendored
the lint script because the umbrella `jsh562/rustylib` is private and
cross-repo `actions/checkout` cannot reach it), rusty-autossh vendors
`tools/feature-lint/{lint.sh,run.sh,README.md}` from the umbrella into
the port repo. The vendored copy is byte-equal to the umbrella source
of truth as of the freeze commit (post the dev-tooling-allowlist +
benches/tests-search + additive-CHANGELOG-support fixes from rusty-ts
v0.2.0 / E011 Phase 3 iteration 2, the path-sanitization fixes from
rusty-sponge v0.2.0 / E011 Phase 4, and the additional sibling-port
iterations through rusty-pdfgrep v0.2.0 / E011 Phase 10).

## Why no new leaves — explicit rationale

Spec 00011 §Scope Edge Cases permits the zero-leaf path for ports whose
entire surface is tightly-coupled:

> A port has many tightly-coupled capabilities (e.g., rusty-pdfgrep's
> regex + recursive walking + encryption) — leaves can be coarse-grained
> when finer splits would be artificial. Pragmatism over purity.

rusty-autossh deliberately chooses the zero-leaf path because:

1. The library API surface (`SshSupervisor`, `SshSupervisorBuilder`,
   `MonitorMode`, `CompatibilityMode`, `SupervisorEvent`, `SignalKind`,
   `AutosshError`) requires `tokio` (process + net + signal + sync +
   time + rt + macros + io-util), `thiserror`, and `socket2` always-on.
   None of these are strippable without breaking the public type
   signatures.
2. Every CLI sub-capability (clap argv parsing, completions subcommand,
   tracing-based logging, atomic pidfile writer, Unix daemonize
   double-fork, Windows `DETACHED_PROCESS` self-respawn analogue,
   Strict-mode argv pre-scanner) is already bundled into the v0.1.x
   `cli` umbrella. Splitting any of them out would narrow `cli` —
   violates the additive-v0.2.0 contract per spec 00011 §Brownfield
   Notes.
3. Platform-specific deps (`daemonize` Unix, `windows-sys` Windows)
   are gated via `[target.'cfg(unix)'.dependencies]` and
   `[target.'cfg(windows)'.dependencies]` Cargo tables, NOT Cargo
   features. They are unreachable on the non-applicable platform
   regardless of feature selection. This is a compile-time platform
   gate, not a feature gate; they don't show up as `[features]`
   entries and don't need a `check-leaf-<name>` CI matrix entry.
4. The cost of carving a speculative leaf (cfg-gate scaffolding,
   per-leaf CI matrix entry, README/CHANGELOG row, glossary candidacy)
   outweighs the value when no orthogonal capability exists to gate.
5. The portfolio-wide convention shape (umbrella set, README "Cargo
   Features" section, lint compliance) is preserved verbatim — a
   downstream library consumer reading the README for rusty-autossh
   gets the same one-glance feature matrix UX as one reading
   rusty-figlet, rusty-ts, rusty-sponge, rusty-vipe, rusty-pee,
   rusty-pwgen, rusty-detox, rusty-pv, or rusty-pdfgrep.
6. v0.2.0 is **purely additive**. Every v0.1.x feature is preserved
   verbatim; no SemVer break. Future minor releases can add leaves
   without breaking the v0.2.0 contract: a hypothetical
   `metrics-exporter` v0.3.0 feature emitting Prometheus-style
   supervisor metrics would slot in as `metrics-exporter = ["dep:prometheus"]`
   alongside the existing umbrellas with zero migration cost.
