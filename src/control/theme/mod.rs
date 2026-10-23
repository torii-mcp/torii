//! The colors the prompt windows paint with.
//!
//! A prompt process opens a single window, so the appearance is chosen once,
//! when the process starts, from the human's `settings.yaml`: [`install`]
//! fixes it, [`palette`] and [`folded_bar`] read it while drawing, and
//! [`apply`] hands the theme's widget colors to egui. Each theme lives in its
//! own file; the folded bar is derived from a single color in `folded`.

mod dark;
mod folded;
mod light;
mod nord;

use std::sync::OnceLock;

use eframe::egui;

pub(crate) use folded::FoldedBar;

use crate::config::appearance::{FoldedBarColor, Theme};

/// What each theme names, beyond the widget colors egui already themes.
pub(crate) struct Palette {
    pub(crate) visuals: egui::Visuals,
    pub(crate) title_bar_bg: egui::Color32,
    pub(crate) window_border: egui::Color32,
    /// The tokens the rule fixes, in the argument pills.
    pub(crate) fixed_group_bg: egui::Color32,
    pub(crate) fixed_pill_bg: egui::Color32,
    pub(crate) fixed_stroke: egui::Color32,
    pub(crate) fixed_text: egui::Color32,
    /// The tokens the rule leaves free.
    pub(crate) free_group_bg: egui::Color32,
    pub(crate) free_pill_bg: egui::Color32,
    pub(crate) free_stroke: egui::Color32,
    pub(crate) free_text: egui::Color32,
    /// Where an approval reaches: the scope line and the prefix boundary.
    pub(crate) boundary_accent: egui::Color32,
    pub(crate) scope_summary_bg: egui::Color32,
    pub(crate) permanent_accept_stroke: egui::Color32,
    pub(crate) permanent_deny_stroke: egui::Color32,
    pub(crate) permanent_deny_progress_bg: egui::Color32,
    pub(crate) target_warning_bg: egui::Color32,
    pub(crate) target_warning_stroke: egui::Color32,
    pub(crate) hold_progress_bg: egui::Color32,
    pub(crate) agent_description_bg: egui::Color32,
    pub(crate) agent_description_stroke: egui::Color32,
    pub(crate) error: egui::Color32,
    pub(crate) success: egui::Color32,
}

struct Appearance {
    palette: Palette,
    /// `None` when the folded bar keeps the theme's title bar.
    folded: Option<FoldedBar>,
}

static APPEARANCE: OnceLock<Appearance> = OnceLock::new();

/// Fix the appearance for the rest of the process. Only the first call counts.
pub(crate) fn install(theme: Theme, folded: FoldedBarColor) {
    let _ = APPEARANCE.set(appearance(theme, folded));
}

fn appearance(theme: Theme, folded: FoldedBarColor) -> Appearance {
    let palette = match theme {
        Theme::Dark => dark::palette(),
        Theme::Light => light::palette(),
        Theme::Nord => nord::palette(),
    };
    Appearance {
        palette,
        folded: folded.rgb().map(FoldedBar::from_rgb),
    }
}

/// The installed appearance, or the defaults when nothing was installed.
fn current() -> &'static Appearance {
    APPEARANCE.get_or_init(|| appearance(Theme::default(), FoldedBarColor::default()))
}

pub(crate) fn palette() -> &'static Palette {
    &current().palette
}

/// The colors of a folded title bar, unless it keeps the theme's own.
pub(crate) fn folded_bar() -> Option<&'static FoldedBar> {
    current().folded.as_ref()
}

/// The fill of a hold button's progress, which in the folded bar has to read
/// over the bar's color rather than the theme's.
pub(crate) fn hold_progress(folded: bool) -> egui::Color32 {
    match folded_bar() {
        Some(bar) if folded => bar.progress,
        _ => palette().hold_progress_bg,
    }
}

/// Give egui the theme's widget colors. Both of egui's theme slots get them,
/// so the operating system's light or dark mode cannot pick other colors.
pub(crate) fn apply(ctx: &egui::Context) {
    let visuals = palette().visuals.clone();
    ctx.set_theme(if visuals.dark_mode {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    ctx.set_visuals_of(egui::Theme::Dark, visuals.clone());
    ctx.set_visuals_of(egui::Theme::Light, visuals);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_theme_reports_its_own_mode() {
        assert!(
            appearance(Theme::Dark, FoldedBarColor::Theme)
                .palette
                .visuals
                .dark_mode
        );
        assert!(
            !appearance(Theme::Light, FoldedBarColor::Theme)
                .palette
                .visuals
                .dark_mode
        );
        assert!(
            appearance(Theme::Nord, FoldedBarColor::Theme)
                .palette
                .visuals
                .dark_mode
        );
    }

    #[test]
    fn the_folded_bar_follows_the_theme_only_when_asked() {
        assert!(appearance(Theme::Dark, FoldedBarColor::Theme)
            .folded
            .is_none());
        let custom = appearance(Theme::Light, FoldedBarColor::Custom([10, 20, 30]));
        assert_eq!(
            custom.folded.map(|bar| bar.fill),
            Some(egui::Color32::from_rgb(10, 20, 30))
        );
    }
}
