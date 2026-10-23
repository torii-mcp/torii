//! `torii config log-retention`, `log-tail` and `log-output`.

use std::ops::RangeInclusive;

use super::{row, update};
use crate::config::logging::{self, LogOutput};
use crate::config::{settings, ConfigPaths};
use crate::error::{Error, Result};

pub(super) fn days(count: u32) -> String {
    if count == 1 {
        "1 day".into()
    } else {
        format!("{count} days")
    }
}

pub(super) fn lines(count: u32) -> String {
    if count == 1 {
        "1 record".into()
    } else {
        format!("{count} records")
    }
}

pub(super) fn list_retention(paths: &ConfigPaths) -> Result<()> {
    let current = settings::load(&paths.settings())?.log_retention_days;
    list_count(
        current,
        logging::DEFAULT_RETENTION_DAYS,
        &logging::RETENTION_DAYS,
        days,
    );
    println!(
        "The automatic prune runs when an MCP server starts; `torii log prune [days]` runs it now."
    );
    Ok(())
}

pub(super) fn set_retention(paths: &ConfigPaths, value: &str) -> Result<()> {
    let count = parse(value, "log-retention", "days", &logging::RETENTION_DAYS)?;
    update(paths, |settings| settings.log_retention_days = count)?;
    eprintln!(
        "The audit log now keeps {}. Running MCP servers apply it when they restart.",
        days(count)
    );
    Ok(())
}

pub(super) fn reset_retention(paths: &ConfigPaths) -> Result<()> {
    update(paths, |settings| {
        settings.log_retention_days = logging::DEFAULT_RETENTION_DAYS
    })?;
    eprintln!(
        "The audit log now keeps {}, the default.",
        days(logging::DEFAULT_RETENTION_DAYS)
    );
    Ok(())
}

pub(super) fn list_tail(paths: &ConfigPaths) -> Result<()> {
    let current = settings::load(&paths.settings())?.log_tail_lines;
    list_count(
        current,
        logging::DEFAULT_TAIL_LINES,
        &logging::TAIL_LINES,
        lines,
    );
    Ok(())
}

pub(super) fn set_tail(paths: &ConfigPaths, value: &str) -> Result<()> {
    let count = parse(value, "log-tail", "records", &logging::TAIL_LINES)?;
    update(paths, |settings| settings.log_tail_lines = count)?;
    eprintln!("`torii log tail` now shows {} by default.", lines(count));
    Ok(())
}

pub(super) fn reset_tail(paths: &ConfigPaths) -> Result<()> {
    update(paths, |settings| {
        settings.log_tail_lines = logging::DEFAULT_TAIL_LINES
    })?;
    eprintln!(
        "`torii log tail` now shows {} by default.",
        lines(logging::DEFAULT_TAIL_LINES)
    );
    Ok(())
}

pub(super) fn list_output(paths: &ConfigPaths) -> Result<()> {
    let current = settings::load(&paths.settings())?.log_output;
    for choice in LogOutput::ALL {
        let note = match choice {
            LogOutput::Full => "keep stdout and stderr, compressed and sealed",
            LogOutput::None => "keep only sizes and exit code",
        };
        let default = if choice == LogOutput::default() {
            "default"
        } else {
            ""
        };
        println!("{}", row(choice == current, choice.name(), default, note));
    }
    println!("\n* in use");
    Ok(())
}

pub(super) fn set_output(paths: &ConfigPaths, value: &str) -> Result<()> {
    let choice = LogOutput::parse(value).ok_or_else(|| {
        Error::InvalidSetting(format!("unknown log-output {value:?}; it is full or none"))
    })?;
    save_output(paths, choice)
}

pub(super) fn reset_output(paths: &ConfigPaths) -> Result<()> {
    save_output(paths, LogOutput::default())
}

fn save_output(paths: &ConfigPaths, choice: LogOutput) -> Result<()> {
    update(paths, |settings| settings.log_output = choice)?;
    eprintln!("log-output set to {choice}. Running MCP servers apply it when they restart.");
    Ok(())
}

/// A numeric preference: its value, its default and the values it accepts.
fn list_count(current: u32, default: u32, range: &RangeInclusive<u32>, unit: fn(u32) -> String) {
    println!("{}", row(true, &unit(current), "", "in use"));
    if current != default {
        println!("{}", row(false, &unit(default), "", "default"));
    }
    println!("\nAccepts {} to {}.", range.start(), range.end());
}

fn parse(value: &str, name: &str, unit: &str, range: &RangeInclusive<u32>) -> Result<u32> {
    logging::parse_count(value, range).ok_or_else(|| {
        Error::InvalidSetting(format!(
            "{name} takes a whole number of {unit} from {} to {}, not {value:?}",
            range.start(),
            range.end()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::super::{saved, test_paths};
    use super::*;

    #[test]
    fn retention_and_tail_take_whole_numbers_in_range() {
        let (_temp, paths) = test_paths();
        set_retention(&paths, "30").unwrap();
        set_tail(&paths, "50").unwrap();
        let settings = saved(&paths);
        assert_eq!(settings.log_retention_days, 30);
        assert_eq!(settings.log_tail_lines, 50);
        assert!(set_retention(&paths, "0").is_err());
        assert!(set_tail(&paths, "muitos").is_err());
        reset_retention(&paths).unwrap();
        reset_tail(&paths).unwrap();
        let settings = saved(&paths);
        assert_eq!(settings.log_retention_days, logging::DEFAULT_RETENTION_DAYS);
        assert_eq!(settings.log_tail_lines, logging::DEFAULT_TAIL_LINES);
    }

    #[test]
    fn output_is_full_or_none() {
        let (_temp, paths) = test_paths();
        set_output(&paths, "none").unwrap();
        assert_eq!(saved(&paths).log_output, LogOutput::None);
        assert!(set_output(&paths, "half").is_err());
        reset_output(&paths).unwrap();
        assert_eq!(saved(&paths).log_output, LogOutput::Full);
    }
}
