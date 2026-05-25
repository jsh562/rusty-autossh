//! Hand-rolled Strict-mode argv parser.
//!
//! Per AD-007 + FR-052 + FR-053: clap's diagnostics cannot byte-equal
//! upstream `autossh(1)`. This module implements a tokenizer state machine
//! that:
//!
//! - Accepts the in-scope short flags `-M <port[:echo]>`, `-f`, `-V`, `-1`.
//! - Rejects v0.1-excluded short flags (`-d`, `-D`, `-X`, `-T`, `-a`,
//!   `-N`, `-Y`, `-q`) with `autossh: invalid option -- '<char>'`.
//! - Rejects v0.1-excluded long flags (any `--<name>` not listed) AND the
//!   `completions` subcommand token with
//!   `autossh: unrecognized option '--<flag>'` (or `'completions'` for
//!   the subcommand per Clarifications Q3).
//! - Collects all remaining tokens after the autossh-known prefix as
//!   ssh argv passthrough per FR-012.
//!
//! Full integration with the supervisor + snapshot byte-equality lands in
//! Phase 5 (T070).

use std::ffi::OsString;

use crate::AutosshError;

/// Result of a successful Strict-mode argv parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StrictArgs {
    /// `-M <port[:echo]>` value when supplied.
    pub monitor: Option<String>,
    /// `-f` background flag.
    pub background: bool,
    /// `-V` version-print flag.
    pub version: bool,
    /// `-1` one-shot flag.
    pub one_shot: bool,
    /// Remaining argv tokens passed verbatim to ssh.
    pub ssh_args: Vec<String>,
}

/// Errors surfaced by [`parse_argv`].
#[non_exhaustive]
#[derive(Debug, thiserror::Error)]
pub enum StrictError {
    /// An excluded short flag was supplied. The `String` payload is the
    /// upstream-exact stderr line (per [`format_unknown_flag`]).
    #[error("{0}")]
    UnknownShort(String),
    /// An excluded long flag was supplied. The `String` payload is the
    /// upstream-exact stderr line.
    #[error("{0}")]
    UnknownLong(String),
    /// An expected argument value was missing (e.g. `-M` with no port).
    #[error("autossh: option requires an argument -- '{0}'")]
    MissingValue(char),
    /// Conversion of [`StrictError`] into [`AutosshError`] for the public
    /// API.
    #[error("strict parse failed: {0}")]
    Internal(String),
}

impl From<StrictError> for AutosshError {
    fn from(_err: StrictError) -> Self {
        // Strict errors are formatted at the CLI boundary; the library
        // surface only sees a generic Internal tag.
        AutosshError::Internal("strict parse error")
    }
}

/// Format an unknown-flag stderr line in upstream-`autossh 1.4g` style.
///
/// - Short flag (single dash + one char): `autossh: invalid option -- '<char>'`.
/// - Long flag (`--<name>`): `autossh: unrecognized option '--<name>'`.
/// - Bare subcommand token (e.g. `completions` per Clarifications Q3):
///   `autossh: unrecognized option '<token>'`.
///
/// The literal `autossh:` prefix is preserved (per FR-051 — Strict mode
/// must produce upstream-byte-equal stderr so upstream-peer interop is
/// unaffected).
pub fn format_unknown_flag(token: &str) -> String {
    if let Some(rest) = token.strip_prefix("--") {
        // Long flag.
        format!("autossh: unrecognized option '--{rest}'")
    } else if let Some(rest) = token.strip_prefix('-') {
        // Short flag — take only the first char per getopt's diagnostic
        // format.
        let first = rest.chars().next().unwrap_or('?');
        format!("autossh: invalid option -- '{first}'")
    } else {
        // Bare subcommand token (e.g. `completions` per Clarifications
        // Q3).
        format!("autossh: unrecognized option '{token}'")
    }
}

/// Excluded short flags per spec §Strict-Mode Coverage.
const EXCLUDED_SHORT: &[char] = &['d', 'D', 'X', 'T', 'a', 'N', 'Y', 'q'];

/// Excluded long flags per spec §Strict-Mode Coverage.
///
/// Note: `--strict` and `--no-strict` are NOT in this list — they are
/// consumed by [`crate::mode::resolve`] before strict parsing runs, so the
/// strict parser treats them as transparent no-ops (otherwise the user
/// invoking `rusty-autossh --strict ...` would immediately receive an
/// `autossh: unrecognized option '--strict'` diagnostic, which would be
/// hostile UX).
const EXCLUDED_LONG: &[&str] = &[
    "monitor-port",
    "poll",
    "first-poll",
    "gate-time",
    "max-start",
    "max-lifetime",
    "ssh-path",
    "log-file",
    "pid-file",
    "debug",
    "log-level",
    "background",
    "version",
    "one-shot",
];

/// Long flags that the strict parser silently accepts (consumed earlier
/// by [`crate::mode::resolve`]). They never reach ssh-args passthrough.
const STRICT_NOOP_LONG: &[&str] = &["strict", "no-strict"];

/// Parse a Strict-mode argv.
///
/// Tokens are walked left-to-right:
/// - `-M <val>` / `-Mval` — consumes the next token (or inline value).
/// - `-f` / `-V` / `-1` — boolean flags.
/// - Any other `-X` short flag → [`StrictError::UnknownShort`] (excluded
///   short flag per FR-052).
/// - Any `--<name>` long flag → [`StrictError::UnknownLong`] (excluded
///   long flag per FR-053; covers `completions` subcommand token via
///   Clarifications Q3).
/// - `--` separator → switch to ssh-argv-passthrough mode for all
///   remaining tokens.
/// - Other tokens → ssh-argv-passthrough.
pub fn parse_argv(argv: &[OsString]) -> Result<StrictArgs, StrictError> {
    let mut out = StrictArgs::default();
    let mut i = 0;
    let mut passthrough = false;

    while i < argv.len() {
        let raw = argv[i].clone();
        let Some(tok) = raw.to_str() else {
            // Non-UTF8 token — treat as ssh-args passthrough.
            out.ssh_args.push(raw.to_string_lossy().into_owned());
            i += 1;
            continue;
        };

        if passthrough {
            out.ssh_args.push(tok.to_string());
            i += 1;
            continue;
        }

        if tok == "--" {
            passthrough = true;
            i += 1;
            continue;
        }

        // Long-form flag rejection.
        if let Some(rest) = tok.strip_prefix("--") {
            // Split off `=value` suffix for matching.
            let name = rest.split('=').next().unwrap_or(rest);
            // `--strict`/`--no-strict` are consumed by mode::resolve;
            // treat them as transparent no-ops here.
            if STRICT_NOOP_LONG.contains(&name) {
                i += 1;
                continue;
            }
            if EXCLUDED_LONG.contains(&name) {
                return Err(StrictError::UnknownLong(format_unknown_flag(tok)));
            }
            // Unknown long flag — same diagnostic format.
            return Err(StrictError::UnknownLong(format_unknown_flag(tok)));
        }

        // Short flag handling.
        if let Some(rest) = tok.strip_prefix('-') {
            // `-1` is a flag, not an option group.
            if rest == "1" {
                out.one_shot = true;
                i += 1;
                continue;
            }
            let mut chars = rest.chars();
            let Some(first) = chars.next() else {
                // Bare `-` token → passthrough to ssh.
                out.ssh_args.push(tok.to_string());
                i += 1;
                continue;
            };

            match first {
                'M' => {
                    // Inline value or next token.
                    let inline: String = chars.collect();
                    if !inline.is_empty() {
                        out.monitor = Some(inline);
                    } else if i + 1 < argv.len() {
                        let val = argv[i + 1].to_string_lossy().into_owned();
                        out.monitor = Some(val);
                        i += 1;
                    } else {
                        return Err(StrictError::MissingValue('M'));
                    }
                }
                'f' => out.background = true,
                'V' => out.version = true,
                c if EXCLUDED_SHORT.contains(&c) => {
                    return Err(StrictError::UnknownShort(format_unknown_flag(tok)));
                }
                _ => {
                    // Unknown short flag.
                    return Err(StrictError::UnknownShort(format_unknown_flag(tok)));
                }
            }

            i += 1;
            continue;
        }

        // Bare token: per Clarifications Q3 the `completions` subcommand
        // token in Strict mode is rejected as an unrecognized option.
        if tok == "completions" {
            return Err(StrictError::UnknownLong(format_unknown_flag(tok)));
        }

        // Anything else (e.g. `user@host`, `sleep 60`, etc.) starts the
        // ssh-args passthrough.
        passthrough = true;
        out.ssh_args.push(tok.to_string());
        i += 1;
    }

    Ok(out)
}

#[cfg(test)]
#[allow(non_snake_case)] // Test names mirror upstream short-flag names (-M, -X).
mod tests {
    use super::*;

    fn argv(s: &[&str]) -> Vec<OsString> {
        s.iter().map(|x| OsString::from(*x)).collect()
    }

    #[test]
    fn format_short_unknown_flag_matches_upstream() {
        assert_eq!(format_unknown_flag("-X"), "autossh: invalid option -- 'X'");
    }

    #[test]
    fn format_long_unknown_flag_matches_upstream() {
        assert_eq!(
            format_unknown_flag("--monitor-port"),
            "autossh: unrecognized option '--monitor-port'"
        );
    }

    #[test]
    fn format_bare_subcommand_token_matches_upstream() {
        assert_eq!(
            format_unknown_flag("completions"),
            "autossh: unrecognized option 'completions'"
        );
    }

    #[test]
    fn parses_dash_M_with_separate_value() {
        let args = parse_argv(&argv(&["-M", "20000", "user@host"])).unwrap();
        assert_eq!(args.monitor.as_deref(), Some("20000"));
        assert_eq!(args.ssh_args, vec!["user@host".to_string()]);
    }

    #[test]
    fn parses_dash_M_with_inline_value() {
        let args = parse_argv(&argv(&["-M20000", "user@host"])).unwrap();
        assert_eq!(args.monitor.as_deref(), Some("20000"));
    }

    #[test]
    fn parses_dash_f_dash_one() {
        let args = parse_argv(&argv(&["-f", "-1", "user@host"])).unwrap();
        assert!(args.background);
        assert!(args.one_shot);
    }

    #[test]
    fn rejects_excluded_short_dash_X() {
        let err = parse_argv(&argv(&["-X"])).unwrap_err();
        match err {
            StrictError::UnknownShort(s) => assert_eq!(s, "autossh: invalid option -- 'X'"),
            _ => panic!("expected UnknownShort"),
        }
    }

    #[test]
    fn rejects_excluded_long_monitor_port() {
        let err = parse_argv(&argv(&["--monitor-port", "20000"])).unwrap_err();
        match err {
            StrictError::UnknownLong(s) => {
                assert_eq!(s, "autossh: unrecognized option '--monitor-port'");
            }
            _ => panic!("expected UnknownLong"),
        }
    }

    #[test]
    fn rejects_completions_subcommand() {
        let err = parse_argv(&argv(&["completions", "bash"])).unwrap_err();
        match err {
            StrictError::UnknownLong(s) => {
                assert_eq!(s, "autossh: unrecognized option 'completions'");
            }
            _ => panic!("expected UnknownLong"),
        }
    }

    #[test]
    fn double_dash_starts_passthrough() {
        let args = parse_argv(&argv(&["-f", "--", "--strict", "-X"])).unwrap();
        assert!(args.background);
        assert_eq!(
            args.ssh_args,
            vec!["--strict".to_string(), "-X".to_string()]
        );
    }
}
