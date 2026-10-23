//! `torii config`: show and change the human preferences in `settings.yaml`.
//!
//! Every action is a verb. Each preference answers `list` (the choices, with
//! the default and the one in use marked), `set <value>` and `reset` (back to
//! the default); the appearance ones also answer `test [<value>]`, which opens
//! a sample authorization window. Prompt windows and the audit log read the
//! settings when they need them, so a change needs no restart, except for the
//! retention, which a running server applies at its next start.
//!
//! - `appearance`: the theme and the folded bar color;
//! - `logging`: how long the audit log keeps records, how much `torii log tail`
//!   shows, and whether a call's output is kept.

mod appearance;
mod logging;

use crate::config::{settings, ConfigPaths, Settings};
use crate::console::Table;
use crate::error::{Error, Result};

/// What a preference answers.
const VERBS: [&str; 5] = ["list", "ls", "set", "reset", "test"];

/// The preferences, by the name `torii config` knows them.
const PREFERENCES: [&str; 6] = [
    "theme",
    "folded-bar-color",
    "fbc",
    "log-retention",
    "log-tail",
    "log-output",
];

/// Route `torii config <args>`. A line without a known verb gets the usage of
/// the nearest command; the old `theme <value>` form is pointed to `set`.
pub fn run(paths: &ConfigPaths, args: &[String]) -> Result<()> {
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["list" | "ls"] => list(paths),
        ["dir", "show"] => {
            println!("{}", paths.base().display());
            Ok(())
        }
        ["theme", "list" | "ls"] => appearance::list_themes(paths),
        ["theme", "set", value] => appearance::set_theme(paths, value),
        ["theme", "reset"] => appearance::reset_theme(paths),
        ["theme", "test", value @ ..] if value.len() <= 1 => {
            appearance::test_theme(paths, value.first().copied())
        }
        ["folded-bar-color" | "fbc", "list" | "ls"] => appearance::list_folded_bar_colors(paths),
        ["folded-bar-color" | "fbc", "set", value] => {
            appearance::set_folded_bar_color(paths, value)
        }
        ["folded-bar-color" | "fbc", "reset"] => appearance::reset_folded_bar_color(paths),
        ["folded-bar-color" | "fbc", "test", value @ ..] if value.len() <= 1 => {
            appearance::test_folded_bar_color(paths, value.first().copied())
        }
        ["log-retention", "list" | "ls"] => logging::list_retention(paths),
        ["log-retention", "set", value] => logging::set_retention(paths, value),
        ["log-retention", "reset"] => logging::reset_retention(paths),
        ["log-tail", "list" | "ls"] => logging::list_tail(paths),
        ["log-tail", "set", value] => logging::set_tail(paths, value),
        ["log-tail", "reset"] => logging::reset_tail(paths),
        ["log-output", "list" | "ls"] => logging::list_output(paths),
        ["log-output", "set", value] => logging::set_output(paths, value),
        ["log-output", "reset"] => logging::reset_output(paths),
        [preference, value] if PREFERENCES.contains(preference) && !VERBS.contains(value) => {
            Err(Error::Usage(format!(
                "`{value}` is not a verb; to choose it, run `torii config {preference} set {value}`"
            )))
        }
        _ => Err(Error::Usage(format!(
            "unrecognized command line\n\n{}",
            crate::help::nearest(&[&["config"], &words[..]].concat())
        ))),
    }
}

/// Every preference, its value and its default.
fn list(paths: &ConfigPaths) -> Result<()> {
    let settings = settings::load(&paths.settings())?;
    let defaults = Settings::default();
    let rows = [
        (
            "theme",
            settings.theme.to_string(),
            defaults.theme.to_string(),
        ),
        (
            "folded-bar-color",
            settings.folded_bar_color.to_string(),
            defaults.folded_bar_color.to_string(),
        ),
        (
            "log-retention",
            logging::days(settings.log_retention_days),
            logging::days(defaults.log_retention_days),
        ),
        (
            "log-tail",
            logging::lines(settings.log_tail_lines),
            logging::lines(defaults.log_tail_lines),
        ),
        (
            "log-output",
            settings.log_output.to_string(),
            defaults.log_output.to_string(),
        ),
    ];
    let mut table = Table::new(&["PREFERENCE", "VALUE", "DEFAULT"]);
    for (key, value, default) in rows {
        table.row([key.to_string(), value, default]);
    }
    table.print();
    Ok(())
}

/// The first cell of a choice list: `*` on the one in use.
fn mark(in_use: bool) -> &'static str {
    if in_use {
        "*"
    } else {
        ""
    }
}

/// Load, change and save the settings, creating the file when it is missing.
fn update(paths: &ConfigPaths, change: impl FnOnce(&mut Settings)) -> Result<()> {
    paths.ensure()?;
    let path = paths.settings();
    let mut settings = settings::load(&path)?;
    change(&mut settings);
    settings::save(&path, &settings)
}

#[cfg(test)]
fn test_paths() -> (tempfile::TempDir, ConfigPaths) {
    let temp = tempfile::TempDir::new().unwrap();
    let paths = ConfigPaths::new(temp.path().join("torii"));
    (temp, paths)
}

#[cfg(test)]
fn saved(paths: &ConfigPaths) -> Settings {
    settings::load(&paths.settings()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_every_preference_saves_nothing() {
        let (_temp, paths) = test_paths();
        list(&paths).unwrap();
        assert!(!paths.settings().exists());
    }

    #[test]
    fn a_value_where_a_verb_belongs_points_to_set() {
        let (_temp, paths) = test_paths();
        for preference in PREFERENCES {
            let error = run(&paths, &[preference.into(), "nord".into()]).unwrap_err();
            assert!(
                error.to_string().contains("set nord"),
                "{preference}: {error}"
            );
        }
    }
}
