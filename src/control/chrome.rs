//! Custom window chrome shared by the prompt windows.
//!
//! The prompt windows are wide and always on top, so they cover whatever the
//! human is reading while they decide. The native title bar cannot carry Torii
//! buttons, so the windows are undecorated and draw this bar instead: it drags
//! the window, closes it, and collapses it to its own height. Collapsed, the
//! bar keeps the window's primary action within one click, so a prompt can be
//! parked out of the way without losing the ability to answer it.

use eframe::egui;

/// Height of the custom title bar, and of the whole window when collapsed.
pub(crate) const TITLE_BAR_HEIGHT: f32 = 30.0;

const BAR_BUTTON_WIDTH: f32 = 26.0;
const TITLE_BAR_BG: egui::Color32 = egui::Color32::from_rgb(38, 38, 42);
const WINDOW_BORDER: egui::Color32 = egui::Color32::from_rgb(90, 90, 98);

/// What the human asked of the chrome on this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChromeEvent {
    None,
    Close,
    /// Collapse to the title bar, or restore the full window.
    Toggle,
}

/// Collapse state of a prompt window, and the height to restore it to.
pub(crate) struct WindowState {
    collapsed: bool,
    expanded_height: f32,
    /// A height asked of the window manager, retried until it is accepted.
    pending: Option<f32>,
    /// Set while the pointer that toggled the collapse is still down. The quick
    /// actions appear where the toggle was clicked, and egui hands a widget the
    /// press that started on the widget occupying its slot: without this the
    /// click that collapses the window would also fire the quick action that
    /// takes its place.
    guard: bool,
}

impl WindowState {
    pub(crate) fn new(expanded_height: f32) -> Self {
        Self {
            collapsed: false,
            expanded_height,
            pending: None,
            guard: false,
        }
    }

    pub(crate) fn collapsed(&self) -> bool {
        self.collapsed
    }

    /// The height to restore, when the window's own layout changes while it
    /// is collapsed.
    pub(crate) fn set_expanded_height(&mut self, height: f32) {
        self.expanded_height = height;
    }

    /// Whether the quick actions may be shown this frame: not while the click
    /// that toggled the collapse is still being held.
    pub(crate) fn quick_actions_armed(&mut self, ctx: &egui::Context) -> bool {
        if self.guard && !ctx.input(|input| input.pointer.any_down()) {
            self.guard = false;
        }
        !self.guard
    }

    pub(crate) fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
        self.guard = true;
        self.pending = Some(if self.collapsed {
            TITLE_BAR_HEIGHT
        } else {
            self.expanded_height
        });
    }

    /// Restore the full window, when a quick action means the human is coming
    /// back to it.
    pub(crate) fn expand(&mut self) {
        if self.collapsed {
            self.toggle();
        }
    }

    /// Push a pending collapse or restore to the window manager. Returns
    /// whether one was in flight, so a window that also resizes itself for its
    /// own reasons leaves this frame's height alone.
    pub(crate) fn apply(&mut self, ctx: &egui::Context) -> bool {
        let Some(height) = self.pending else {
            return false;
        };
        if set_window_height(ctx, height) {
            self.pending = None;
        }
        true
    }
}

/// Resize the window to `height`, keeping its top-left corner where it is.
///
/// Returns whether the current geometry was known yet: before the first frame
/// reports it there is no width to preserve, so the caller retries.
pub(crate) fn set_window_height(ctx: &egui::Context, height: f32) -> bool {
    let Some(width) = ctx.input(|input| input.viewport().inner_rect.map(|rect| rect.width()))
    else {
        return false;
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
    ctx.request_repaint();
    true
}

/// Draw the title bar. `quick` renders the window's own quick actions and is
/// only called while collapsed, laid out right to left from the chrome buttons.
///
/// Folded, the window is one bar tall and a tooltip would be clamped back over
/// the very button it describes, blocking the click. So the chrome explains
/// itself only while the window is open, and the quick actions never do: their
/// labels are the same words as the buttons they stand for.
pub(crate) fn title_bar(
    ctx: &egui::Context,
    title: &str,
    collapsed: bool,
    quick_armed: bool,
    quick: impl FnOnce(&mut egui::Ui),
) -> ChromeEvent {
    let mut event = ChromeEvent::None;
    egui::TopBottomPanel::top("torii_title_bar")
        .resizable(false)
        .exact_height(TITLE_BAR_HEIGHT)
        .show_separator_line(true)
        .frame(
            egui::Frame::none()
                .fill(TITLE_BAR_BG)
                .inner_margin(egui::Margin::symmetric(8.0, 0.0)),
        )
        .show(ctx, |ui| {
            // Claimed before the buttons so they keep the clicks that land on
            // them: the bar itself only drags where no button sits.
            let bar = ui.interact(
                ui.max_rect(),
                ui.id().with("drag"),
                egui::Sense::click_and_drag(),
            );
            if bar.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if bar.double_clicked() {
                event = ChromeEvent::Toggle;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if close_button(ui, collapsed).clicked() {
                    event = ChromeEvent::Close;
                }
                if fold_button(ui, collapsed).clicked() {
                    event = ChromeEvent::Toggle;
                }
                if collapsed && quick_armed {
                    ui.add_space(6.0);
                    // Stable ids: a quick action must never inherit the press
                    // that landed on the chrome button it replaced.
                    ui.push_id("torii_quick_actions", quick);
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(title).strong())
                            .truncate()
                            .selectable(false),
                    );
                });
            });
        });
    event
}

/// An undecorated window has no frame of its own; outline it so it stays
/// readable over whatever it covers.
pub(crate) fn paint_border(ctx: &egui::Context) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("torii_window_border"),
    ));
    painter.rect_stroke(
        ctx.screen_rect().shrink(0.5),
        0.0,
        egui::Stroke::new(1.0, WINDOW_BORDER),
    );
}

/// The chrome buttons are drawn, not typed: the bundled fonts have no glyph
/// for most window-control symbols and a missing one renders as an empty box.
fn chrome_button(
    ui: &mut egui::Ui,
    id: &str,
    hint: Option<&str>,
) -> (egui::Response, egui::Painter) {
    let mut response = ui
        .push_id(id, |ui| {
            ui.allocate_response(
                egui::vec2(BAR_BUTTON_WIDTH, TITLE_BAR_HEIGHT - 6.0),
                egui::Sense::click(),
            )
        })
        .inner;
    if let Some(hint) = hint {
        response = response.on_hover_text(hint);
    }
    let painter = ui.painter_at(response.rect);
    if response.hovered() {
        painter.rect_filled(
            response.rect,
            3.0,
            ui.visuals().widgets.hovered.weak_bg_fill,
        );
    }
    (response, painter)
}

fn chrome_stroke(ui: &egui::Ui, response: &egui::Response) -> egui::Stroke {
    let color = if response.hovered() {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    egui::Stroke::new(1.4, color)
}

fn close_button(ui: &mut egui::Ui, collapsed: bool) -> egui::Response {
    let (response, painter) = chrome_button(ui, "close", (!collapsed).then_some("Fechar"));
    let stroke = chrome_stroke(ui, &response);
    let cross = response.rect.center();
    let arm = 4.5;
    painter.line_segment(
        [
            egui::pos2(cross.x - arm, cross.y - arm),
            egui::pos2(cross.x + arm, cross.y + arm),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(cross.x - arm, cross.y + arm),
            egui::pos2(cross.x + arm, cross.y - arm),
        ],
        stroke,
    );
    response
}

/// Fold and unfold, not minimize: the window keeps its width and its place, so
/// the icon is a chevron pointing the way the body will move, over the line
/// that stands for the title bar it folds into.
fn fold_button(ui: &mut egui::Ui, collapsed: bool) -> egui::Response {
    let hint =
        (!collapsed).then_some("Dobrar a janela até esta barra, mantendo a largura e a posição");
    let (response, painter) = chrome_button(ui, "fold", hint);
    let stroke = chrome_stroke(ui, &response);
    let center = response.rect.center();
    let half = 5.0;
    let rise = 3.0;
    let gap = 4.0;
    // The line stands for this title bar, always above: the chevron points up
    // into it to fold, and down out of it to unfold.
    let line_y = center.y - gap - rise;
    painter.line_segment(
        [
            egui::pos2(center.x - half, line_y),
            egui::pos2(center.x + half, line_y),
        ],
        stroke,
    );
    let tip_y = if collapsed {
        center.y + rise
    } else {
        center.y - rise
    };
    let base_y = if collapsed {
        center.y - rise
    } else {
        center.y + rise
    };
    painter.line_segment(
        [
            egui::pos2(center.x - half, base_y),
            egui::pos2(center.x, tip_y),
        ],
        stroke,
    );
    painter.line_segment(
        [
            egui::pos2(center.x, tip_y),
            egui::pos2(center.x + half, base_y),
        ],
        stroke,
    );
    response
}
