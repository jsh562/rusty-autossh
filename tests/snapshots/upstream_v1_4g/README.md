# Strict-Mode Snapshot Suite — upstream `autossh 1.4g`

This directory hosts the captured upstream `autossh 1.4g` stderr fixtures
that drive `tests/compat_strict.rs` byte-equality assertions per
[`plan.md`](../../../../rusty/specs/00010-autossh-port/plan.md)
§Strict-Mode Snapshot Capture & Coverage + spec SC-005 + SC-007 + FR-051.
Provenance lives in [`PROVENANCE.txt`](PROVENANCE.txt) (per HINT-004 + T019).

## Refresh Policy

Snapshot refresh is a **deliberate maintenance step on upstream version bump**,
NOT a silent CI refresh. CI consumes the committed snapshots as-is; any
change to snapshot bytes MUST land in a commit whose message carries the
`[snapshot-refresh]` tag so the diff is explicit and reviewable. CI does not
regenerate snapshots automatically.

When upstream `autossh` bumps to a new release (e.g., 1.4h or 2.0), the
maintainer:

1. Updates `PROVENANCE.txt` to record the new upstream version, capture host,
   capture date, and the upstream source location.
2. Renames this directory from `upstream_v1_4g/` to `upstream_vX_Y_Z/` to
   match the new version. Also update `tests/common/mod.rs::strip_for_snapshot`
   if the program-name token in any captured stderr changes.
3. Re-runs the capture procedure from `PROVENANCE.txt` against the new upstream.
4. Commits the new snapshots + manifest + provenance under a single
   `[snapshot-refresh] upstream → vX.Y.Z` commit.

## Minimum Matrix (per spec Clarifications Q4 / plan §Strict-Mode Snapshot Capture & Coverage)

The committed Strict-mode snapshot scenarios MUST cover at minimum
**≥20 fixtures**:

### 4 in-scope flags

The four short flags that Strict mode accepts. Each fixture exercises the
flag through the supervisor without any failures (or, where applicable, the
version-print path):

- `-M <port>` (monitor port)
- `-f` (background / daemonize)
- `-V` (version)
- `-1` (one-shot)

### 8 excluded short flags

Sampling the getopt single-char surface across the debug / forwarding /
auth / quiet axes. Each fixture produces upstream's exact stderr
`autossh: invalid option -- '<char>'` and exits 1 (per FR-052):

- `-d` (upstream debug)
- `-D`
- `-X`
- `-T`
- `-a`
- `-N`
- `-Y`
- `-q`

### 8 excluded long flags

Each `--<name>` Rust-native default-mode flag rejected in Strict (per
FR-053). Each fixture produces upstream's exact stderr
`autossh: unrecognized option '--<name>'` and exits 1:

- `--monitor-port`
- `--poll`
- `--first-poll`
- `--gate-time`
- `--max-start`
- `--max-lifetime`
- `--ssh-path`
- `--log-file`

### argv-passthrough boundary cases

Plus combinations exercising argv-passthrough behavior — the `--` separator
plus `-o "ProxyJump=..."` quoted ssh options.

## Snapshot File Layout

Each scenario has:

```
outputs/<scenario_id>.stderr     ← captured upstream stderr (LF eol; .gitattributes enforces)
manifest.toml                    ← scenario list (single file; sections per scenario)
```

The manifest schema (TOML; see `manifest.toml` for the live skeleton):

```toml
upstream_version = "1.4g"
capture_host     = "Debian 12 (Bookworm) x86_64"
capture_date     = "2026-MM-DD"

[[scenario]]
id          = "invalid_option_X"
args        = ["-X"]
stderr_file = "outputs/invalid_option_X.stderr"
exit_code   = 1
```

## Snapshot-Strip Helper

`tests/common/mod.rs::strip_for_snapshot(raw: &[u8]) -> Vec<u8>` is the SOLE
canonical snapshot-strip helper. At v0.1.0 it is a passthrough — per
FR-051 the literal `autossh:` prefix is preserved verbatim in Strict-mode
stderr (the wire-format parity guarantee). The helper exists as a single
extension point for future strip rules; per-test ad-hoc normalization is
FORBIDDEN.

## Status (Phase 1 Setup)

T019 (`PROVENANCE.txt`) and T021 (`manifest.toml` stub) are **DEFERRED**
in Phase 1 because the Windows development environment cannot run upstream
`autossh 1.4g` natively. Capture happens on a Linux host (Debian/Ubuntu
`apt install autossh=1.4g` or built-from-source) during T086 + T087 + T088
+ T089 before the v0.1.0 release. Until then `tests/compat_strict.rs`
byte-equal assertions are skipped via `#[ignore]` or a feature flag.
