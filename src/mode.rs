//! Strict-mode activation precedence ladder.
//!
//! Per AD-006 + FR-050 + Clarifications Q8 strict mode is activated by ANY
//! of (precedence high → low): `--strict` > `--no-strict` > env
//! `RUSTY_AUTOSSH_STRICT=1` > `argv[0]` basename == `autossh` > default.
//!
//! Last-wins applies ONLY between `--strict` and `--no-strict` on the
//! command line per Clarifications Q8. Env and `argv[0]` are activation
//! sources, not toggles.

use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::CompatibilityMode;
use crate::clock::EnvSnapshot;

/// Resolve the [`CompatibilityMode`] from argv tokens, env vars, and
/// `argv[0]`.
///
/// Algorithm:
/// 1. Scan `args` for `--strict` / `--no-strict`; last occurrence wins
///    between the two. If either appears it OVERRIDES env + `argv[0]`.
/// 2. Else: if `RUSTY_AUTOSSH_STRICT=1` is set in env → Strict.
/// 3. Else: if `argv[0]` basename (with `.exe` stripped on Windows) ==
///    `autossh` → Strict.
/// 4. Else: Default.
pub fn resolve(args: &[OsString], env: &EnvSnapshot, argv0: &OsStr) -> CompatibilityMode {
    // Phase 1: flag last-wins (overrides env + argv[0]).
    let mut flag_decision: Option<CompatibilityMode> = None;
    for arg in args {
        if let Some(s) = arg.to_str() {
            if s == "--strict" {
                flag_decision = Some(CompatibilityMode::Strict);
            } else if s == "--no-strict" {
                flag_decision = Some(CompatibilityMode::Default);
            }
        }
    }
    if let Some(mode) = flag_decision {
        return mode;
    }

    // Phase 2: env-var activation.
    // Per FR-050 / T073: accept `1`, `true`, `yes` (case-insensitive).
    if let Some(v) = env.vars.get("RUSTY_AUTOSSH_STRICT") {
        if let Some(s) = v.to_str() {
            let s_lc = s.trim().to_ascii_lowercase();
            if matches!(s_lc.as_str(), "1" | "true" | "yes") {
                return CompatibilityMode::Strict;
            }
        }
    }

    // Phase 3: argv[0] basename activation.
    let basename = Path::new(argv0).file_stem().and_then(|s| s.to_str());
    if matches!(basename, Some("autossh")) {
        return CompatibilityMode::Strict;
    }

    CompatibilityMode::Default
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env_with(pairs: &[(&str, &str)]) -> EnvSnapshot {
        let mut vars = HashMap::new();
        for (k, v) in pairs {
            vars.insert((*k).to_string(), OsString::from(*v));
        }
        EnvSnapshot { vars }
    }

    fn argv(s: &[&str]) -> Vec<OsString> {
        s.iter().map(|x| OsString::from(*x)).collect()
    }

    #[test]
    fn strict_flag_alone_is_strict() {
        let env = EnvSnapshot::default();
        let mode = resolve(&argv(&["--strict"]), &env, OsStr::new("rusty-autossh"));
        assert_eq!(mode, CompatibilityMode::Strict);
    }

    #[test]
    fn env_var_one_is_strict() {
        let env = env_with(&[("RUSTY_AUTOSSH_STRICT", "1")]);
        let mode = resolve(&[], &env, OsStr::new("rusty-autossh"));
        assert_eq!(mode, CompatibilityMode::Strict);
    }

    #[test]
    fn argv0_autossh_is_strict() {
        let env = EnvSnapshot::default();
        let mode = resolve(&[], &env, OsStr::new("autossh"));
        assert_eq!(mode, CompatibilityMode::Strict);
    }

    #[test]
    fn no_strict_overrides_env() {
        let env = env_with(&[("RUSTY_AUTOSSH_STRICT", "1")]);
        let mode = resolve(&argv(&["--no-strict"]), &env, OsStr::new("rusty-autossh"));
        assert_eq!(mode, CompatibilityMode::Default);
    }

    #[test]
    fn no_strict_overrides_argv0() {
        let env = EnvSnapshot::default();
        let mode = resolve(&argv(&["--no-strict"]), &env, OsStr::new("autossh"));
        assert_eq!(mode, CompatibilityMode::Default);
    }

    #[test]
    fn last_wins_strict_then_no_strict_is_default() {
        let env = EnvSnapshot::default();
        let mode = resolve(
            &argv(&["--strict", "--no-strict"]),
            &env,
            OsStr::new("rusty-autossh"),
        );
        assert_eq!(mode, CompatibilityMode::Default);
    }

    #[test]
    fn last_wins_no_strict_then_strict_is_strict() {
        let env = EnvSnapshot::default();
        let mode = resolve(
            &argv(&["--no-strict", "--strict"]),
            &env,
            OsStr::new("rusty-autossh"),
        );
        assert_eq!(mode, CompatibilityMode::Strict);
    }

    #[test]
    fn argv0_autossh_exe_on_windows_is_strict_after_stem_strip() {
        let env = EnvSnapshot::default();
        let mode = resolve(&[], &env, OsStr::new("autossh.exe"));
        assert_eq!(mode, CompatibilityMode::Strict);
    }

    #[test]
    fn argv0_with_path_prefix_still_strict() {
        let env = EnvSnapshot::default();
        let mode = resolve(&[], &env, OsStr::new("/usr/local/bin/autossh"));
        assert_eq!(mode, CompatibilityMode::Strict);
    }

    #[test]
    fn default_when_nothing_activates() {
        let env = EnvSnapshot::default();
        let mode = resolve(&[], &env, OsStr::new("rusty-autossh"));
        assert_eq!(mode, CompatibilityMode::Default);
    }
}
