# rusty-autossh — CI Runtime Baseline

**Status**: v0.2.0 baseline projection (E011 Phase 11 T125).

**Authority**:
- Spec 00011 SC-010 — wall-clock for the longest-path job MUST stay
  under 25 minutes (HARD gate per Clarifications Q1).
- HINT-010 of spec 00011 — remediation strategies if the gate is
  approached.

## Method

rusty-autossh is a tightly-coupled-capability port per spec 00011 §Scope
Edge Cases. The CI matrix scales with the leaf count; since zero leaves
are carved, Tier 4 (`check-leaf-<leaf>`) is empty, leaving a
smaller-than-average matrix vs the reference rusty-figlet port (which has
5 v0.2.0 leaves and 5 corresponding Tier 4 jobs). The matrix shape matches
the rusty-sponge v0.2.0, rusty-vipe v0.2.0, rusty-pee v0.2.0, rusty-pwgen
v0.2.0, rusty-detox v0.2.0, rusty-pv v0.2.0, and rusty-pdfgrep v0.2.0
baselines (all zero-leaf ports; rusty-detox additionally retains its
v0.1.x `inline-detox` feature under Tier 4, but rusty-autossh has no
such pre-existing leaf and its Tier 4 is fully empty).

The full v0.2.0 CI matrix for rusty-autossh has these jobs:

| Tier | Job name | OS | Count | Notes |
|------|----------|-----|------:|-------|
| Pre-gates | fmt, clippy, audit, deny, msrv | Linux | 5 | Identical to v0.1.x (cargo deny retained). |
| Tier 1 | test-default | linux x86_64 + aarch64 + macos x86_64 + macos aarch64 + windows x86_64 | 5 | Full DDR-003 cross-compile matrix (preserves v0.1.x coverage). The Linux x86_64 entry also runs the SC-002 default-install smoke (drives `--help`, `--version`, and the completions subcommand). |
| Tier 2 | test-no-default | Linux x86_64 | 1 | Bare library + SC-001 dep-tree audit. |
| Tier 3 | test-autossh-classic, test-autossh-minimal | Linux x86_64 | 2 | Preset bundle install + SC-003 size check. |
| Tier 3 (SC-004) | test-keeplist | Linux x86_64 | 1 | Keep-list workaround install smoke. |
| Tier 4 | (none) | — | 0 | **Zero leaves carved — Tier 4 intentionally empty per docs/feature-layout.md.** |
| Tier 5 | lint-convention | Linux x86_64 | 1 | Vendored `tools/feature-lint/run.sh` invocation. |
| Optional | convention-lint-self-test | Linux x86_64 | 1 | `workflow_dispatch` only — does NOT run on PR/main. |
| Legacy | library-no-default-features | Linux x86_64 | 1 | Retained v0.1.x parity gate. |
| Publish | publish-dry-run | Linux x86_64 | 1 | Runs after every tier above; cargo publish --dry-run. |

**Total scheduled jobs per PR/main push**: 5 (pre-gates) + 5 (Tier 1) + 1
(Tier 2) + 2 (Tier 3) + 1 (SC-004) + 1 (Tier 5) + 1 (legacy) + 1
(publish-dry-run) = **17 jobs**.

The `convention-lint-self-test` job runs only on `workflow_dispatch`
and is excluded from the steady-state PR/main load.

## Wall-clock projection

The longest single-job wall-clock is expected to be the `test-default`
matrix entry running `aarch64-unknown-linux-gnu` under `cross`
(cross-compile via QEMU adds ~30-50% on cargo-build wall vs native), or
the Windows x86_64 entry (Windows runners are typically the slowest in
GitHub-hosted matrix). rusty-autossh's heaviest cargo-build path is the
all-features build, which links `tokio` (multi-feature: process + net +
signal + sync + time + rt + macros + io-util), the tracing stack
(`tracing` + `tracing-subscriber` + `tracing-appender`), `clap` +
`clap_complete`, plus the platform-conditional `daemonize` (Unix) /
`windows-sys` (Windows) self-respawn deps.

Per the v0.1.0 baseline (single matrix entry per target, no Cargo
Features Convention overlays):

| Target | v0.1.0 wall-clock (median of 3 runs, projected) |
|--------|---------------------------------------------:|
| linux x86_64 (native) | ~4 min |
| linux aarch64 (cross via QEMU) | ~8-10 min |
| macos x86_64 (build-only) | ~3 min |
| macos aarch64 | ~4 min |
| windows x86_64 | ~6-8 min |

The v0.2.0 matrix adds 5 Tier 2/3/SC-004/5 Linux x86_64 jobs that all
run in parallel with Tier 1. Each is bounded by a single Linux
`cargo build/test --features <bundle>` cycle (~2-3 min each on a warm
rust-cache). Net effect on the longest-path wall-clock: negligible
(the critical path remains the slowest Tier 1 cross-compile or the
Windows runner).

**Projected longest-path wall-clock for v0.2.0**: ~10-12 minutes
(aarch64-linux cross via QEMU; tokio + tracing compile dominate),
well under the 25-minute HARD gate per SC-010.

## Empirical local capture (cargo check baseline)

A local `cargo check` matrix run was performed on Windows during T125
validation to confirm Cargo.lock regeneration and per-feature
compilability. Sanitized output (all on a warm dep-graph after the
initial cold compile):

| Variant | Wall-clock |
|---------|-----------:|
| `cargo check --all-features` (warm) | ~1.0 s |
| `cargo check --no-default-features` (warm) | ~0.5 s |
| `cargo check --no-default-features --features cli` (warm) | ~0.8 s |
| `cargo check --no-default-features --features autossh-classic` (warm) | ~0.6 s |
| `cargo check --no-default-features --features autossh-minimal` (warm) | ~0.7 s |
| `cargo check --no-default-features --features full` (warm) | ~0.7 s |

The cold compile of `--no-default-features` populates the `tokio` +
`thiserror` + `socket2` dep graph (the always-on library stack), which
takes the longest single step locally (~30-60 s on a warm registry).

These are local-machine checks (toolchain pinned via
`rust-toolchain.toml` at 1.85) intended only as a quick sanity check
that the per-feature graph compiles. The CI matrix uses
`cargo build/test` on cold rust-caches per target and will be slower;
the empirical CI capture (3 full runs, median of the slowest matrix
entry) is deferred until the v0.2.0 PR opens its first CI run.

## Empirical capture (deferred)

Per HINT-010 the empirical capture (3 full CI runs, median of the
slowest matrix entry) is deferred until the v0.2.0 PR opens its first
CI run. The projection above is well under the 25-minute gate; if the
first 3 CI runs show otherwise, this document is updated with the
empirical numbers and remediation steps per HINT-010(a/b/c).

## Remediation strategies (HINT-010, not yet needed)

If the empirical wall-clock approaches the 25-minute gate:

1. **Cache aggressively** — the existing `Swatinem/rust-cache@v2`
   keyed by target already does this; verify cache hit-rate.
2. **Drop `cross` for aarch64-linux** — use `actions/setup-qemu` +
   native arm runners if available, or accept the cross-compile-only
   (no test) for that target.
3. **Move per-preset SC-003 install + size check to a single
   matrix-driven job** — currently each Tier 3 job redundantly
   compiles the binary. A single matrix-strategy job could
   parallelize across preset bundles with one shared rust-cache.
4. **Trim the `tokio` + `tracing` cold-compile path** — these are the
   dominant cold-build cost for rusty-autossh. Cache the registry /
   dep graph aggressively; consider a workspace-level `cargo nextest`
   parallelization if a future iteration adds significant test
   surface.
5. **Build `test-default` and `test-no-default` share the same
   cross-compile-target cache** — currently both jobs check out fresh;
   sharing the rust-cache key (with care for feature-set divergence)
   could shave a few minutes off the aggregate.

None of these are needed at v0.2.0; capturing here for future
maintenance.

## SC-010 verdict

Projected longest-path wall-clock ~10-12 min for the v0.2.0 matrix
shape on rusty-autossh is well under the 25-minute HARD gate. SC-010
verdict: **PROJECTED PASS** (empirical capture pending first CI run).

## Phase 11 note — LAST sibling port

rusty-autossh is the LAST sibling port in the E011 Phase 3..11 rollout.
With v0.2.0 publish, all 10 portfolio ports converge on the
portfolio-wide convention shape, satisfying spec 00011 SC-006 (10/10
ports shipped). Phase 12 (portfolio wrap-up) commences after T127
publish.
