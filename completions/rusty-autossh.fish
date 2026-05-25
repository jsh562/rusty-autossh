# Print an optspec for argparse to handle cmd's options that are independent of any subcommand.
function __fish_rusty_autossh_global_optspecs
	string join \n M/monitor-port= f/background V/version 1/one-shot poll= first-poll= gate-time= max-start= max-lifetime= ssh-path= pid-file= log-file= debug log-level= strict no-strict h/help
end

function __fish_rusty_autossh_needs_command
	# Figure out if the current invocation already has a command.
	set -l cmd (commandline -opc)
	set -e cmd[1]
	argparse -s (__fish_rusty_autossh_global_optspecs) -- $cmd 2>/dev/null
	or return
	if set -q argv[1]
		# Also print the command, so this can be used to figure out what it is.
		echo $argv[1]
		return 1
	end
	return 0
end

function __fish_rusty_autossh_using_subcommand
	set -l cmd (__fish_rusty_autossh_needs_command)
	test -z "$cmd"
	and return 1
	contains -- $cmd[1] $argv
end

complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -s M -l monitor-port -d '`-M <PORT[:ECHO]>` / `--monitor-port`: monitor port (0 disables)' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l poll -d '`--poll <SECS>`: heartbeat interval (default 600s)' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l first-poll -d '`--first-poll <SECS>`: initial poll delay' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l gate-time -d '`--gate-time <SECS>`: min lifetime before retry counts as failure' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l max-start -d '`--max-start <N>`: consecutive-retry cap (-1 = unlimited)' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l max-lifetime -d '`--max-lifetime <SECS>`: total-runtime cap (0 = unlimited)' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l ssh-path -d '`--ssh-path <PATH>`: override ssh binary (else `AUTOSSH_PATH` / PATH)' -r -F
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l pid-file -d '`--pid-file <PATH>`: override `AUTOSSH_PIDFILE`' -r -F
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l log-file -d '`--log-file <PATH>`: override `AUTOSSH_LOGFILE`' -r -F
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l log-level -d '`--log-level <LEVEL>`: explicit log level (trace/debug/info/warn/error)' -r
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -s f -l background -d '`-f` / `--background`: daemonize to background'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -s V -l version -d '`-V` / `--version`: print version + exit'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -s 1 -l one-shot -d '`-1` / `--one-shot`: exit non-zero on first connection failure'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l debug -d '`--debug`: enable debug logging'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l strict -d '`--strict`: force Strict (upstream-compat) mode'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -l no-strict -d '`--no-strict`: force Default mode'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -s h -l help -d 'Print help (see more with \'--help\')'
complete -c rusty-autossh -n "__fish_rusty_autossh_needs_command" -a "completions" -d 'Emit a shell completion script to stdout'
complete -c rusty-autossh -n "__fish_rusty_autossh_using_subcommand completions" -s h -l help -d 'Print help (see more with \'--help\')'
