//! The folded bar, painted from a single color.
//!
//! The bar's color is the human's choice and can be anything, so everything
//! drawn over it is derived from it: dark or light text, whichever contrasts
//! more, and buttons, outlines and border as shades of the bar itself.

use eframe::egui::{self, Color32, Stroke};

use crate::config::appearance::Rgb;

const DARK_TEXT: Color32 = Color32::from_rgb(24, 24, 27);
const LIGHT_TEXT: Color32 = Color32::from_rgb(250, 250, 250);

pub(crate) struct FoldedBar {
    pub(crate) fill: Color32,
    pub(crate) border: Color32,
    pub(crate) text: Color32,
    button: Color32,
    button_hovered: Color32,
    button_active: Color32,
    button_outline: Color32,
    pub(crate) progress: Color32,
}

impl FoldedBar {
    pub(crate) fn from_rgb([red, green, blue]: Rgb) -> Self {
        let fill = Color32::from_rgb(red, green, blue);
        let text = if contrast(fill, DARK_TEXT) >= contrast(fill, LIGHT_TEXT) {
            DARK_TEXT
        } else {
            LIGHT_TEXT
        };
        // Shades move toward the text: darker on a light bar, lighter on a
        // dark one, so they stay visible either way.
        let shade = |amount: f32| mix(fill, text, amount);
        Self {
            fill,
            border: shade(0.45),
            text,
            button: shade(0.12),
            button_hovered: shade(0.2),
            button_active: shade(0.28),
            button_outline: shade(0.35),
            progress: shade(0.4),
        }
    }

    /// Repaint the widgets of the bar's `Ui` for its color. Only that `Ui` is
    /// touched; the rest of the window keeps the theme.
    pub(crate) fn apply(&self, visuals: &mut egui::Visuals) {
        visuals.override_text_color = Some(self.text);
        // Disabled widgets fade toward the window fill.
        visuals.window_fill = self.fill;
        visuals.panel_fill = self.fill;
        visuals.widgets.noninteractive.fg_stroke.color = self.text;
        for (widget, fill) in [
            (&mut visuals.widgets.inactive, self.button),
            (&mut visuals.widgets.hovered, self.button_hovered),
            (&mut visuals.widgets.active, self.button_active),
        ] {
            widget.bg_fill = fill;
            widget.weak_bg_fill = fill;
            widget.bg_stroke = Stroke::new(1.0_f32, self.button_outline);
            widget.fg_stroke.color = self.text;
        }
    }
}

/// WCAG contrast ratio, from 1 (none) to 21 (black on white).
fn contrast(a: Color32, b: Color32) -> f32 {
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn luminance(color: Color32) -> f32 {
    let channel = |value: u8| {
        let value = f32::from(value) / 255.0;
        if value <= 0.039_28 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
}

fn mix(from: Color32, to: Color32, amount: f32) -> Color32 {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::appearance::NAMED_COLORS;

    #[test]
    fn named_colors_carry_readable_text() {
        for color in NAMED_COLORS {
            let bar = FoldedBar::from_rgb(color.rgb);
            assert!(
                contrast(bar.fill, bar.text) >= 4.5,
                "{} text contrast {}",
                color.name,
                contrast(bar.fill, bar.text)
            );
        }
    }

    #[test]
    fn any_gray_still_carries_legible_text() {
        for value in (0..=255).step_by(5) {
            let bar = FoldedBar::from_rgb([value, value, value]);
            assert!(contrast(bar.fill, bar.text) >= 4.0, "gray {value}");
        }
    }

    #[test]
    fn pastel_bars_get_dark_text_and_dark_bars_light_text() {
        assert_eq!(FoldedBar::from_rgb([217, 167, 180]).text, DARK_TEXT);
        assert_eq!(FoldedBar::from_rgb([38, 38, 42]).text, LIGHT_TEXT);
    }

    #[test]
    fn the_border_stands_apart_from_the_bar() {
        for color in NAMED_COLORS {
            let bar = FoldedBar::from_rgb(color.rgb);
            assert!(contrast(bar.fill, bar.border) >= 1.8, "{}", color.name);
        }
    }
}
