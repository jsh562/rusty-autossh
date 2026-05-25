_rusty-autossh() {
    local i cur prev opts cmd
    COMPREPLY=()
    if [[ "${BASH_VERSINFO[0]}" -ge 4 ]]; then
        cur="$2"
    else
        cur="${COMP_WORDS[COMP_CWORD]}"
    fi
    prev="$3"
    cmd=""
    opts=""

    for i in "${COMP_WORDS[@]:0:COMP_CWORD}"
    do
        case "${cmd},${i}" in
            ",$1")
                cmd="rusty__autossh"
                ;;
            rusty__autossh,completions)
                cmd="rusty__autossh__subcmd__completions"
                ;;
            *)
                ;;
        esac
    done

    case "${cmd}" in
        rusty__autossh)
            opts="-M -f -V -1 -h --monitor-port --background --version --one-shot --poll --first-poll --gate-time --max-start --max-lifetime --ssh-path --pid-file --log-file --debug --log-level --strict --no-strict --help [SSH_ARGS]... completions"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 1 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                --monitor-port)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                -M)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --poll)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --first-poll)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --gate-time)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-start)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --max-lifetime)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --ssh-path)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --pid-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --log-file)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                --log-level)
                    COMPREPLY=($(compgen -f "${cur}"))
                    return 0
                    ;;
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
        rusty__subcmd__autossh__subcmd__completions)
            opts="-h --help bash zsh fish powershell"
            if [[ ${cur} == -* || ${COMP_CWORD} -eq 2 ]] ; then
                COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
                return 0
            fi
            case "${prev}" in
                *)
                    COMPREPLY=()
                    ;;
            esac
            COMPREPLY=( $(compgen -W "${opts}" -- "${cur}") )
            return 0
            ;;
    esac
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -F _rusty-autossh -o nosort -o bashdefault -o default rusty-autossh
else
    complete -F _rusty-autossh -o bashdefault -o default rusty-autossh
fi
