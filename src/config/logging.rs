//! Preferences of the audit log: how long records stay, how many `torii log
//! tail` shows, and whether a call's output is kept.
//!
//! Like the appearance, they live in `settings.yaml` and change through
//! `torii config`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::RangeInclusive;

pub const DEFAULT_RETENTION_DAYS: u32 = 90;
/// Up to ten years.
pub const RETENTION_DAYS: RangeInclusive<u32> = 1..=3650;

pub const DEFAULT_TAIL_LINES: u32 = 20;
pub const TAIL_LINES: RangeInclusive<u32> = 1..=10_000;

/// Whether the audit record of a call keeps what the call returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogOutput {
    /// stdout and stderr, as returned to the agent, compressed and sealed.
    #[default]
    Full,
    /// Sizes and exit code only.
    None,
}

impl LogOutput {
    pub const ALL: [LogOutput; 2] = [LogOutput::Full, LogOutput::None];

    pub fn name(self) -> &'static str {
        match self {
            LogOutput::Full => "full",
            LogOutput::None => "none",
        }
    }

    pub fn parse(value: &str) -> Option<LogOutput> {
        let value = value.trim().to_ascii_lowercase();
        LogOutput::ALL
            .into_iter()
            .find(|choice| choice.name() == value)
    }

    pub fn keeps_output(self) -> bool {
        self == LogOutput::Full
    }
}

impl fmt::Display for LogOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A whole number within `range`, for the numeric preferences.
pub fn parse_count(value: &str, range: &RangeInclusive<u32>) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|count| range.contains(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_kept_by_default() {
        assert!(LogOutput::default().keeps_output());
        assert!(!LogOutput::None.keeps_output());
        for choice in LogOutput::ALL {
            assert_eq!(LogOutput::parse(choice.name()), Some(choice));
        }
        assert_eq!(LogOutput::parse("FULL"), Some(LogOutput::Full));
        assert_eq!(LogOutput::parse("some"), None);
    }

    #[test]
    fn counts_stay_within_their_range() {
        assert_eq!(parse_count("30", &RETENTION_DAYS), Some(30));
        assert_eq!(parse_count("0", &RETENTION_DAYS), None);
        assert_eq!(parse_count("3651", &RETENTION_DAYS), None);
        assert_eq!(parse_count("-1", &TAIL_LINES), None);
        assert_eq!(parse_count("dez", &TAIL_LINES), None);
    }
}
