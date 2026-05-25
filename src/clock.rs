//! Poll clock configuration.
//!
//! Per spec §Key Entities + HINT-013 + HINT-015 this module owns
//! [`PollClock`] — the bundle of `AUTOSSH_POLL` / `AUTOSSH_FIRST_POLL` /
//! `AUTOSSH_GATETIME` / `AUTOSSH_MAXSTART` / `AUTOSSH_MAXLIFETIME` values
//! resolved from env vars then overridden by CLI flags.

use std::collections::HashMap;
use std::ffi::OsString;
use std::time::Duration;

/// Default poll interval (seconds) when `AUTOSSH_POLL` is unset.
pub const DEFAULT_POLL_SECS: u64 = 600;

/// Default gate-time (seconds) when `AUTOSSH_GATETIME` is unset.
pub const DEFAULT_GATE_TIME_SECS: u64 = 30;

/// Resolved poll-clock configuration.
///
/// Field semantics:
/// - `poll` — heartbeat probe interval (default 600 s).
/// - `first_poll` — initial poll delay (default = `poll`).
/// - `gate_time` — minimum lifetime before retry counts as failure
///   (default 30 s). `-f` forces 0 unconditionally per FR-022.
/// - `max_start` — `None` = unlimited (the `-1` sentinel); `Some(n)` =
///   cap consecutive retries at `n`.
/// - `max_lifetime` — `None` = unlimited (0 sentinel); `Some(d)` =
///   supervisor self-terminates after `d` total runtime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PollClock {
    /// Heartbeat probe interval.
    pub poll: Duration,
    /// Initial poll delay before the first probe.
    pub first_poll: Duration,
    /// Minimum lifetime before a child exit counts toward
    /// `AUTOSSH_MAXSTART`.
    pub gate_time: Duration,
    /// Consecutive-retry cap; `None` = unlimited.
    pub max_start: Option<u32>,
    /// Total-runtime cap; `None` = unlimited.
    pub max_lifetime: Option<Duration>,
}

impl Default for PollClock {
    fn default() -> Self {
        let poll = Duration::from_secs(DEFAULT_POLL_SECS);
        Self {
            poll,
            first_poll: poll,
            gate_time: Duration::from_secs(DEFAULT_GATE_TIME_SECS),
            max_start: None,
            max_lifetime: None,
        }
    }
}

/// Snapshot of relevant environment variables, suitable for unit testing.
///
/// In production callers populate this from `std::env::vars_os`; tests
/// populate it from an in-memory map per the `tests/common::env_guard`
/// pattern (T037).
#[derive(Debug, Clone, Default)]
pub struct EnvSnapshot {
    /// `AUTOSSH_*` env vars keyed by name.
    pub vars: HashMap<String, OsString>,
}

impl EnvSnapshot {
    /// Capture the host process environment into a new snapshot.
    pub fn from_process_env() -> Self {
        let mut vars = HashMap::new();
        for (k, v) in std::env::vars_os() {
            if let Some(key) = k.to_str() {
                vars.insert(key.to_string(), v);
            }
        }
        Self { vars }
    }

    fn get_str(&self, key: &str) -> Option<&str> {
        self.vars.get(key).and_then(|v| v.to_str())
    }
}

/// CLI-flag overrides for the poll clock (post-clap parsing).
#[derive(Debug, Clone, Default)]
pub struct ClockFlags {
    /// `--poll <secs>`.
    pub poll: Option<Duration>,
    /// `--first-poll <secs>`.
    pub first_poll: Option<Duration>,
    /// `--gate-time <secs>`.
    pub gate_time: Option<Duration>,
    /// `--max-start <n>` — `Some(None)` encodes `-1` sentinel.
    pub max_start: Option<Option<u32>>,
    /// `--max-lifetime <secs>` — `Some(None)` encodes `0` sentinel.
    pub max_lifetime: Option<Option<Duration>>,
}

impl PollClock {
    /// Resolve a [`PollClock`] from environment variables.
    ///
    /// Defaults per spec: poll=600s, first_poll=poll, gate_time=30s,
    /// max_start=None (`-1` sentinel = unlimited), max_lifetime=None
    /// (`0` sentinel = unlimited).
    ///
    /// Numeric parse errors silently fall back to the default for the
    /// field (matches upstream `autossh` lenient behavior).
    pub fn resolve_from_env(env: &EnvSnapshot) -> Self {
        let mut clock = Self::default();

        if let Some(s) = env.get_str("AUTOSSH_POLL") {
            if let Ok(n) = s.parse::<u64>() {
                clock.poll = Duration::from_secs(n);
                clock.first_poll = clock.poll;
            }
        }

        if let Some(s) = env.get_str("AUTOSSH_FIRST_POLL") {
            if let Ok(n) = s.parse::<u64>() {
                clock.first_poll = Duration::from_secs(n);
            }
        }

        if let Some(s) = env.get_str("AUTOSSH_GATETIME") {
            if let Ok(n) = s.parse::<u64>() {
                clock.gate_time = Duration::from_secs(n);
            }
        }

        if let Some(s) = env.get_str("AUTOSSH_MAXSTART") {
            if let Ok(n) = s.parse::<i64>() {
                // -1 sentinel = unlimited per FR-008.
                clock.max_start = if n < 0 { None } else { Some(n as u32) };
            }
        }

        if let Some(s) = env.get_str("AUTOSSH_MAXLIFETIME") {
            if let Ok(n) = s.parse::<u64>() {
                // 0 sentinel = unlimited per FR-009.
                clock.max_lifetime = if n == 0 {
                    None
                } else {
                    Some(Duration::from_secs(n))
                };
            }
        }

        clock
    }

    /// Resolve a [`PollClock`] from env vars then apply CLI flag overrides.
    ///
    /// When `dash_f_supplied = true` the `gate_time` is UNCONDITIONALLY
    /// set to `Duration::ZERO` regardless of any env or flag value (per
    /// FR-022 + Clarifications Q6).
    pub fn resolve_from_env_and_flags(
        env: &EnvSnapshot,
        flags: &ClockFlags,
        dash_f_supplied: bool,
    ) -> Self {
        let mut clock = Self::resolve_from_env(env);

        if let Some(d) = flags.poll {
            clock.poll = d;
        }
        if let Some(d) = flags.first_poll {
            clock.first_poll = d;
        }
        if let Some(d) = flags.gate_time {
            clock.gate_time = d;
        }
        if let Some(max_start) = flags.max_start {
            clock.max_start = max_start;
        }
        if let Some(max_lifetime) = flags.max_lifetime {
            clock.max_lifetime = max_lifetime;
        }

        if dash_f_supplied {
            // FR-022 unconditional override.
            clock.gate_time = Duration::ZERO;
        }

        clock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(pairs: &[(&str, &str)]) -> EnvSnapshot {
        let mut vars = HashMap::new();
        for (k, v) in pairs {
            vars.insert((*k).to_string(), OsString::from(*v));
        }
        EnvSnapshot { vars }
    }

    #[test]
    fn defaults_match_spec() {
        let env = EnvSnapshot::default();
        let clock = PollClock::resolve_from_env(&env);
        assert_eq!(clock.poll, Duration::from_secs(600));
        assert_eq!(clock.first_poll, Duration::from_secs(600));
        assert_eq!(clock.gate_time, Duration::from_secs(30));
        assert_eq!(clock.max_start, None);
        assert_eq!(clock.max_lifetime, None);
    }

    #[test]
    fn env_only_poll_and_gate_time() {
        let env = env_with(&[("AUTOSSH_POLL", "120"), ("AUTOSSH_GATETIME", "15")]);
        let clock = PollClock::resolve_from_env(&env);
        assert_eq!(clock.poll, Duration::from_secs(120));
        // first_poll mirrors poll when not separately set.
        assert_eq!(clock.first_poll, Duration::from_secs(120));
        assert_eq!(clock.gate_time, Duration::from_secs(15));
    }

    #[test]
    fn flag_wins_over_env_for_poll() {
        let env = env_with(&[("AUTOSSH_POLL", "60")]);
        let flags = ClockFlags {
            poll: Some(Duration::from_secs(30)),
            ..ClockFlags::default()
        };
        let clock = PollClock::resolve_from_env_and_flags(&env, &flags, false);
        assert_eq!(clock.poll, Duration::from_secs(30));
    }

    #[test]
    fn max_start_negative_one_is_unlimited_sentinel() {
        let env = env_with(&[("AUTOSSH_MAXSTART", "-1")]);
        let clock = PollClock::resolve_from_env(&env);
        assert_eq!(clock.max_start, None);
    }

    #[test]
    fn max_start_positive_caps_retries() {
        let env = env_with(&[("AUTOSSH_MAXSTART", "3")]);
        let clock = PollClock::resolve_from_env(&env);
        assert_eq!(clock.max_start, Some(3));
    }

    #[test]
    fn max_lifetime_zero_is_unlimited_sentinel() {
        let env = env_with(&[("AUTOSSH_MAXLIFETIME", "0")]);
        let clock = PollClock::resolve_from_env(&env);
        assert_eq!(clock.max_lifetime, None);
    }

    #[test]
    fn max_lifetime_flag_zero_encoded_as_none() {
        let env = EnvSnapshot::default();
        let flags = ClockFlags {
            max_lifetime: Some(None),
            ..ClockFlags::default()
        };
        let clock = PollClock::resolve_from_env_and_flags(&env, &flags, false);
        assert_eq!(clock.max_lifetime, None);
    }

    #[test]
    fn dash_f_forces_gate_time_zero_overrides_env() {
        let env = env_with(&[("AUTOSSH_GATETIME", "99")]);
        let clock = PollClock::resolve_from_env_and_flags(&env, &ClockFlags::default(), true);
        assert_eq!(clock.gate_time, Duration::ZERO);
    }

    #[test]
    fn dash_f_forces_gate_time_zero_overrides_flag() {
        let env = EnvSnapshot::default();
        let flags = ClockFlags {
            gate_time: Some(Duration::from_secs(99)),
            ..ClockFlags::default()
        };
        let clock = PollClock::resolve_from_env_and_flags(&env, &flags, true);
        assert_eq!(clock.gate_time, Duration::ZERO);
    }
}
