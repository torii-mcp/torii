//! egui's light widgets, with the dark theme's accents darkened until they
//! read as text over a light panel.

use eframe::egui::{Color32, Visuals};

use super::Palette;

pub(super) fn palette() -> Palette {
    Palette {
        visuals: Visuals::light(),
        title_bar_bg: Color32::from_rgb(230, 231, 235),
        window_border: Color32::from_rgb(160, 163, 172),
        fixed_group_bg: Color32::from_rgb(226, 242, 245),
        fixed_pill_bg: Color32::from_rgb(201, 229, 235),
        fixed_stroke: Color32::from_rgb(22, 117, 132),
        fixed_text: Color32::from_rgb(12, 50, 58),
        free_group_bg: Color32::from_rgb(238, 240, 244),
        free_pill_bg: Color32::from_rgb(225, 228, 234),
        free_stroke: Color32::from_rgb(150, 157, 170),
        free_text: Color32::from_rgb(52, 59, 72),
        boundary_accent: Color32::from_rgb(140, 92, 0),
        scope_summary_bg: Color32::from_rgb(250, 240, 214),
        permanent_accept_stroke: Color32::from_rgb(54, 125, 46),
        permanent_deny_stroke: Color32::from_rgb(192, 46, 58),
        permanent_deny_progress_bg: Color32::from_rgb(242, 192, 197),
        target_warning_bg: Color32::from_rgb(252, 228, 228),
        target_warning_stroke: Color32::from_rgb(192, 46, 58),
        hold_progress_bg: Color32::from_rgb(182, 216, 162),
        agent_description_bg: Color32::from_rgb(229, 236, 252),
        agent_description_stroke: Color32::from_rgb(48, 92, 196),
        error: Color32::from_rgb(186, 38, 50),
        success: Color32::from_rgb(38, 118, 38),
    }
}
