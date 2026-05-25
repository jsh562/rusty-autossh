
using namespace System.Management.Automation
using namespace System.Management.Automation.Language

Register-ArgumentCompleter -Native -CommandName 'rusty-autossh' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $commandElements = $commandAst.CommandElements
    $command = @(
        'rusty-autossh'
        for ($i = 1; $i -lt $commandElements.Count; $i++) {
            $element = $commandElements[$i]
            if ($element -isnot [StringConstantExpressionAst] -or
                $element.StringConstantType -ne [StringConstantType]::BareWord -or
                $element.Value.StartsWith('-') -or
                $element.Value -eq $wordToComplete) {
                break
        }
        $element.Value
    }) -join ';'

    $completions = @(switch ($command) {
        'rusty-autossh' {
            [CompletionResult]::new('-M', '-M ', [CompletionResultType]::ParameterName, '`-M <PORT[:ECHO]>` / `--monitor-port`: monitor port (0 disables)')
            [CompletionResult]::new('--monitor-port', '--monitor-port', [CompletionResultType]::ParameterName, '`-M <PORT[:ECHO]>` / `--monitor-port`: monitor port (0 disables)')
            [CompletionResult]::new('--poll', '--poll', [CompletionResultType]::ParameterName, '`--poll <SECS>`: heartbeat interval (default 600s)')
            [CompletionResult]::new('--first-poll', '--first-poll', [CompletionResultType]::ParameterName, '`--first-poll <SECS>`: initial poll delay')
            [CompletionResult]::new('--gate-time', '--gate-time', [CompletionResultType]::ParameterName, '`--gate-time <SECS>`: min lifetime before retry counts as failure')
            [CompletionResult]::new('--max-start', '--max-start', [CompletionResultType]::ParameterName, '`--max-start <N>`: consecutive-retry cap (-1 = unlimited)')
            [CompletionResult]::new('--max-lifetime', '--max-lifetime', [CompletionResultType]::ParameterName, '`--max-lifetime <SECS>`: total-runtime cap (0 = unlimited)')
            [CompletionResult]::new('--ssh-path', '--ssh-path', [CompletionResultType]::ParameterName, '`--ssh-path <PATH>`: override ssh binary (else `AUTOSSH_PATH` / PATH)')
            [CompletionResult]::new('--pid-file', '--pid-file', [CompletionResultType]::ParameterName, '`--pid-file <PATH>`: override `AUTOSSH_PIDFILE`')
            [CompletionResult]::new('--log-file', '--log-file', [CompletionResultType]::ParameterName, '`--log-file <PATH>`: override `AUTOSSH_LOGFILE`')
            [CompletionResult]::new('--log-level', '--log-level', [CompletionResultType]::ParameterName, '`--log-level <LEVEL>`: explicit log level (trace/debug/info/warn/error)')
            [CompletionResult]::new('-f', '-f', [CompletionResultType]::ParameterName, '`-f` / `--background`: daemonize to background')
            [CompletionResult]::new('--background', '--background', [CompletionResultType]::ParameterName, '`-f` / `--background`: daemonize to background')
            [CompletionResult]::new('-V', '-V ', [CompletionResultType]::ParameterName, '`-V` / `--version`: print version + exit')
            [CompletionResult]::new('--version', '--version', [CompletionResultType]::ParameterName, '`-V` / `--version`: print version + exit')
            [CompletionResult]::new('-1', '-1', [CompletionResultType]::ParameterName, '`-1` / `--one-shot`: exit non-zero on first connection failure')
            [CompletionResult]::new('--one-shot', '--one-shot', [CompletionResultType]::ParameterName, '`-1` / `--one-shot`: exit non-zero on first connection failure')
            [CompletionResult]::new('--debug', '--debug', [CompletionResultType]::ParameterName, '`--debug`: enable debug logging')
            [CompletionResult]::new('--strict', '--strict', [CompletionResultType]::ParameterName, '`--strict`: force Strict (upstream-compat) mode')
            [CompletionResult]::new('--no-strict', '--no-strict', [CompletionResultType]::ParameterName, '`--no-strict`: force Default mode')
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('completions', 'completions', [CompletionResultType]::ParameterValue, 'Emit a shell completion script to stdout')
            break
        }
        'rusty-autossh;completions' {
            [CompletionResult]::new('-h', '-h', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            [CompletionResult]::new('--help', '--help', [CompletionResultType]::ParameterName, 'Print help (see more with ''--help'')')
            break
        }
    })

    $completions.Where{ $_.CompletionText -like "$wordToComplete*" } |
        Sort-Object -Property ListItemText
}
