//! `rusty-autossh` CLI binary entry.
//!
//! Per HINT-008 + HINT-019: split argv into (autossh-flags, ssh-passthrough)
//! at the boundary; dispatch on [`CompatibilityMode`] resolution to either
//! the Default-mode supervisor (clap path) or the Strict-mode path
//! (hand-rolled parser). On panic the runtime's default exit code 101
//! surfaces; on [`AutosshError`] the dispatcher maps to exit codes 1
//! (operational failure) or 2 (startup error).
//!
//! Phase 2 stub: dispatch is wired but `SshSupervisor::run` is a stub
//! until Phase 3 US1 T051 completes the Default-mode pipeline.

use std::process::ExitCode;

#[cfg(feature = "cli")]
fn main() -> ExitCode {
    rusty_autossh_cli::run()
}

#[cfg(not(feature = "cli"))]
fn main() -> ExitCode {
    eprintln!("rusty-autossh: the `cli` feature is required to use the binary");
    ExitCode::from(2)
}

#[cfg(feature = "cli")]
mod rusty_autossh_cli {
    use std::ffi::OsString;
    use std::process::ExitCode;

    use clap::Parser;

    use rusty_autossh::cli::{Cli, Subcommand};
    use rusty_autossh::clock::EnvSnapshot;
    use rusty_autossh::mode::resolve as resolve_mode;
    use rusty_autossh::strict::parse_argv as strict_parse;
    use rusty_autossh::{AutosshError, CompatibilityMode};

    /// CLI dispatcher entry. Returns the process exit code.
    pub fn run() -> ExitCode {
        let argv: Vec<OsString> = std::env::args_os().collect();
        let argv0 = argv
            .first()
            .cloned()
            .unwrap_or_else(|| OsString::from("rusty-autossh"));
        let env = EnvSnapshot::from_process_env();

        // Mode resolution per AD-006.
        let mode = resolve_mode(&argv[1..], &env, &argv0);

        match mode {
            CompatibilityMode::Strict => run_strict(&argv[1..]),
            CompatibilityMode::Default => run_default(),
            // Required wildcard for #[non_exhaustive] CompatibilityMode
            // per AD-014. Future modes default to Default-mode dispatch.
            _ => run_default(),
        }
    }

    fn run_default() -> ExitCode {
        // Use clap-derive to parse. clap handles --help / --version /
        // unknown-flag diagnostics in Default mode.
        let cli = match Cli::try_parse() {
            Ok(c) => c,
            Err(e) => {
                // clap maps usage errors to exit 2, others to 0
                // (--help / --version). Preserve clap's exit code.
                let code = e.exit_code() as u8;
                let _ = e.print();
                return ExitCode::from(code);
            }
        };

        // Subcommand dispatch (FR-071 completions).
        if let Some(Subcommand::Completions { shell }) = cli.command {
            return emit_completions(shell);
        }

        // Resolve `MonitorMode` from the `-M`/`--monitor-port` flag value
        // (or AUTOSSH_PORT env var if the flag is absent).
        let monitor_mode = match parse_monitor_value(cli.monitor.as_deref()) {
            Ok(m) => m,
            Err(msg) => {
                eprintln!("rusty-autossh: {msg}");
                return ExitCode::from(2);
            }
        };

        // Resolve `PollClock` from env + flags + `-f` override per FR-022.
        let env = rusty_autossh::clock::EnvSnapshot::from_process_env();
        let flags = rusty_autossh::clock::ClockFlags {
            poll: cli.poll.map(std::time::Duration::from_secs),
            first_poll: cli.first_poll.map(std::time::Duration::from_secs),
            gate_time: cli.gate_time.map(std::time::Duration::from_secs),
            max_start: cli
                .max_start
                .map(|n| if n < 0 { None } else { Some(n as u32) }),
            max_lifetime: cli.max_lifetime.map(|n| {
                if n == 0 {
                    None
                } else {
                    Some(std::time::Duration::from_secs(n))
                }
            }),
        };
        let clock = rusty_autossh::clock::PollClock::resolve_from_env_and_flags(
            &env,
            &flags,
            cli.background,
        );

        let mut builder = rusty_autossh::SshSupervisorBuilder::new()
            .ssh_args(cli.ssh_args)
            .monitor_mode(monitor_mode)
            .poll(clock.poll)
            .first_poll(clock.first_poll)
            .gate_time(clock.gate_time)
            .max_start(clock.max_start)
            .max_lifetime(clock.max_lifetime)
            .one_shot(cli.one_shot);

        if let Some(path) = cli.ssh_path {
            builder = builder.ssh_path(path);
        }
        if let Some(msg) = env.vars.get("AUTOSSH_MESSAGE").and_then(|v| v.to_str()) {
            builder = builder.message(msg.to_string());
        }

        // T106 + T107: pidfile + logfile path resolution. CLI flag wins
        // over env var per FR-030 / FR-031.
        if let Some(path) = pidfile_path_from_cli_or_env(&cli.pid_file, &env) {
            builder = builder.pidfile_path(path);
        }
        if let Some(path) = logfile_path_from_cli_or_env(&cli.log_file, &env) {
            builder = builder.logfile_path(path);
        }

        // T104 + T105: daemonize when `-f` is supplied. Must happen
        // BEFORE the supervisor is built / run so the post-daemon process
        // owns the pidfile + logfile + supervisor loop. Per HINT-011
        // step 5.
        if cli.background {
            if let Err(e) = perform_daemonize_default(cli.log_file.clone(), &env) {
                return map_error(e);
            }
        }

        let supervisor_result = builder.build();
        match supervisor_result {
            Ok(mut s) => match futures_blocking_run(&mut s) {
                Ok(()) => ExitCode::from(0),
                Err(e) => map_error(e),
            },
            Err(e) => map_error(e),
        }
    }

    /// Resolve the pidfile path from `--pid-file` (wins) or
    /// `AUTOSSH_PIDFILE` env var.
    fn pidfile_path_from_cli_or_env(
        cli_path: &Option<std::path::PathBuf>,
        env: &rusty_autossh::clock::EnvSnapshot,
    ) -> Option<std::path::PathBuf> {
        if let Some(p) = cli_path {
            return Some(p.clone());
        }
        env.vars
            .get("AUTOSSH_PIDFILE")
            .map(std::path::PathBuf::from)
    }

    /// Resolve the logfile path from `--log-file` (wins) or
    /// `AUTOSSH_LOGFILE` env var.
    fn logfile_path_from_cli_or_env(
        cli_path: &Option<std::path::PathBuf>,
        env: &rusty_autossh::clock::EnvSnapshot,
    ) -> Option<std::path::PathBuf> {
        if let Some(p) = cli_path {
            return Some(p.clone());
        }
        env.vars
            .get("AUTOSSH_LOGFILE")
            .map(std::path::PathBuf::from)
    }

    /// Perform platform-appropriate daemonization for `-f` per FR-020 /
    /// FR-021. Returns the [`AutosshError::Daemonize`] on failure. On
    /// success, on Unix this function returns only in the daemon child;
    /// on Windows it returns in the foreground process, which then exits
    /// after returning from `run_default` / `run_strict`.
    #[cfg(unix)]
    fn perform_daemonize_default(
        logfile: Option<std::path::PathBuf>,
        env: &rusty_autossh::clock::EnvSnapshot,
    ) -> Result<(), AutosshError> {
        // Honor AUTOSSH_LOGFILE env var when --log-file is not supplied.
        let logfile = logfile.or_else(|| {
            env.vars
                .get("AUTOSSH_LOGFILE")
                .map(std::path::PathBuf::from)
        });
        rusty_autossh::daemonizer::daemonize_unix(None, logfile)
    }

    #[cfg(windows)]
    fn perform_daemonize_default(
        _logfile: Option<std::path::PathBuf>,
        env: &rusty_autossh::clock::EnvSnapshot,
    ) -> Result<(), AutosshError> {
        // Detached-child marker: the foreground process sets this env
        // var before `CreateProcessW` so the child does NOT recurse into
        // detach_windows when it sees `-f` in its inherited argv.
        if env.vars.contains_key("RUSTY_AUTOSSH_DETACHED") {
            // We are the detached child; clear the marker so the
            // supervisor's view of the env stays clean.
            // SAFETY: single-threaded boundary at process startup.
            unsafe {
                std::env::remove_var("RUSTY_AUTOSSH_DETACHED");
            }
            return Ok(());
        }
        // No pre-bound listeners to surrender from this code path — the
        // supervisor binds inside `run`, which runs in the detached child.
        // The foreground process detaches without holding listeners.
        rusty_autossh::daemonizer::detach_windows(Vec::new())?;
        // Windows: the foreground process exits here to surrender the
        // console; the detached child runs independently.
        std::process::exit(0);
    }

    /// Parse the `-M`/`--monitor-port` value into a [`MonitorMode`].
    ///
    /// Accepts `<port>` or `<port>:<echo>`. `<port> = 0` → no monitor
    /// (`MonitorMode::None` per FR-005). A `None` input → `MonitorMode::None`.
    fn parse_monitor_value(val: Option<&str>) -> Result<rusty_autossh::MonitorMode, String> {
        let Some(s) = val else {
            return Ok(rusty_autossh::MonitorMode::None);
        };
        if let Some((p, e)) = s.split_once(':') {
            let port: u16 = p.parse().map_err(|_| format!("invalid -M port '{p}'"))?;
            let echo: u16 = e.parse().map_err(|_| format!("invalid -M echo '{e}'"))?;
            if port == 0 {
                return Ok(rusty_autossh::MonitorMode::None);
            }
            return Ok(rusty_autossh::MonitorMode::Active {
                port,
                echo: Some(echo),
            });
        }
        let port: u16 = s.parse().map_err(|_| format!("invalid -M port '{s}'"))?;
        if port == 0 {
            Ok(rusty_autossh::MonitorMode::None)
        } else {
            Ok(rusty_autossh::MonitorMode::Active { port, echo: None })
        }
    }

    /// Strict-mode dispatcher (T072).
    ///
    /// Per FR-050 + FR-051 + FR-052 + FR-053 + FR-054:
    /// 1. Hand-rolled `strict::parse_argv` BEFORE clap (clap's diagnostics
    ///    cannot byte-equal upstream `autossh`).
    /// 2. On parse error: write byte-exact `autossh: ...` stderr + exit 1.
    /// 3. On parse ok with `-V`: print version + exit 0.
    /// 4. Build Supervisor with `CompatibilityMode::Strict` + `-f`
    ///    forces `AUTOSSH_GATETIME=0` per FR-022 + Clarifications Q6.
    /// 5. Strict mode SKIPS the Default-mode ISO timestamp prefix on log
    ///    lines (FR-054) — this is satisfied by NOT installing a tracing
    ///    subscriber in this branch (Default mode would set one up in
    ///    `logging::init_logfile`; Strict opens raw OpenOptions::append).
    fn run_strict(args: &[OsString]) -> ExitCode {
        let parsed = match strict_parse(args) {
            Ok(p) => p,
            Err(e) => {
                // Byte-exact upstream stderr per FR-052 / FR-053; `e`'s
                // Display impl emits the `autossh: ...` line verbatim.
                eprintln!("{e}");
                return ExitCode::from(1);
            }
        };

        // -V: print version + exit 0 (matches upstream + FR-010).
        if parsed.version {
            println!("rusty-autossh {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::from(0);
        }

        // Resolve MonitorMode from the parsed `-M` value.
        let monitor_mode = match parse_monitor_value(parsed.monitor.as_deref()) {
            Ok(m) => m,
            Err(msg) => {
                eprintln!("autossh: {msg}");
                return ExitCode::from(1);
            }
        };

        // Resolve PollClock from env (Strict mode has NO clock-flag CLI
        // overrides — all such flags are excluded per FR-053; only env
        // vars + `-f` apply).
        let env = rusty_autossh::clock::EnvSnapshot::from_process_env();
        let clock = rusty_autossh::clock::PollClock::resolve_from_env_and_flags(
            &env,
            &rusty_autossh::clock::ClockFlags::default(),
            parsed.background, // T075: -f forces gate_time=0 in strict too
        );

        let mut builder = rusty_autossh::SshSupervisorBuilder::new()
            .ssh_args(parsed.ssh_args)
            .monitor_mode(monitor_mode)
            .poll(clock.poll)
            .first_poll(clock.first_poll)
            .gate_time(clock.gate_time)
            .max_start(clock.max_start)
            .max_lifetime(clock.max_lifetime)
            .one_shot(parsed.one_shot)
            .compatibility_mode(CompatibilityMode::Strict);

        // AUTOSSH_MESSAGE is honored in both modes (FR-013).
        if let Some(msg) = env.vars.get("AUTOSSH_MESSAGE").and_then(|v| v.to_str()) {
            builder = builder.message(msg.to_string());
        }

        // T106 + T107: pidfile + logfile path resolution. Strict mode has
        // no --pid-file / --log-file flags (excluded per FR-053), so only
        // env vars apply.
        if let Some(p) = env
            .vars
            .get("AUTOSSH_PIDFILE")
            .map(std::path::PathBuf::from)
        {
            builder = builder.pidfile_path(p);
        }
        if let Some(p) = env
            .vars
            .get("AUTOSSH_LOGFILE")
            .map(std::path::PathBuf::from)
        {
            builder = builder.logfile_path(p);
        }

        // T104 + T105: daemonize when `-f` is supplied. Strict mode
        // shares the cross-platform dispatch with Default mode.
        if parsed.background {
            if let Err(e) = perform_daemonize_default(None, &env) {
                return map_strict_error(e);
            }
        }

        match builder.build() {
            Ok(mut s) => match futures_blocking_run(&mut s) {
                Ok(()) => ExitCode::from(0),
                Err(e) => map_strict_error(e),
            },
            Err(e) => map_strict_error(e),
        }
    }

    /// Map an [`AutosshError`] to a Strict-mode exit code.
    ///
    /// Per FR-054 + FR-051: stderr lines under Strict mode use the
    /// `autossh:` prefix and have NO ISO timestamp.
    fn map_strict_error(e: AutosshError) -> ExitCode {
        eprintln!("autossh: {e}");
        match e {
            AutosshError::SshNotFound { .. } => ExitCode::from(1),
            AutosshError::MaxStartReached { .. } => ExitCode::from(1),
            AutosshError::MonitorBindFailed { .. } => ExitCode::from(2),
            AutosshError::PidfileWrite { .. } => ExitCode::from(2),
            AutosshError::LogfileWrite { .. } => ExitCode::from(2),
            AutosshError::Daemonize { .. } => ExitCode::from(2),
            AutosshError::Io(_) => ExitCode::from(2),
            _ => ExitCode::from(2),
        }
    }

    fn emit_completions(shell: rusty_autossh::cli::Shell) -> ExitCode {
        use clap::CommandFactory;
        use clap_complete::{Shell as CcShell, generate};

        let cc_shell = match shell {
            rusty_autossh::cli::Shell::Bash => CcShell::Bash,
            rusty_autossh::cli::Shell::Zsh => CcShell::Zsh,
            rusty_autossh::cli::Shell::Fish => CcShell::Fish,
            rusty_autossh::cli::Shell::Powershell => CcShell::PowerShell,
        };

        let mut cmd = Cli::command();
        let bin_name = cmd.get_name().to_string();
        generate(cc_shell, &mut cmd, bin_name, &mut std::io::stdout());
        ExitCode::from(0)
    }

    fn map_error(e: AutosshError) -> ExitCode {
        eprintln!("rusty-autossh: {e}");
        match e {
            AutosshError::SshNotFound { .. } => ExitCode::from(1),
            AutosshError::MaxStartReached { .. } => ExitCode::from(1),
            AutosshError::MonitorBindFailed { .. } => ExitCode::from(2),
            AutosshError::PidfileWrite { .. } => ExitCode::from(2),
            AutosshError::LogfileWrite { .. } => ExitCode::from(2),
            AutosshError::Daemonize { .. } => ExitCode::from(2),
            AutosshError::Io(_) => ExitCode::from(2),
            _ => ExitCode::from(2),
        }
    }

    /// Block on `supervisor.run()` by constructing a per-call tokio
    /// runtime. Phase 2 stub: the supervisor returns Ok(()) immediately
    /// at this point in the lifecycle, so the runtime construction
    /// overhead is bounded.
    fn futures_blocking_run(
        supervisor: &mut rusty_autossh::SshSupervisor,
    ) -> Result<(), AutosshError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(AutosshError::Io)?;
        rt.block_on(async { supervisor.run().await })
    }
}
