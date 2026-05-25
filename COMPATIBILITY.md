# Compatibility — `rusty-autossh` vs upstream `autossh 1.4g`

Reference baseline: Carson Harding's `autossh 1.4g`
(<https://www.harding.motd.ca/autossh/>). Strict-mode byte-equal stderr is
enforced for the in-scope flag matrix; all other behavior is best-effort
parity unless explicitly enumerated below.

This document is the canonical source of the v0.1.0 compatibility matrix.
The README links here; CHANGELOG references the BREAKING-CHANGE block.

## Status (v0.1.0 — Polish phase finalized)

This document is the canonical per-flag compatibility matrix for v0.1.0
(populated during Polish phase T147). The BREAKING-CHANGE block and
excluded-flag enumeration below remain load-bearing references from README
and CHANGELOG.

## BREAKING-CHANGE block (v0.1.0)

The v0.1.0 release intentionally diverges from upstream `autossh 1.4g` in the
following enumerated ways:

- **(a) Windows `-f`** uses `CreateProcessW(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)`
  self-respawn instead of fork. The foreground process closes its monitor-port
  `TcpListener` handles BEFORE invoking `CreateProcessW` (per FR-021 +
  Clarifications Q5); the detached child re-binds the listeners from scratch
  as part of its own startup. `SO_EXCLUSIVEADDRUSE` is left unset (the
  Windows default) so the brief overlap window does not produce
  `WSAEADDRINUSE`.
- **(b) `AUTOSSH_NTSERVICE`** (Cygwin NT-service mode) is NOT implemented.
  Users wanting Windows Service integration should use `sc.exe create` or
  NSSM against the foreground `rusty-autossh` binary.
- **(c) SIGUSR1 + SIGHUP** have no Windows equivalents. Workaround on
  Windows: `taskkill /PID <ssh-pid>` triggers respawn via the
  SIGCHLD-equivalent code path (Tokio's `Child::wait` future resolves with
  the externally-killed child's exit status, supervisor respawns per the
  HINT-018 truth table).
- **(d) Windows ssh-child termination** uses `TerminateProcess(child_handle, 1)`
  (immediate) with NO 10-second SIGTERM grace window. Unix retains the
  upstream-style SIGTERM + 10s + SIGKILL fallback per FR-040.
- **(e) MSRV is Rust 1.85** (edition 2024) per `project-instructions.md`
  portfolio convention.

## Excluded short flags (Strict mode rejection per FR-052)

Strict mode emits `autossh: invalid option -- '<char>'` for each of these
single-char flags and exits 1:

| Flag | Upstream meaning (autossh 1.4g) | rusty-autossh disposition |
|------|----------------------------------|---------------------------|
| `-d` | Debug mode                       | Excluded v0.1; use `--debug` in Default |
| `-D` | (reserved upstream)              | Excluded v0.1                            |
| `-X` | (reserved upstream)              | Excluded v0.1                            |
| `-T` | (reserved upstream)              | Excluded v0.1                            |
| `-a` | (reserved upstream)              | Excluded v0.1                            |
| `-N` | (reserved upstream)              | Excluded v0.1                            |
| `-Y` | (reserved upstream)              | Excluded v0.1                            |
| `-q` | (reserved upstream)              | Excluded v0.1                            |

## Excluded long flags (Strict mode rejection per FR-053)

Strict mode emits `autossh: unrecognized option '--<name>'` for each of these
long flags (which exist only in Default mode) and exits 1:

| Flag              | Default-mode meaning                  |
|-------------------|----------------------------------------|
| `--monitor-port`  | Alias for `-M <port[:echo]>`           |
| `--poll`          | Override `AUTOSSH_POLL`                |
| `--first-poll`    | Override `AUTOSSH_FIRST_POLL`          |
| `--gate-time`     | Override `AUTOSSH_GATETIME`            |
| `--max-start`     | Override `AUTOSSH_MAXSTART`            |
| `--max-lifetime`  | Override `AUTOSSH_MAXLIFETIME`         |
| `--ssh-path`      | Override `AUTOSSH_PATH`                |
| `--log-file`      | Override `AUTOSSH_LOGFILE`             |

The `completions <shell>` subcommand is likewise rejected by Strict mode
(treated as an unrecognized option per FR-053 + Clarifications Q3 +
US7 AS3): `autossh: unrecognized option 'completions'`.

## Per-flag matrix

`In` = accepted, `Out` = rejected, `Pass` = passed through to `ssh(1)`,
`Same` = upstream behavior preserved.

### Upstream `autossh 1.4g` flags

| Flag                | Default | Strict | Notes                                                |
|---------------------|---------|--------|------------------------------------------------------|
| `-M <port>`         | In      | In     | Port pair (`port`, `port+1`); two TCP listeners.     |
| `-M <port>:<echo>`  | In      | In     | Single listener; `-L port:127.0.0.1:echo` injected.  |
| `-M 0`              | In      | In     | No monitor port; supervisor respawns on non-zero exit only. |
| `-f`                | In      | In     | Daemonize. Unconditionally sets `AUTOSSH_GATETIME=0` (FR-022). |
| `-V`                | In      | In     | Print version + exit (FR-010).                       |
| `-1`                | In      | In     | One-shot; do not respawn after first non-zero exit.  |
| `-d`                | Out     | Out    | Excluded; use `--debug` in Default mode (FR-052).    |
| `-D`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |
| `-X`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |
| `-T`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |
| `-a`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |
| `-N`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |
| `-Y`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |
| `-q`                | Out     | Out    | Excluded (reserved upstream) (FR-052).               |

### `rusty-autossh`-added long flags (Default-mode only)

| Flag                  | Default | Strict | Notes                                              |
|-----------------------|---------|--------|----------------------------------------------------|
| `--monitor-port`      | In      | Out    | Alias for `-M`; rejected as unrecognized (FR-053). |
| `--background`        | In      | Out    | Alias for `-f`.                                    |
| `--version`           | In      | Out    | Alias for `-V`.                                    |
| `--one-shot`          | In      | Out    | Alias for `-1`.                                    |
| `--poll <S>`          | In      | Out    | Overrides `AUTOSSH_POLL`.                          |
| `--first-poll <S>`    | In      | Out    | Overrides `AUTOSSH_FIRST_POLL`.                    |
| `--gate-time <S>`     | In      | Out    | Overrides `AUTOSSH_GATETIME`; `-f` still forces 0. |
| `--max-start <N>`     | In      | Out    | Overrides `AUTOSSH_MAXSTART`; `-1` = unlimited.    |
| `--max-lifetime <S>`  | In      | Out    | Overrides `AUTOSSH_MAXLIFETIME`.                   |
| `--ssh-path <P>`      | In      | Out    | Overrides `AUTOSSH_PATH` (verbatim).               |
| `--pid-file <P>`      | In      | Out    | Overrides `AUTOSSH_PIDFILE`.                       |
| `--log-file <P>`      | In      | Out    | Overrides `AUTOSSH_LOGFILE`.                       |
| `--debug`             | In      | Out    | Enable debug-level tracing.                        |
| `--log-level <L>`     | In      | Out    | Set log level.                                     |
| `--strict`            | In      | In     | Force Strict mode.                                 |
| `--no-strict`         | In      | In     | Force Default mode.                                |
| `completions <shell>` | In      | Out    | Default-mode subcommand only (FR-053, US7 AS3).    |

## argv passthrough boundary

`rusty-autossh` peels its own recognized flags off the left of argv and
passes the remainder verbatim to `ssh(1)`. The standard `--` end-of-options
separator is honored: tokens after `--` are passed without further parsing
even if they would otherwise collide with rusty-autossh long flags. Strict
mode treats `--` identically to upstream `autossh 1.4g` per FR-051.

```text
rusty-autossh -M 20000 -L 8080:localhost:80 -- -o "ProxyJump=jump.example.com" user@host
              \_______/ \_________________/    \_____________________________________________/
                 own              own                          → ssh argv (passthrough)
```

Quoting boundary cases (`-o "ProxyJump=..."`, embedded spaces, equals-form
ssh flags) match upstream `autossh 1.4g` behavior in Strict mode and are
captured under `tests/snapshots/upstream_v1_4g/` per spec Clarifications Q4
(snapshot capture itself DEFERRED pending Linux host; the parser path is
unit-tested under `tests/compat_default.rs` + `tests/compat_strict.rs`).

## Windows kill-grace absence (per FR-043)

Upstream `autossh` on Unix uses `SIGTERM` then waits up to 10 seconds
before escalating to `SIGKILL`. `rusty-autossh` retains this behavior on
Unix (FR-040) but on Windows uses `TerminateProcess(child_handle, 1)`
**immediately**, with NO grace window. This is documented in
CHANGELOG.md BREAKING-CHANGE (d) and is intentional because Windows has no
unified soft-termination primitive equivalent to POSIX SIGTERM that
`DETACHED_PROCESS` children honor uniformly. Operators who need a graceful
shutdown story on Windows should use `taskkill /PID <ssh-pid>` (without
`/F`) against the ssh child directly, which lets ssh exit cleanly and the
supervisor observe the SIGCHLD-equivalent path via `Child::wait`.

## Library API compatibility

The library API (`SshSupervisor`, `SshSupervisorBuilder`, `MonitorMode`,
`SupervisorEvent`, `AutosshError`) is governed by SemVer. `MonitorMode`,
`SupervisorEvent`, and `AutosshError` are all `#[non_exhaustive]` —
downstream callers MUST use a wildcard `_ =>` arm in exhaustive matches.
Adding a variant to any of these enums is a MINOR bump; removing or
renaming a variant or a public type is a MAJOR bump (lockstep with the
CLI surface per the SemVer Bump Policy in `docs/DESIGN.md`).
