//! `torii config theme` and `torii config folded-bar-color`.

use super::{row, update};
use crate::config::appearance::{hex, FoldedBarColor, Theme, NAMED_COLORS};
use crate::config::{settings, ConfigPaths};
use crate::control::preview;
use crate::error::{Error, Result};

pub(super) fn list_themes(paths: &ConfigPaths) -> Result<()> {
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

pub(super) fn set_theme(paths: &ConfigPaths, value: &str) -> Result<()> {
    save_theme(paths, parse_theme(value)?)
}

pub(super) fn reset_theme(paths: &ConfigPaths) -> Result<()> {
    save_theme(paths, Theme::default())
}

/// The sample window in `value`, or in the saved theme; the folded bar keeps
/// its saved color.
pub(super) fn test_theme(paths: &ConfigPaths, value: Option<&str>) -> Result<()> {
    let settings = settings::load(&paths.settings())?;
    let theme = value
        .map(parse_theme)
        .transpose()?
        .unwrap_or(settings.theme);
    preview::show(theme, settings.folded_bar_color)?;
    eprintln!("Test window in the {theme} theme closed. Nothing ran and nothing was saved.");
    Ok(())
}

pub(super) fn list_folded_bar_colors(paths: &ConfigPaths) -> Result<()> {
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

pub(super) fn set_folded_bar_color(paths: &ConfigPaths, value: &str) -> Result<()> {
    save_folded_bar_color(paths, parse_folded_bar_color(value)?)
}

pub(super) fn reset_folded_bar_color(paths: &ConfigPaths) -> Result<()> {
    save_folded_bar_color(paths, FoldedBarColor::default())
}

/// The sample window in the saved theme, with `value` or the saved color on
/// its folded bar.
pub(super) fn test_folded_bar_color(paths: &ConfigPaths, value: Option<&str>) -> Result<()> {
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

#[cfg(test)]
mod tests {
    use super::super::{saved, test_paths};
    use super::*;

    #[test]
    fn setting_a_theme_creates_the_settings_file() {
        let (_temp, paths) = test_paths();
        set_theme(&paths, "nord").unwrap();
        assert_eq!(saved(&paths).theme, Theme::Nord);
        reset_theme(&paths).unwrap();
        assert_eq!(saved(&paths).theme, Theme::Dark);
    }

    #[test]
    fn a_change_keeps_the_other_settings() {
        let (_temp, paths) = test_paths();
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
        let (_temp, paths) = test_paths();
        list_themes(&paths).unwrap();
        list_folded_bar_colors(&paths).unwrap();
        assert!(!paths.settings().exists());
    }

    #[test]
    fn an_invalid_value_leaves_the_file_untouched() {
        let (_temp, paths) = test_paths();
        assert!(set_theme(&paths, "solarized").is_err());
        assert!(set_folded_bar_color(&paths, "roxo").is_err());
        assert!(test_theme(&paths, Some("solarized")).is_err());
        assert!(test_folded_bar_color(&paths, Some("roxo")).is_err());
        assert!(!paths.settings().exists());
    }
}
