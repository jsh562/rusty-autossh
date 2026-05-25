//! Default-mode CLI parsing via clap-derive.
//!
//! Per FR-010 + FR-011 + FR-070 + AD-016 + HINT-008 this module exposes:
//!
//! - [`Cli`] — clap-derive struct with short/long flags + Rust-native
//!   long-form aliases + `--strict`/`--no-strict`.
//! - [`Subcommand::Completions`] — `completions <shell>` per FR-071.
//! - [`split_autossh_args`] — argv splitter that separates the autossh
//!   flag prefix from the ssh-passthrough remainder per HINT-008.
//! - [`apply_dash_f_overrides`] — FR-022 unconditional gate-time-zero
//!   override at the CLI boundary.

use std::time::Duration;

use clap::{Parser, Subcommand as ClapSubcommand, ValueEnum};

use crate::clock::PollClock;

/// `completions <shell>` target shell selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    /// Bash completion script.
    Bash,
    /// Zsh completion script.
    Zsh,
    /// Fish completion script.
    Fish,
    /// PowerShell completion script.
    Powershell,
}

/// Top-level CLI parsed via clap-derive.
///
/// Short flags + long-form aliases per FR-010; Rust-native long-form flags
/// per FR-011. Strict-mode toggle pair per FR-050.
#[derive(Debug, Parser)]
#[command(
    name = "rusty-autossh",
    bin_name = "rusty-autossh",
    version,
    about = "Keep an SSH tunnel alive across drops (Rust port of autossh(1))",
    disable_help_subcommand = true,
    disable_version_flag = true,
    trailing_var_arg = true,
    allow_hyphen_values = true
)]
pub struct Cli {
    /// `-M <PORT[:ECHO]>` / `--monitor-port`: monitor port (0 disables).
    #[arg(short = 'M', long = "monitor-port", value_name = "PORT[:ECHO]")]
    pub monitor: Option<String>,

    /// `-f` / `--background`: daemonize to background.
    #[arg(short = 'f', long = "background")]
    pub background: bool,

    /// `-V` / `--version`: print version + exit.
    #[arg(short = 'V', long = "version", action = clap::ArgAction::Version)]
    pub print_version: Option<bool>,

    /// `-1` / `--one-shot`: exit non-zero on first connection failure.
    #[arg(short = '1', long = "one-shot")]
    pub one_shot: bool,

    /// `--poll <SECS>`: heartbeat interval (default 600s).
    #[arg(long = "poll", value_name = "SECS")]
    pub poll: Option<u64>,

    /// `--first-poll <SECS>`: initial poll delay.
    #[arg(long = "first-poll", value_name = "SECS")]
    pub first_poll: Option<u64>,

    /// `--gate-time <SECS>`: min lifetime before retry counts as failure.
    #[arg(long = "gate-time", value_name = "SECS")]
    pub gate_time: Option<u64>,

    /// `--max-start <N>`: consecutive-retry cap (-1 = unlimited).
    #[arg(long = "max-start", value_name = "N", allow_negative_numbers = true)]
    pub max_start: Option<i64>,

    /// `--max-lifetime <SECS>`: total-runtime cap (0 = unlimited).
    #[arg(long = "max-lifetime", value_name = "SECS")]
    pub max_lifetime: Option<u64>,

    /// `--ssh-path <PATH>`: override ssh binary (else `AUTOSSH_PATH` / PATH).
    #[arg(long = "ssh-path", value_name = "PATH")]
    pub ssh_path: Option<std::path::PathBuf>,

    /// `--pid-file <PATH>`: override `AUTOSSH_PIDFILE`.
    #[arg(long = "pid-file", value_name = "PATH")]
    pub pid_file: Option<std::path::PathBuf>,

    /// `--log-file <PATH>`: override `AUTOSSH_LOGFILE`.
    #[arg(long = "log-file", value_name = "PATH")]
    pub log_file: Option<std::path::PathBuf>,

    /// `--debug`: enable debug logging.
    #[arg(long = "debug")]
    pub debug: bool,

    /// `--log-level <LEVEL>`: explicit log level (trace/debug/info/warn/error).
    #[arg(long = "log-level", value_name = "LEVEL")]
    pub log_level: Option<String>,

    /// `--strict`: force Strict (upstream-compat) mode.
    #[arg(long = "strict", conflicts_with = "no_strict")]
    pub strict: bool,

    /// `--no-strict`: force Default mode.
    #[arg(long = "no-strict")]
    pub no_strict: bool,

    /// Subcommand (currently only `completions <shell>` per FR-071).
    #[command(subcommand)]
    pub command: Option<Subcommand>,

    /// Remaining positional argv passed verbatim to ssh.
    #[arg(trailing_var_arg = true)]
    pub ssh_args: Vec<String>,
}

/// Top-level subcommands.
#[derive(Debug, ClapSubcommand)]
pub enum Subcommand {
    /// Emit a shell completion script to stdout.
    Completions {
        /// Target shell.
        shell: Shell,
    },
}

/// Split argv left-to-right into (autossh-flags, ssh-passthrough-args) per
/// HINT-008. Stops accumulating into autossh-flags on the first
/// unrecognized token or explicit `--` separator.
///
/// This is a complementary helper for callers that need the split BEFORE
/// invoking clap (e.g. to feed Strict mode's hand-rolled parser without
/// clap's transformations).
pub fn split_autossh_args(argv: &[String]) -> (Vec<String>, Vec<String>) {
    let mut autossh = Vec::new();
    let mut ssh = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let tok = &argv[i];
        if tok == "--" {
            ssh.extend(argv[i + 1..].iter().cloned());
            break;
        }
        if is_autossh_known_flag(tok) {
            autossh.push(tok.clone());
            // Some flags consume the next token as a value; collect it
            // into the autossh prefix too.
            if flag_takes_value(tok) && i + 1 < argv.len() {
                autossh.push(argv[i + 1].clone());
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        // First unrecognized token starts the ssh-args passthrough.
        ssh.extend(argv[i..].iter().cloned());
        break;
    }
    (autossh, ssh)
}

fn is_autossh_known_flag(tok: &str) -> bool {
    matches!(
        tok,
        "-M" | "-f"
            | "-V"
            | "-1"
            | "--monitor-port"
            | "--background"
            | "--version"
            | "--one-shot"
            | "--poll"
            | "--first-poll"
            | "--gate-time"
            | "--max-start"
            | "--max-lifetime"
            | "--ssh-path"
            | "--pid-file"
            | "--log-file"
            | "--debug"
            | "--log-level"
            | "--strict"
            | "--no-strict"
    ) || tok.starts_with("--monitor-port=")
        || tok.starts_with("--poll=")
        || tok.starts_with("--first-poll=")
        || tok.starts_with("--gate-time=")
        || tok.starts_with("--max-start=")
        || tok.starts_with("--max-lifetime=")
        || tok.starts_with("--ssh-path=")
        || tok.starts_with("--pid-file=")
        || tok.starts_with("--log-file=")
        || tok.starts_with("--log-level=")
        || tok.starts_with("-M") // inline `-M20000`
}

fn flag_takes_value(tok: &str) -> bool {
    matches!(
        tok,
        "-M" | "--monitor-port"
            | "--poll"
            | "--first-poll"
            | "--gate-time"
            | "--max-start"
            | "--max-lifetime"
            | "--ssh-path"
            | "--pid-file"
            | "--log-file"
            | "--log-level"
    )
}

/// FR-022 unconditional override: when `-f` is supplied set
/// `gate_time = Duration::ZERO` on the resolved [`PollClock`].
pub fn apply_dash_f_overrides(clock: &mut PollClock, dash_f: bool) {
    if dash_f {
        clock.gate_time = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_separator_dash_dash() {
        let argv = vec![
            "-M".to_string(),
            "20000".to_string(),
            "--".to_string(),
            "--strict".to_string(),
            "user@host".to_string(),
        ];
        let (a, s) = split_autossh_args(&argv);
        assert_eq!(a, vec!["-M".to_string(), "20000".to_string()]);
        assert_eq!(s, vec!["--strict".to_string(), "user@host".to_string()]);
    }

    #[test]
    fn split_first_unrecognized_starts_ssh_args() {
        let argv = vec![
            "-f".to_string(),
            "user@host".to_string(),
            "-L".to_string(),
            "8080:localhost:80".to_string(),
        ];
        let (a, s) = split_autossh_args(&argv);
        assert_eq!(a, vec!["-f".to_string()]);
        assert_eq!(
            s,
            vec![
                "user@host".to_string(),
                "-L".to_string(),
                "8080:localhost:80".to_string(),
            ]
        );
    }

    #[test]
    fn apply_dash_f_zeros_gate_time() {
        let mut clock = PollClock::default();
        apply_dash_f_overrides(&mut clock, true);
        assert_eq!(clock.gate_time, Duration::ZERO);
    }
}
