//! `torii config`: show and change the human preferences in `settings.yaml`.
//!
//! Every action is a verb. Each preference answers `list` (the choices, with
//! the default and the one in use marked), `set <value>`, `reset` (back to
//! the default) and `test [<value>]`, which opens a sample authorization
//! window to see a value before or after choosing it. Prompt windows read the
//! settings when they open, so a change applies to the next window without a
//! restart.

use crate::config::appearance::{hex, FoldedBarColor, Theme, NAMED_COLORS};
use crate::config::{settings, ConfigPaths, Settings};
use crate::control::preview;
use crate::error::{Error, Result};

/// What every preference answers.
const VERBS: [&str; 5] = ["list", "ls", "set", "reset", "test"];

/// Route `torii config <args>`. A line without a known verb gets the usage of
/// the nearest command; the old `theme <value>` form is pointed to `set`.
pub fn run(paths: &ConfigPaths, args: &[String]) -> Result<()> {
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["list" | "ls"] => list(paths),
        ["dir", "show"] => {
            show_dir(paths);
            Ok(())
        }
        ["theme", "list" | "ls"] => list_themes(paths),
        ["theme", "set", value] => set_theme(paths, value),
        ["theme", "reset"] => reset_theme(paths),
        ["theme", "test", value @ ..] if value.len() <= 1 => {
            test_theme(paths, value.first().copied())
        }
        ["folded-bar-color" | "fbc", "list" | "ls"] => list_folded_bar_colors(paths),
        ["folded-bar-color" | "fbc", "set", value] => set_folded_bar_color(paths, value),
        ["folded-bar-color" | "fbc", "reset"] => reset_folded_bar_color(paths),
        ["folded-bar-color" | "fbc", "test", value @ ..] if value.len() <= 1 => {
            test_folded_bar_color(paths, value.first().copied())
        }
        [preference @ ("theme" | "folded-bar-color" | "fbc"), value] if !VERBS.contains(value) => {
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

pub fn show_dir(paths: &ConfigPaths) {
    println!("{}", paths.base().display());
}

pub fn list(paths: &ConfigPaths) -> Result<()> {
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
    ];
    for (key, value, default) in rows {
        println!("{key:<18} {value:<14} default: {default}");
    }
    Ok(())
}

pub fn list_themes(paths: &ConfigPaths) -> Result<()> {
    let current = settings::load(&paths.settings())?.theme;
    for theme in Theme::ALL {
        let note = if theme == Theme::default() {
            "default"
        } else {
            ""
        };
        println!("{}", row(theme == current, theme.name(), note, ""));
    }
    println!("\n* in use");
    Ok(())
}

pub fn set_theme(paths: &ConfigPaths, value: &str) -> Result<()> {
    save_theme(paths, parse_theme(value)?)
}

pub fn reset_theme(paths: &ConfigPaths) -> Result<()> {
    save_theme(paths, Theme::default())
}

/// The sample window in `value`, or in the saved theme; the folded bar keeps
/// its saved color.
pub fn test_theme(paths: &ConfigPaths, value: Option<&str>) -> Result<()> {
    let settings = settings::load(&paths.settings())?;
    let theme = value
        .map(parse_theme)
        .transpose()?
        .unwrap_or(settings.theme);
    preview::show(theme, settings.folded_bar_color)?;
    eprintln!("Test window in the {theme} theme closed. Nothing ran and nothing was saved.");
    Ok(())
}

pub fn list_folded_bar_colors(paths: &ConfigPaths) -> Result<()> {
    let current = settings::load(&paths.settings())?.folded_bar_color;
    let default = FoldedBarColor::default();
    println!("{}", row(false, "NAME", "COLOR", ""));
    for color in NAMED_COLORS {
        let value = FoldedBarColor::Named(color);
        let note = match color.name {
            _ if value == default => "default",
            "grafite" => "the dark bar of earlier versions",
            _ => "",
        };
        println!(
            "{}",
            row(value == current, color.name, &hex(color.rgb), note)
        );
    }
    println!(
        "{}",
        row(
            current == FoldedBarColor::Theme,
            "theme",
            "-",
            "the theme's own title bar"
        )
    );
    if let FoldedBarColor::Custom(rgb) = current {
        println!("{}", row(true, "", &hex(rgb), "custom"));
    }
    println!(
        "\n* in use. `set` also takes any #rrggbb; `torii config folded-bar-color reset` restores {default}."
    );
    Ok(())
}

pub fn set_folded_bar_color(paths: &ConfigPaths, value: &str) -> Result<()> {
    save_folded_bar_color(paths, parse_folded_bar_color(value)?)
}

pub fn reset_folded_bar_color(paths: &ConfigPaths) -> Result<()> {
    save_folded_bar_color(paths, FoldedBarColor::default())
}

/// The sample window in the saved theme, with `value` or the saved color on
/// its folded bar.
pub fn test_folded_bar_color(paths: &ConfigPaths, value: Option<&str>) -> Result<()> {
    let settings = settings::load(&paths.settings())?;
    let color = value
        .map(parse_folded_bar_color)
        .transpose()?
        .unwrap_or(settings.folded_bar_color);
    preview::show(settings.theme, color)?;
    eprintln!("Test window with the {color} folded bar closed. Nothing ran and nothing was saved.");
    Ok(())
}

fn parse_theme(value: &str) -> Result<Theme> {
    Theme::parse(value).ok_or_else(|| {
        Error::InvalidSetting(format!(
            "unknown theme {value:?}; run `torii config theme list` for the choices"
        ))
    })
}

fn parse_folded_bar_color(value: &str) -> Result<FoldedBarColor> {
    FoldedBarColor::parse(value).ok_or_else(|| {
        Error::InvalidSetting(format!(
            "unknown folded bar color {value:?}; use a name from `torii config folded-bar-color list`, `theme` or #rrggbb such as #d9a7b4"
        ))
    })
}

fn save_theme(paths: &ConfigPaths, theme: Theme) -> Result<()> {
    update(paths, |settings| settings.theme = theme)?;
    eprintln!("Theme set to {theme}. Prompt windows opened from now on use it.");
    Ok(())
}

fn save_folded_bar_color(paths: &ConfigPaths, color: FoldedBarColor) -> Result<()> {
    update(paths, |settings| settings.folded_bar_color = color)?;
    eprintln!("Folded bar color set to {color}. Prompt windows opened from now on use it.");
    Ok(())
}

/// One line of a choice list: the in-use mark, then aligned columns.
fn row(in_use: bool, name: &str, value: &str, note: &str) -> String {
    let mark = if in_use { '*' } else { ' ' };
    format!("{mark} {name:<12} {value:<9} {note}")
        .trim_end()
        .to_owned()
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
mod tests {
    use super::*;

    fn paths() -> (tempfile::TempDir, ConfigPaths) {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().join("torii"));
        (temp, paths)
    }

    fn saved(paths: &ConfigPaths) -> Settings {
        settings::load(&paths.settings()).unwrap()
    }

    #[test]
    fn setting_a_theme_creates_the_settings_file() {
        let (_temp, paths) = paths();
        set_theme(&paths, "nord").unwrap();
        assert_eq!(saved(&paths).theme, Theme::Nord);
        reset_theme(&paths).unwrap();
        assert_eq!(saved(&paths).theme, Theme::Dark);
    }

    #[test]
    fn a_change_keeps_the_other_settings() {
        let (_temp, paths) = paths();
        paths.ensure().unwrap();
        std::fs::write(paths.settings(), "default_grant_minutes: 7\ntheme: light\n").unwrap();
        set_folded_bar_color(&paths, "#123456").unwrap();
        let settings = saved(&paths);
        assert_eq!(settings.default_grant_minutes, 7);
        assert_eq!(settings.theme, Theme::Light);
        assert_eq!(
            settings.folded_bar_color,
            FoldedBarColor::Custom([0x12, 0x34, 0x56])
        );
        reset_folded_bar_color(&paths).unwrap();
        assert_eq!(saved(&paths).folded_bar_color, FoldedBarColor::default());
    }

    #[test]
    fn listing_saves_nothing() {
        let (_temp, paths) = paths();
        list(&paths).unwrap();
        list_themes(&paths).unwrap();
        list_folded_bar_colors(&paths).unwrap();
        assert!(!paths.settings().exists());
    }

    #[test]
    fn an_invalid_value_leaves_the_file_untouched() {
        let (_temp, paths) = paths();
        assert!(set_theme(&paths, "solarized").is_err());
        assert!(set_folded_bar_color(&paths, "roxo").is_err());
        assert!(test_theme(&paths, Some("solarized")).is_err());
        assert!(test_folded_bar_color(&paths, Some("roxo")).is_err());
        assert!(!paths.settings().exists());
    }
}
