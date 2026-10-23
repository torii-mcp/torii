//! The default theme: egui's dark widgets with Torii's own accents.

use eframe::egui::{Color32, Visuals};

use super::Palette;

pub(super) fn palette() -> Palette {
    Palette {
        visuals: Visuals::dark(),
        title_bar_bg: Color32::from_rgb(38, 38, 42),
        window_border: Color32::from_rgb(90, 90, 98),
        fixed_group_bg: Color32::from_rgb(22, 42, 48),
        fixed_pill_bg: Color32::from_rgb(32, 61, 74),
        fixed_stroke: Color32::from_rgb(86, 182, 194),
        fixed_text: Color32::from_rgb(229, 246, 248),
        free_group_bg: Color32::from_rgb(32, 35, 42),
        free_pill_bg: Color32::from_rgb(44, 49, 58),
        free_stroke: Color32::from_rgb(92, 99, 112),
        free_text: Color32::from_rgb(198, 203, 211),
        boundary_accent: Color32::from_rgb(229, 192, 123),
        scope_summary_bg: Color32::from_rgb(45, 40, 28),
        permanent_accept_stroke: Color32::from_rgb(120, 174, 108),
        permanent_deny_stroke: Color32::from_rgb(224, 108, 117),
        permanent_deny_progress_bg: Color32::from_rgb(112, 47, 51),
        target_warning_bg: Color32::from_rgb(63, 31, 31),
        target_warning_stroke: Color32::from_rgb(224, 108, 117),
        hold_progress_bg: Color32::from_rgb(77, 105, 58),
        agent_description_bg: Color32::from_rgb(28, 36, 54),
        agent_description_stroke: Color32::from_rgb(122, 162, 247),
        error: Color32::from_rgb(224, 108, 117),
        success: Color32::from_rgb(152, 195, 121),
    }
}
