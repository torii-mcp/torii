//! How the prompt windows look: the theme and the color of the folded bar.
//!
//! Both are human preferences kept in `settings.yaml` and changed through
//! `torii config`. This module only names the choices and parses them; the
//! colors each theme paints with live in `control::theme`.

use serde::{Deserialize, Serialize};
use std::fmt;

/// An sRGB color, `[red, green, blue]`.
pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
    Nord,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Dark, Theme::Light, Theme::Nord];

    pub fn name(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
            Theme::Nord => "nord",
        }
    }

    pub fn parse(value: &str) -> Option<Theme> {
        let value = value.trim().to_ascii_lowercase();
        Theme::ALL.into_iter().find(|theme| theme.name() == value)
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A folded bar color offered by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedColor {
    pub name: &'static str,
    pub rgb: Rgb,
}

/// Soft colors that stand out over both light and dark pages, so a folded
/// window can be found on the screen. `grafite` is the dark bar Torii used
/// before the bar had a color of its own.
pub const NAMED_COLORS: [NamedColor; 11] = [
    named("rosa-antigo", [217, 167, 180]),
    named("salvia", [169, 196, 160]),
    named("menta", [159, 211, 193]),
    named("azul-ceu", [158, 193, 217]),
    named("azul-aco", [143, 166, 189]),
    named("lavanda", [186, 180, 226]),
    named("coral", [233, 168, 153]),
    named("pessego", [237, 187, 149]),
    named("manteiga", [230, 211, 140]),
    named("ocre", [211, 180, 110]),
    named("grafite", [38, 38, 42]),
];

const DEFAULT_COLOR: NamedColor = NAMED_COLORS[0];

const fn named(name: &'static str, rgb: Rgb) -> NamedColor {
    NamedColor { name, rgb }
}

/// What the title bar is painted with while its window is folded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum FoldedBarColor {
    /// The theme's own title bar color, the same as when the window is open.
    Theme,
    Named(NamedColor),
    Custom(Rgb),
}

impl Default for FoldedBarColor {
    fn default() -> Self {
        FoldedBarColor::Named(DEFAULT_COLOR)
    }
}

impl FoldedBarColor {
    /// Accepts `theme`, a name from [`NAMED_COLORS`] or `#rrggbb`. Names are
    /// matched ignoring case and accents, so `Sálvia` finds `salvia`.
    pub fn parse(value: &str) -> Option<FoldedBarColor> {
        let value = value.trim();
        if let Some(rgb) = parse_hex(value) {
            return Some(FoldedBarColor::Custom(rgb));
        }
        let key = normalized_name(value);
        if key == "theme" {
            return Some(FoldedBarColor::Theme);
        }
        NAMED_COLORS
            .into_iter()
            .find(|color| color.name == key)
            .map(FoldedBarColor::Named)
    }

    /// The color to paint, or `None` when the theme decides.
    pub fn rgb(self) -> Option<Rgb> {
        match self {
            FoldedBarColor::Theme => None,
            FoldedBarColor::Named(color) => Some(color.rgb),
            FoldedBarColor::Custom(rgb) => Some(rgb),
        }
    }
}

impl fmt::Display for FoldedBarColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FoldedBarColor::Theme => f.write_str("theme"),
            FoldedBarColor::Named(color) => f.write_str(color.name),
            FoldedBarColor::Custom(rgb) => f.write_str(&hex(*rgb)),
        }
    }
}

impl TryFrom<String> for FoldedBarColor {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        FoldedBarColor::parse(&value).ok_or_else(|| {
            format!("invalid folded bar color {value:?}; expected `theme`, a color name or #rrggbb")
        })
    }
}

impl From<FoldedBarColor> for String {
    fn from(color: FoldedBarColor) -> Self {
        color.to_string()
    }
}

pub fn hex([red, green, blue]: Rgb) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// `#rrggbb` or `rrggbb`.
fn parse_hex(value: &str) -> Option<Rgb> {
    let digits = value.strip_prefix('#').unwrap_or(value);
    if digits.len() != 6 || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |index: usize| u8::from_str_radix(&digits[index..index + 2], 16).ok();
    Some([channel(0)?, channel(2)?, channel(4)?])
}

/// Lowercase, without the accents of Portuguese, with spaces and underscores
/// as hyphens: how the names in [`NAMED_COLORS`] are written.
fn normalized_name(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|character| match character {
            'á' | 'à' | 'â' | 'ã' => 'a',
            'é' | 'ê' => 'e',
            'í' => 'i',
            'ó' | 'ô' | 'õ' => 'o',
            'ú' | 'ü' => 'u',
            'ç' => 'c',
            ' ' | '_' => '-',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_folded_bar_is_rosa_antigo() {
        assert_eq!(FoldedBarColor::default().to_string(), "rosa-antigo");
    }

    #[test]
    fn color_names_ignore_case_and_accents() {
        assert_eq!(
            FoldedBarColor::parse("Sálvia"),
            Some(FoldedBarColor::Named(NAMED_COLORS[1]))
        );
        assert_eq!(
            FoldedBarColor::parse("Azul Aço"),
            FoldedBarColor::parse("azul-aco")
        );
        assert_eq!(FoldedBarColor::parse("THEME"), Some(FoldedBarColor::Theme));
        assert_eq!(FoldedBarColor::parse("roxo"), None);
    }

    #[test]
    fn custom_colors_accept_hex_with_or_without_hash() {
        assert_eq!(
            FoldedBarColor::parse("#D9A7B4"),
            Some(FoldedBarColor::Custom([217, 167, 180]))
        );
        assert_eq!(
            FoldedBarColor::parse("d9a7b4"),
            Some(FoldedBarColor::Custom([217, 167, 180]))
        );
        assert_eq!(FoldedBarColor::parse("#d9a7b"), None);
        assert_eq!(FoldedBarColor::parse("#gggggg"), None);
        assert_eq!(FoldedBarColor::Custom([1, 2, 255]).to_string(), "#0102ff");
    }

    #[test]
    fn every_value_survives_a_round_trip_through_its_text() {
        let values = NAMED_COLORS
            .into_iter()
            .map(FoldedBarColor::Named)
            .chain([FoldedBarColor::Theme, FoldedBarColor::Custom([10, 20, 30])]);
        for value in values {
            assert_eq!(FoldedBarColor::parse(&value.to_string()), Some(value));
        }
        for theme in Theme::ALL {
            assert_eq!(Theme::parse(theme.name()), Some(theme));
        }
    }

    #[test]
    fn color_names_are_unique_and_already_normalized() {
        for (index, color) in NAMED_COLORS.iter().enumerate() {
            assert_eq!(normalized_name(color.name), color.name);
            assert!(parse_hex(color.name).is_none());
            assert!(NAMED_COLORS[index + 1..]
                .iter()
                .all(|other| other.name != color.name));
        }
    }
}
