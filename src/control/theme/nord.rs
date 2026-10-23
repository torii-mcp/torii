//! The Nord palette (<https://www.nordtheme.com>): Polar Night for the
//! surfaces, Snow Storm for the text, Frost for what the rule fixes and Aurora
//! for the states.

use eframe::egui::{Color32, Stroke, Visuals};

use super::Palette;

const NORD0: Color32 = Color32::from_rgb(46, 52, 64);
const NORD1: Color32 = Color32::from_rgb(59, 66, 82);
const NORD2: Color32 = Color32::from_rgb(67, 76, 94);
const NORD3: Color32 = Color32::from_rgb(76, 86, 106);
const NORD4: Color32 = Color32::from_rgb(216, 222, 233);
const NORD6: Color32 = Color32::from_rgb(236, 239, 244);
const NORD8: Color32 = Color32::from_rgb(136, 192, 208);
const NORD9: Color32 = Color32::from_rgb(129, 161, 193);
const NORD10: Color32 = Color32::from_rgb(94, 129, 172);
const NORD11: Color32 = Color32::from_rgb(191, 97, 106);
const NORD13: Color32 = Color32::from_rgb(235, 203, 139);
const NORD14: Color32 = Color32::from_rgb(163, 190, 140);

pub(super) fn palette() -> Palette {
    Palette {
        visuals: visuals(),
        title_bar_bg: NORD1,
        window_border: NORD3,
        fixed_group_bg: Color32::from_rgb(52, 68, 82),
        fixed_pill_bg: Color32::from_rgb(64, 88, 104),
        fixed_stroke: NORD8,
        fixed_text: NORD6,
        free_group_bg: NORD1,
        free_pill_bg: NORD2,
        free_stroke: Color32::from_rgb(97, 108, 130),
        free_text: NORD4,
        boundary_accent: NORD13,
        scope_summary_bg: Color32::from_rgb(72, 68, 64),
        permanent_accept_stroke: NORD14,
        permanent_deny_stroke: NORD11,
        permanent_deny_progress_bg: Color32::from_rgb(110, 62, 72),
        target_warning_bg: Color32::from_rgb(78, 56, 66),
        target_warning_stroke: NORD11,
        hold_progress_bg: Color32::from_rgb(88, 110, 84),
        agent_description_bg: Color32::from_rgb(52, 64, 86),
        agent_description_stroke: NORD9,
        error: NORD11,
        success: NORD14,
    }
}

/// egui's dark widgets repainted with Polar Night and Snow Storm.
fn visuals() -> Visuals {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = NORD0;
    visuals.window_fill = NORD0;
    visuals.window_stroke = Stroke::new(1.0_f32, NORD3);
    visuals.extreme_bg_color = Color32::from_rgb(39, 44, 54);
    visuals.faint_bg_color = NORD1;
    visuals.code_bg_color = NORD1;
    visuals.hyperlink_color = NORD8;
    visuals.warn_fg_color = NORD13;
    visuals.error_fg_color = NORD11;
    visuals.selection.bg_fill = NORD10;
    visuals.selection.stroke = Stroke::new(1.0_f32, NORD6);

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = NORD0;
    widgets.noninteractive.weak_bg_fill = NORD0;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, NORD2);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, NORD4);
    for (widget, fill, outline, text) in [
        (&mut widgets.inactive, NORD2, Stroke::NONE, NORD4),
        (
            &mut widgets.hovered,
            NORD3,
            Stroke::new(1.0_f32, NORD9),
            NORD6,
        ),
        (
            &mut widgets.active,
            NORD3,
            Stroke::new(1.0_f32, NORD8),
            NORD6,
        ),
        (&mut widgets.open, NORD2, Stroke::new(1.0_f32, NORD3), NORD6),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = outline;
        widget.fg_stroke = Stroke::new(widget.fg_stroke.width, text);
    }
    visuals
}
