use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::egui;
use serde::{Deserialize, Serialize};

use super::{
    AccessChoice, ActiveTargetAuthorization, AuthPromptResult, AuthValidation, GrantSelection,
    PermanentPolicy, TargetAccessChoice,
};
use crate::error::{Error, Result};
use crate::providers::AuthField;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PromptRequest {
    Access {
        provider: String,
        args: Vec<String>,
        default_minutes: u32,
        /// Time left on the oldest grant still valid, offered as the first preset.
        align_seconds: Option<u32>,
        /// What a permanent decision would write, and whether it can be written.
        permanent: PermanentPolicy,
    },
    TargetAccess {
        provider: String,
        requested_target: String,
        requested_binding: String,
        active_targets: Vec<ActiveTargetAuthorization>,
        default_minutes: u32,
    },
    Auth {
        provider: String,
        fields: Vec<AuthField>,
        error: Option<String>,
        validation: AuthValidation,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum PromptResponse {
    Access(AccessChoice),
    TargetAccess(TargetAccessChoice),
    Auth(AuthPromptResult),
    Error(String),
}

pub async fn ask_access(
    provider: &str,
    args: &[String],
    default_minutes: u32,
    align_seconds: Option<u32>,
    permanent: PermanentPolicy,
) -> Result<AccessChoice> {
    let request = PromptRequest::Access {
        provider: provider.into(),
        args: args.to_vec(),
        default_minutes,
        align_seconds,
        permanent,
    };
    match invoke_child(request).await? {
        PromptResponse::Access(choice) => Ok(choice),
        PromptResponse::Error(message) => Err(Error::Prompt(message)),
        PromptResponse::TargetAccess(_) | PromptResponse::Auth(_) => {
            Err(Error::Prompt("unexpected access prompt response".into()))
        }
    }
}

pub async fn ask_target_access(
    provider: &str,
    requested_target: &str,
    requested_binding: &str,
    active_targets: &[ActiveTargetAuthorization],
    default_minutes: u32,
) -> Result<TargetAccessChoice> {
    let request = PromptRequest::TargetAccess {
        provider: provider.into(),
        requested_target: requested_target.into(),
        requested_binding: requested_binding.into(),
        active_targets: active_targets.to_vec(),
        default_minutes,
    };
    match invoke_child(request).await? {
        PromptResponse::TargetAccess(choice) => Ok(choice),
        PromptResponse::Error(message) => Err(Error::Prompt(message)),
        PromptResponse::Access(_) | PromptResponse::Auth(_) => Err(Error::Prompt(
            "unexpected target access prompt response".into(),
        )),
    }
}

pub async fn ask_auth(
    provider: &str,
    fields: &[AuthField],
    error: Option<&str>,
    validation: AuthValidation,
) -> Result<AuthPromptResult> {
    let request = PromptRequest::Auth {
        provider: provider.into(),
        fields: fields.to_vec(),
        error: error.map(str::to_owned),
        validation,
    };
    match invoke_child(request).await? {
        PromptResponse::Auth(result) => Ok(result),
        PromptResponse::Error(message) => Err(Error::Prompt(message)),
        PromptResponse::Access(_) | PromptResponse::TargetAccess(_) => Err(Error::Prompt(
            "unexpected authentication prompt response".into(),
        )),
    }
}

async fn invoke_child(request: PromptRequest) -> Result<PromptResponse> {
    tokio::task::spawn_blocking(move || {
        let executable =
            std::env::current_exe().map_err(|error| Error::Prompt(error.to_string()))?;
        let mut child = Command::new(executable)
            .arg("__prompt")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| Error::Prompt(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Prompt("prompt stdin unavailable".into()))?;
        let mut writer = BufWriter::new(stdin);
        serde_json::to_writer(&mut writer, &request)
            .map_err(|error| Error::Prompt(error.to_string()))?;
        writer
            .flush()
            .map_err(|error| Error::Prompt(error.to_string()))?;
        drop(writer);
        let output = child
            .wait_with_output()
            .map_err(|error| Error::Prompt(error.to_string()))?;
        if !output.status.success() {
            return Err(Error::Prompt("prompt process failed".into()));
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|error| Error::Prompt(format!("invalid prompt response: {error}")))
    })
    .await
    .map_err(|error| Error::Prompt(error.to_string()))?
}

pub fn run_child() -> i32 {
    let stdin = std::io::stdin();
    let request = serde_json::from_reader::<_, PromptRequest>(BufReader::new(stdin.lock()));
    match request {
        Ok(PromptRequest::Access {
            provider,
            args,
            default_minutes,
            align_seconds,
            permanent,
        }) => {
            let response = PromptResponse::Access(
                access_window(provider, args, default_minutes, align_seconds, permanent)
                    .unwrap_or(AccessChoice::Deny),
            );
            if serde_json::to_writer(std::io::stdout(), &response).is_ok() {
                0
            } else {
                1
            }
        }
        Ok(PromptRequest::TargetAccess {
            provider,
            requested_target,
            requested_binding,
            active_targets,
            default_minutes,
        }) => {
            let response = PromptResponse::TargetAccess(target_access_window(
                provider,
                requested_target,
                requested_binding,
                active_targets,
                default_minutes,
            ));
            if serde_json::to_writer(std::io::stdout(), &response).is_ok() {
                0
            } else {
                1
            }
        }
        Ok(PromptRequest::Auth {
            provider,
            fields,
            error,
            validation,
        }) => {
            let response = PromptResponse::Auth(auth_window(provider, fields, error, validation));
            if serde_json::to_writer(std::io::stdout(), &response).is_ok() {
                0
            } else {
                1
            }
        }
        Err(error) => {
            let response = PromptResponse::Error(error.to_string());
            if serde_json::to_writer(std::io::stdout(), &response).is_ok() {
                0
            } else {
                1
            }
        }
    }
}

fn native_options(width: f32, height: f32) -> eframe::NativeOptions {
    eframe::NativeOptions {
        centered: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_icon(prompt_icon())
            .with_resizable(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_always_on_top()
            .with_active(true),
        ..Default::default()
    }
}

fn prompt_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../../assets/torii-icon.png"))
        .expect("embedded Torii icon must be a valid PNG")
}

fn close(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    ctx.request_repaint();
}

fn display_token(value: &str, max_chars: usize) -> String {
    let escaped = serde_json::to_string(value).unwrap_or_else(|_| "\"<invalid>\"".into());
    if escaped.chars().count() <= max_chars {
        escaped
    } else {
        let mut shortened = escaped
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        shortened.push('…');
        shortened
    }
}

const ACCESS_WIDTH: f32 = 820.0;
const ACCESS_ONCE_HEIGHT: f32 = 360.0;
const ACCESS_TIMED_EXACT_HEIGHT: f32 = 470.0;
const ACCESS_TIMED_PREFIX_HEIGHT: f32 = 620.0;
const ACCESS_DETAILS_EXTRA_HEIGHT: f32 = 90.0;
const ACCESS_MAX_HEIGHT: f32 = 700.0;
/// Subpixel differences must not make the window chase its own measurement.
const ACCESS_HEIGHT_TOLERANCE: f32 = 1.0;
/// Standard durations offered beside the duration field, in minutes.
const DURATION_PRESET_MINUTES: [u32; 5] = [5, 10, 15, 20, 30];
const DURATION_PRESET_WIDTH: f32 = 46.0;
const TOKEN_COMPACT_CHARS: usize = 48;
const TOKEN_TOOLTIP_CHARS: usize = 512;
const TOKEN_DETAIL_PAGE_CHARS: usize = 4096;

const FIXED_GROUP_BG: egui::Color32 = egui::Color32::from_rgb(22, 42, 48);
const FIXED_PILL_BG: egui::Color32 = egui::Color32::from_rgb(32, 61, 74);
const FIXED_STROKE: egui::Color32 = egui::Color32::from_rgb(86, 182, 194);
const FIXED_TEXT: egui::Color32 = egui::Color32::from_rgb(229, 246, 248);
const FREE_GROUP_BG: egui::Color32 = egui::Color32::from_rgb(32, 35, 42);
const FREE_PILL_BG: egui::Color32 = egui::Color32::from_rgb(44, 49, 58);
const FREE_STROKE: egui::Color32 = egui::Color32::from_rgb(92, 99, 112);
const FREE_TEXT: egui::Color32 = egui::Color32::from_rgb(198, 203, 211);
const BOUNDARY_ACCENT: egui::Color32 = egui::Color32::from_rgb(229, 192, 123);
const SCOPE_SUMMARY_BG: egui::Color32 = egui::Color32::from_rgb(45, 40, 28);
const PERMANENT_ACCEPT_STROKE: egui::Color32 = egui::Color32::from_rgb(120, 174, 108);
const PERMANENT_DENY_STROKE: egui::Color32 = egui::Color32::from_rgb(224, 108, 117);
const PERMANENT_DENY_PROGRESS_BG: egui::Color32 = egui::Color32::from_rgb(112, 47, 51);

/// `5 min`, `4 min 30 s` — for sentences.
fn spelled_duration(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (0, rest) => format!("{rest} s"),
        (minutes, 0) => format!("{minutes} min"),
        (minutes, rest) => format!("{minutes} min {rest} s"),
    }
}

/// `5`, `4:30` — for the preset buttons.
fn compact_duration(seconds: u32) -> String {
    match (seconds / 60, seconds % 60) {
        (minutes, 0) => minutes.to_string(),
        (minutes, rest) => format!("{minutes}:{rest:02}"),
    }
}

fn access_height(timed: bool, prefix: bool, details_open: bool) -> f32 {
    let base = match (timed, prefix) {
        (false, _) => ACCESS_ONCE_HEIGHT,
        (true, false) => ACCESS_TIMED_EXACT_HEIGHT,
        (true, true) => ACCESS_TIMED_PREFIX_HEIGHT,
    };
    if details_open {
        (base + ACCESS_DETAILS_EXTRA_HEIGHT).min(ACCESS_MAX_HEIGHT)
    } else {
        base
    }
}

/// Height of the authorization window: the selected flow sets a floor, the body
/// measured on the last frame may raise it, and the cap keeps the window usable
/// on a small screen. Beyond the cap the body scrolls instead of being cut off.
fn access_window_height(flow: f32, measured: Option<f32>) -> f32 {
    measured
        .map_or(flow, |measured| measured.max(flow))
        .min(ACCESS_MAX_HEIGHT)
}

fn suggested_prefix_len(args: &[String]) -> Option<usize> {
    let boundary = args.iter().position(|argument| argument.starts_with('-'))?;
    (boundary >= 2).then_some(boundary)
}

fn request_access_height(ctx: &egui::Context, height: f32) -> bool {
    let geometry = ctx.input(|input| {
        let viewport = input.viewport();
        viewport
            .inner_rect
            .zip(viewport.outer_rect)
            .map(|(inner, outer)| (inner.width(), inner.height(), outer.min))
    });
    let Some((width, current_height, outer_min)) = geometry else {
        return false;
    };
    let position = centered_resize_position(current_height, outer_min, height);
    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(position));
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(width, height)));
    ctx.request_repaint();
    true
}

fn centered_resize_position(
    current_height: f32,
    current_outer_min: egui::Pos2,
    requested_height: f32,
) -> egui::Pos2 {
    egui::pos2(
        current_outer_min.x,
        current_outer_min.y - (requested_height - current_height) / 2.0,
    )
}

fn format_byte_count(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn escaped_fragment(value: &str) -> String {
    let escaped = serde_json::to_string(value).unwrap_or_else(|_| "\"<invalid>\"".into());
    escaped
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(&escaped)
        .to_owned()
}

fn compact_token_label(value: &str, index: usize) -> String {
    if value.chars().nth(TOKEN_COMPACT_CHARS).is_none() {
        return display_token(value, usize::MAX);
    }
    let head = value.chars().take(24).collect::<String>();
    let mut tail = value.chars().rev().take(16).collect::<Vec<_>>();
    tail.reverse();
    let tail = tail.into_iter().collect::<String>();
    format!(
        "#{} \"{}…{}\" · {}",
        index + 1,
        escaped_fragment(&head),
        escaped_fragment(&tail),
        format_byte_count(value.len())
    )
}

fn bounded_token_preview(value: &str, max_chars: usize) -> String {
    let preview = value.chars().take(max_chars).collect::<String>();
    let truncated = preview.len() < value.len();
    let mut escaped = serde_json::to_string(&preview).unwrap_or_else(|_| "\"<invalid>\"".into());
    if truncated {
        escaped.pop();
        escaped.push('…');
        escaped.push('"');
    }
    escaped
}

fn token_detail_page(value: &str, page: usize) -> String {
    let start = page.saturating_mul(TOKEN_DETAIL_PAGE_CHARS);
    let chunk = value
        .chars()
        .skip(start)
        .take(TOKEN_DETAIL_PAGE_CHARS)
        .collect::<String>();
    serde_json::to_string(&chunk).unwrap_or_else(|_| "\"<invalid>\"".into())
}

struct AccessApp {
    provider: String,
    args: Vec<String>,
    /// What a permanent decision would write, decided by the server.
    permanent: PermanentPolicy,
    /// When the window opened, for the arming delay of the permanent buttons.
    opened_at: Instant,
    accept_hold: HoldState,
    deny_hold: HoldState,
    hold: HoldState,
    timed: bool,
    prefix: bool,
    prefix_len: usize,
    suggested_prefix_len: Option<usize>,
    /// The chosen duration in seconds: the alignment preset is rarely a whole
    /// number of minutes.
    seconds: u32,
    /// Time left on the oldest grant still valid, when there is one.
    align_seconds: Option<u32>,
    arg_char_counts: Vec<usize>,
    long_args: Vec<usize>,
    details_arg: Option<usize>,
    details_page: usize,
    requested_height: f32,
    pending_height: Option<f32>,
    /// Total height the body asked for on the last frame, chrome included.
    measured_height: Option<f32>,
    decision_since: Option<Instant>,
    outcome: Rc<RefCell<Option<AccessChoice>>>,
}

impl eframe::App for AccessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_window_height(ctx);
        let decision = *self.outcome.borrow();
        if let Some(decision_since) = self.decision_since {
            // A permanent decision names a file on the way out; the window stays up
            // long enough for that sentence to be read.
            let visible_for = decision.map_or(PROMPT_TERMINAL_VISIBLE_FOR, terminal_visible_for);
            let elapsed = decision_since.elapsed();
            if elapsed >= visible_for {
                close(ctx);
            } else {
                ctx.request_repaint_after(visible_for - elapsed);
            }
        }
        let decided = decision.is_some();

        egui::TopBottomPanel::bottom("access_status")
            .resizable(false)
            .exact_height(PROMPT_STATUS_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::symmetric(6.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.with_layout(
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| match decision {
                        Some(AccessChoice::Deny) => {
                            ui.label(
                                egui::RichText::new("Acesso negado.").color(PROMPT_ERROR_COLOR),
                            );
                        }
                        Some(AccessChoice::AllowOnce) => {
                            ui.label(
                                egui::RichText::new("👍 Acesso autorizado uma vez.")
                                    .color(PROMPT_SUCCESS_COLOR),
                            );
                        }
                        Some(AccessChoice::AllowFor { seconds, .. }) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "👍 Acesso autorizado por {}.",
                                    spelled_duration(seconds)
                                ))
                                .color(PROMPT_SUCCESS_COLOR),
                            );
                        }
                        Some(AccessChoice::AllowAlways { prefix_len }) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "👍 Gravando accept {} na {}.",
                                    self.permanent_rule(prefix_len),
                                    self.permanent.scope
                                ))
                                .color(PROMPT_SUCCESS_COLOR),
                            );
                        }
                        Some(AccessChoice::DenyAlways { prefix_len }) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Negado. Gravando deny {} na {}.",
                                    self.permanent_rule(prefix_len),
                                    self.permanent.scope
                                ))
                                .color(PROMPT_ERROR_COLOR),
                            );
                        }
                        None if self.timed => {
                            ui.label(format!(
                                "Segure o botão para permitir por {}.",
                                spelled_duration(self.seconds)
                            ));
                        }
                        None => {
                            ui.label("Segure o botão para permitir uma vez.");
                        }
                    },
                );
            });

        egui::TopBottomPanel::bottom("access_actions")
            .resizable(false)
            .exact_height(28.0)
            .show_separator_line(true)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!decided, |ui| {
                        if ui.button("Negar").clicked() {
                            self.finish_decision(ctx, AccessChoice::Deny);
                        }
                    });
                    // Press-and-hold to confirm: the deliberate gesture replaces
                    // the old "I reviewed this" checkbox as the confirmation gate.
                    let label = if self.timed {
                        format!("Segure para permitir por {}", spelled_duration(self.seconds))
                    } else {
                        "Segure para permitir uma vez".to_string()
                    };
                    let slots = reserve_hold_paint(ui);
                    let response = ui
                        .add_enabled(
                            !decided,
                            egui::Button::new(&label)
                                .fill(egui::Color32::TRANSPARENT)
                                .min_size(egui::vec2(
                                    ALLOW_HOLD_BUTTON_WIDTH,
                                    ui.spacing().interact_size.y,
                                ))
                                .sense(egui::Sense::click_and_drag()),
                        )
                        .on_hover_text(
                            "Mantenha pressionado por 1 segundo para revisar e confirmar a permissão.",
                        );
                    let (pointer_down, focused) =
                        ctx.input(|input| (input.pointer.primary_down(), input.focused));
                    let pressing = !decided
                        && response.is_pointer_button_down_on()
                        && response.contains_pointer();
                    let (progress, confirmed) = hold_update(
                        &mut self.hold,
                        pressing,
                        pointer_down,
                        focused,
                        Instant::now(),
                    );
                    paint_hold_bar(ui, &slots, &response, progress, HOLD_PROGRESS_BG);
                    if pressing {
                        ctx.request_repaint_after(Duration::from_millis(16));
                    }
                    if confirmed {
                        self.finish_decision(
                            ctx,
                            if self.timed {
                                AccessChoice::AllowFor {
                                    seconds: self.seconds,
                                    selection: if self.prefix {
                                        GrantSelection::Prefix {
                                            token_count: self.prefix_len,
                                        }
                                    } else {
                                        GrantSelection::Exact
                                    },
                                }
                            } else {
                                AccessChoice::AllowOnce
                            },
                        );
                    }
                });
            });

        egui::TopBottomPanel::bottom("access_scope_summary")
            .resizable(false)
            .exact_height(28.0)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(SCOPE_SUMMARY_BG)
                    .inner_margin(egui::Margin::symmetric(8.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(self.scope_summary())
                            .strong()
                            .color(BOUNDARY_ACCENT),
                    );
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            // How tall the body really is only becomes known after layout: the
            // argument pills wrap into as many lines as the longest invocation
            // needs. Measure the body and let the window follow it, and keep the
            // body scrollable so nothing — the duration control above all — can
            // end up parked under the bottom panels.
            let chrome = (ctx.screen_rect().height() - ui.available_height()).max(0.0);
            let body = egui::ScrollArea::vertical()
                .id_salt("access_body")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| self.render_body(ui, ctx, decided));
            self.measured_height = Some(chrome + body.content_size.y);
        });
    }
}

impl AccessApp {
    /// What the call is, how it may be authorized, and for how long.
    fn render_body(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, decided: bool) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
        ui.heading(format!("Torii — autorização ({})", self.provider));
        ui.label("A ação não está resolvida pela política.");
        ui.add_space(2.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!(
                "Invocação solicitada · {} argumentos",
                self.args.len()
            )));
            if !self.long_args.is_empty() {
                const DETAILS_BUTTON_WIDTH: f32 = 210.0;
                ui.add_space((ui.available_width() - DETAILS_BUTTON_WIDTH).max(0.0));
                let label = if self.details_arg.is_some() {
                    "Ocultar detalhes".into()
                } else if self.long_args.len() == 1 {
                    "Revisar 1 argumento longo".into()
                } else {
                    format!("Revisar {} argumentos longos", self.long_args.len())
                };
                if ui.button(label).clicked() {
                    if self.details_arg.is_some() {
                        self.details_arg = None;
                    } else {
                        self.details_arg = self.long_args.first().copied();
                        self.details_page = 0;
                    }
                }
            }
        });
        let editing_prefix = self.timed && self.prefix && !decided;
        if editing_prefix {
            ui.small("Os argumentos estão no editor de escopo abaixo.");
        } else {
            egui::ScrollArea::vertical()
                .id_salt("access_arguments")
                .max_height(105.0)
                .min_scrolled_height(36.0)
                .auto_shrink([false, true])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(ui, |ui| {
                    self.render_argument_strip(ui);
                });
        }
        if self.details_arg.is_some() {
            self.render_argument_details(ui);
        }
        ui.add_space(2.0);
        ui.separator();
        ui.label(egui::RichText::new("Como autorizar?").strong());
        let once_changed = ui.radio_value(&mut self.timed, false, "Uma vez").changed();
        let temporary_changed = ui
            .radio_value(&mut self.timed, true, "Temporariamente")
            .changed();
        if once_changed || temporary_changed {
            self.reset_holds();
        }
        if temporary_changed {
            // The prefix option is the initial one when there is a structural
            // boundary to offer. Without one it would only widen the scope over
            // the exact invocation, so the exact one stays selected.
            if let Some(suggested) = self.suggested_prefix_len {
                self.prefix = true;
                self.prefix_len = suggested;
            } else {
                self.prefix = false;
                self.prefix_len = self.args.len();
            }
        }
        if self.timed {
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.label(egui::RichText::new("Escopo temporário").strong());
                let prefix_label = match self.suggested_prefix_len {
                    Some(suggested) if self.prefix_len == suggested => {
                        format!("Prefixo sugerido · {suggested} argumentos")
                    }
                    Some(_) => format!(
                        "Prefixo personalizado · {} argumentos",
                        self.prefix_len
                    ),
                    None => "Prefixo personalizado".into(),
                };
                let mut prefix_changed = false;
                if self.suggested_prefix_len.is_some() {
                    prefix_changed = ui
                        .radio_value(&mut self.prefix, true, &prefix_label)
                        .changed();
                }
                let exact_changed = ui
                    .radio_value(
                        &mut self.prefix,
                        false,
                        "Somente esta invocação exata",
                    )
                    .changed();
                if self.suggested_prefix_len.is_none() {
                    prefix_changed = ui
                        .radio_value(&mut self.prefix, true, &prefix_label)
                        .changed();
                }
                if prefix_changed {
                    if let Some(suggested) = self.suggested_prefix_len {
                        self.prefix_len = suggested;
                    }
                }
                if exact_changed || prefix_changed {
                    self.reset_holds();
                }
                if self.prefix {
                    if let Some(suggested) = self.suggested_prefix_len {
                        if self.prefix_len == suggested {
                            ui.small(format!(
                                "Sugestão pelo formato: fronteira antes de {}.",
                                bounded_token_preview(&self.args[suggested], 36)
                            ));
                        } else {
                            ui.horizontal(|ui| {
                                ui.small(format!(
                                    "Fronteira ajustada por você; a sugestão original era {suggested}."
                                ));
                                if ui.button("Restaurar sugestão").clicked() {
                                    self.prefix_len = suggested;
                                    self.reset_holds();
                                }
                            });
                        }
                    } else {
                        ui.small(
                            "Nenhuma fronteira estrutural foi encontrada; escolha o prefixo manualmente.",
                        );
                    }
                    ui.label("Clique nas pílulas ou use os controles para mover a fronteira.");
                    egui::ScrollArea::vertical()
                        .id_salt("access_prefix_editor")
                        .max_height(190.0)
                        .min_scrolled_height(80.0)
                        .auto_shrink([false, true])
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                        )
                        .show(ui, |ui| self.render_prefix_editor(ui, ctx, decided));
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                self.prefix_len > 1,
                                egui::Button::new("◀").small(),
                            )
                            .clicked()
                        {
                            self.prefix_len -= 1;
                            self.reset_holds();
                        }
                        ui.label(format!(
                            "{} de {} argumentos fixos",
                            self.prefix_len,
                            self.args.len()
                        ));
                        if ui
                            .add_enabled(
                                self.prefix_len < self.args.len(),
                                egui::Button::new("▶").small(),
                            )
                            .clicked()
                        {
                            self.prefix_len += 1;
                            self.reset_holds();
                        }
                    });
                    let fixed_long = self
                        .long_args
                        .iter()
                        .filter(|index| **index < self.prefix_len)
                        .count();
                    let free_long = self.long_args.len().saturating_sub(fixed_long);
                    if fixed_long > 0 || free_long > 0 {
                        ui.small(format!(
                            "Argumentos longos: {fixed_long} dentro do prefixo e {free_long} fora dele."
                        ));
                    }
                } else {
                    // A permanent rule is a literal one, and a literal rule always
                    // matches by prefix. There is no way to write "only this exact
                    // invocation" as a rule, so the permanent buttons live with the
                    // prefix and are not offered here.
                    ui.small(
                        egui::RichText::new(
                            "Decisão permanente fica disponível ao escolher um prefixo: uma regra literal sempre casa por prefixo.",
                        )
                        .color(BOUNDARY_ACCENT),
                    );
                }
                ui.small("Denies explícitos continuam prevalecendo.");
            });
            ui.horizontal(|ui| {
                ui.label("Duração:");
                // The field stays in minutes: dragging is for a rough value, the
                // presets on the right are for hitting an exact one.
                let mut minutes = (self.seconds / 60).max(1);
                if ui
                    .add(egui::DragValue::new(&mut minutes).range(1..=1440))
                    .changed()
                {
                    self.seconds = minutes * 60;
                    self.reset_holds();
                }
                ui.label("minutos");
                if let Some(align) = self.align_seconds {
                    if self
                        .duration_preset(ui, align, &compact_duration(align))
                        .on_hover_text(
                            "Termina junto com o acesso temporário mais antigo ainda válido.",
                        )
                        .clicked()
                    {
                        self.seconds = align;
                        self.reset_holds();
                    }
                }
                for preset in DURATION_PRESET_MINUTES {
                    let seconds = preset * 60;
                    if self
                        .duration_preset(ui, seconds, &preset.to_string())
                        .clicked()
                    {
                        self.seconds = seconds;
                        self.reset_holds();
                    }
                }
            });
            if !self.seconds.is_multiple_of(60) {
                ui.small(format!(
                    "Duração alinhada: {}. O campo mostra apenas os minutos inteiros.",
                    spelled_duration(self.seconds)
                ));
            }
        }
    }

    /// One of the chunky duration buttons to the right of the field.
    fn duration_preset(&self, ui: &mut egui::Ui, seconds: u32, label: &str) -> egui::Response {
        ui.add_sized(
            egui::vec2(DURATION_PRESET_WIDTH, ui.spacing().interact_size.y),
            egui::SelectableLabel::new(self.seconds == seconds, label),
        )
    }

    fn scope_summary(&self) -> String {
        match (self.timed, self.prefix) {
            (false, _) => "Esta chamada será executada uma vez, sem salvar permissão temporária."
                .into(),
            (true, false) => {
                "Todos os argumentos devem permanecer idênticos; qualquer diferença exige nova autorização."
                    .into()
            }
            (true, true) => format!(
                "Os primeiros {} de {} argumentos ficam fixos; o restante e novos argumentos podem variar.",
                self.prefix_len, self.args.len()
            ),
        }
    }

    fn sync_window_height(&mut self, ctx: &egui::Context) {
        let desired = access_window_height(
            access_height(self.timed, self.prefix, self.details_arg.is_some()),
            self.measured_height,
        );
        if (self.requested_height - desired).abs() > ACCESS_HEIGHT_TOLERANCE {
            self.requested_height = desired;
            self.pending_height = Some(desired);
        }
        if let Some(height) = self.pending_height {
            if request_access_height(ctx, height) {
                self.pending_height = None;
            }
        }
    }

    /// Any change to what is being authorized discards every gesture in flight.
    /// A hold that started at one prefix boundary must never confirm at another.
    fn reset_holds(&mut self) {
        self.hold = HoldState::default();
        self.accept_hold = HoldState::default();
        self.deny_hold = HoldState::default();
    }

    fn finish_decision(&mut self, ctx: &egui::Context, decision: AccessChoice) {
        self.reset_holds();
        *self.outcome.borrow_mut() = Some(decision);
        self.decision_since = Some(Instant::now());
        ctx.request_repaint_after(terminal_visible_for(decision));
    }

    /// One of the two permanent buttons in the corner of the fixed-prefix box.
    ///
    /// It acts on the boundary currently chosen in the editor beside it, is a
    /// press-and-hold of five fixed seconds, stays inert for a moment after the
    /// window opens, and is disabled outright when the server said this boundary
    /// could not be written — with the reason on the tooltip instead of a failure
    /// after the human already held it.
    fn render_permanent_button(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        kind: PermanentKind,
        decided: bool,
    ) {
        let option = self.permanent.at(self.prefix_len).cloned();
        let (label, blocked, available, rule) = match (&option, kind) {
            (None, PermanentKind::Accept) => (
                "Sempre permitir",
                Some("esta fronteira não tem regra correspondente".to_string()),
                false,
                String::new(),
            ),
            (None, PermanentKind::Deny) => (
                "Sempre negar",
                Some("esta fronteira não tem regra correspondente".to_string()),
                false,
                String::new(),
            ),
            (Some(option), PermanentKind::Accept) => (
                "Sempre permitir",
                option.accept_blocked.clone(),
                option.allows_accept(),
                option.rule.clone(),
            ),
            (Some(option), PermanentKind::Deny) => (
                "Sempre negar",
                option.deny_blocked.clone(),
                option.allows_deny(),
                option.rule.clone(),
            ),
        };
        let (stroke, progress_bg, vector) = match kind {
            PermanentKind::Accept => (PERMANENT_ACCEPT_STROKE, HOLD_PROGRESS_BG, "accept"),
            PermanentKind::Deny => (PERMANENT_DENY_STROKE, PERMANENT_DENY_PROGRESS_BG, "deny"),
        };

        let since_open = self.opened_at.elapsed();
        let armed = since_open >= PERMANENT_ARMING_DELAY;
        if !armed && !decided {
            ctx.request_repaint_after(PERMANENT_ARMING_DELAY - since_open);
        }
        let enabled = available && armed && !decided;

        let state = match kind {
            PermanentKind::Accept => &mut self.accept_hold,
            PermanentKind::Deny => &mut self.deny_hold,
        };
        let (pointer_down, focused) =
            ctx.input(|input| (input.pointer.primary_down(), input.focused));
        // The paint slots are reserved before the button so the progress bar goes
        // under its label; the button carries no fill of its own.
        let slots = reserve_hold_paint(ui);
        let response = ui.add_enabled(
            enabled,
            egui::Button::new(egui::RichText::new(label).small())
                .fill(egui::Color32::TRANSPARENT)
                .min_size(egui::vec2(
                    PERMANENT_BUTTON_WIDTH,
                    ui.spacing().interact_size.y,
                ))
                .stroke(egui::Stroke::new(1.0_f32, stroke))
                .sense(egui::Sense::click_and_drag()),
        );
        let pressing =
            enabled && response.is_pointer_button_down_on() && response.contains_pointer();
        let (progress, confirmed) = hold_update_for(
            state,
            pressing,
            pointer_down,
            focused,
            Instant::now(),
            PERMANENT_HOLD_DURATION,
        );
        paint_hold_bar(ui, &slots, &response, progress, progress_bg);
        if pressing {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
        let hover = match &blocked {
            Some(reason) => format!("Indisponível nesta fronteira: {reason}"),
            None if !armed => "Disponível em um instante: um botão permanente não aceita um clique que já estava em curso quando a janela abriu.".to_string(),
            None => format!(
                "Acrescenta a regra {rule:?} ao vetor `{vector}` da {}. Vale até alguém editar a política; nenhum grant temporário é criado.{}",
                self.permanent.scope,
                if kind == PermanentKind::Accept {
                    " Uma regra literal casa por prefixo: os argumentos depois da fronteira ficam livres."
                } else {
                    " Denies explícitos prevalecem sobre accept, grant e aprovação humana."
                }
            ),
        };
        response.on_hover_text(hover);
        if confirmed {
            self.finish_decision(
                ctx,
                match kind {
                    PermanentKind::Accept => AccessChoice::AllowAlways {
                        prefix_len: self.prefix_len,
                    },
                    PermanentKind::Deny => AccessChoice::DenyAlways {
                        prefix_len: self.prefix_len,
                    },
                },
            );
        }
    }

    /// The rule of a boundary, quoted, for the terminal status line.
    fn permanent_rule(&self, prefix_len: usize) -> String {
        self.permanent.at(prefix_len).map_or_else(
            || "a regra".to_string(),
            |option| format!("{:?}", option.rule),
        )
    }

    /// The line inside the fixed-prefix box that says, in words, what a permanent
    /// decision at this boundary writes and where — or why it cannot be written.
    fn render_permanent_footer(&mut self, ui: &mut egui::Ui) {
        let Some(option) = self.permanent.at(self.prefix_len) else {
            return;
        };
        if let Some(reason) = option.shared_block() {
            ui.small(
                egui::RichText::new(format!("Decisão permanente indisponível: {reason}"))
                    .color(BOUNDARY_ACCENT),
            );
            return;
        }
        ui.small(format!(
            "Permanente grava {:?} na {}.",
            option.rule, self.permanent.scope
        ));
        for reason in [&option.accept_blocked, &option.deny_blocked]
            .into_iter()
            .flatten()
        {
            ui.small(egui::RichText::new(reason.clone()).color(BOUNDARY_ACCENT));
        }
    }

    fn render_argument_strip(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            for index in 0..self.args.len() {
                self.render_token(
                    ui,
                    index,
                    ui.visuals().faint_bg_color,
                    ui.visuals().widgets.inactive.bg_stroke,
                    ui.visuals().text_color(),
                    false,
                );
            }
        });
    }

    fn render_prefix_editor(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, decided: bool) {
        let fixed_frame = egui::Frame::group(ui.style())
            .fill(FIXED_GROUP_BG)
            .stroke(egui::Stroke::new(1.0_f32, FIXED_STROKE))
            .inner_margin(egui::Margin::same(8.0));
        fixed_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("PERMANECEM IGUAIS · {}", self.prefix_len))
                        .small()
                        .strong()
                        .color(FIXED_STROKE),
                );
                // The permanent buttons live here, in the corner of the box that
                // shows the tokens the rule is made of: the boundary is tuned
                // first, and only then made permanent.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.render_permanent_button(ui, ctx, PermanentKind::Deny, decided);
                    self.render_permanent_button(ui, ctx, PermanentKind::Accept, decided);
                });
            });
            ui.horizontal_wrapped(|ui| {
                for index in 0..self.prefix_len {
                    self.render_token(
                        ui,
                        index,
                        FIXED_PILL_BG,
                        egui::Stroke::new(1.0_f32, FIXED_STROKE),
                        FIXED_TEXT,
                        true,
                    );
                }
            });
            self.render_permanent_footer(ui);
        });

        ui.add_space(6.0);
        let free_frame = egui::Frame::group(ui.style())
            .fill(FREE_GROUP_BG)
            .stroke(egui::Stroke::new(1.0_f32, FREE_STROKE))
            .inner_margin(egui::Margin::same(8.0));
        free_frame.show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(
                egui::RichText::new(format!(
                    "PODEM MUDAR OU DESAPARECER · {}",
                    self.args.len().saturating_sub(self.prefix_len)
                ))
                .small()
                .strong()
                .color(FREE_TEXT),
            );
            ui.horizontal_wrapped(|ui| {
                for index in self.prefix_len..self.args.len() {
                    self.render_token(
                        ui,
                        index,
                        FREE_PILL_BG,
                        egui::Stroke::new(1.0_f32, FREE_STROKE),
                        FREE_TEXT,
                        true,
                    );
                }
                ui.add(
                    egui::Button::new(
                        egui::RichText::new("+ zero ou mais argumentos futuros")
                            .small()
                            .color(BOUNDARY_ACCENT),
                    )
                    .fill(FREE_GROUP_BG)
                    .stroke(egui::Stroke::new(1.5_f32, BOUNDARY_ACCENT))
                    .sense(egui::Sense::hover()),
                )
                .on_hover_text(
                    "Um grant por prefixo também aceita novos argumentos depois dos atuais.",
                );
            });
        });
    }

    fn render_token(
        &mut self,
        ui: &mut egui::Ui,
        index: usize,
        fill: egui::Color32,
        stroke: egui::Stroke,
        text: egui::Color32,
        selectable: bool,
    ) {
        let label = compact_token_label(&self.args[index], index);
        let button = egui::Button::new(egui::RichText::new(label).monospace().color(text))
            .fill(fill)
            .stroke(stroke)
            .sense(if selectable {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            });
        let response = ui.add(button).on_hover_ui(|ui| {
            ui.monospace(bounded_token_preview(
                &self.args[index],
                TOKEN_TOOLTIP_CHARS,
            ));
            if self.arg_char_counts[index] > TOKEN_TOOLTIP_CHARS {
                ui.small(format!(
                    "Argumento {} · {} · visualização abreviada",
                    index + 1,
                    format_byte_count(self.args[index].len())
                ));
            }
        });
        if selectable && response.clicked() {
            self.prefix_len = index + 1;
            self.reset_holds();
        }
    }

    fn render_argument_details(&mut self, ui: &mut egui::Ui) {
        let Some(index) = self.details_arg else {
            return;
        };
        let char_count = self.arg_char_counts[index];
        let page_count = char_count.div_ceil(TOKEN_DETAIL_PAGE_CHARS).max(1);
        self.details_page = self.details_page.min(page_count - 1);
        let detail = token_detail_page(&self.args[index], self.details_page);
        let long_position = self
            .long_args
            .iter()
            .position(|candidate| *candidate == index)
            .unwrap_or(0);

        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "Argumento {} de {} · {}",
                        index + 1,
                        self.args.len(),
                        format_byte_count(self.args[index].len())
                    ))
                    .strong(),
                );
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui
                            .add_enabled(
                                long_position + 1 < self.long_args.len(),
                                egui::Button::new("Próximo argumento"),
                            )
                            .clicked()
                        {
                            self.details_arg = Some(self.long_args[long_position + 1]);
                            self.details_page = 0;
                        }
                        if ui
                            .add_enabled(
                                long_position > 0,
                                egui::Button::new("Argumento anterior"),
                            )
                            .clicked()
                        {
                            self.details_arg = Some(self.long_args[long_position - 1]);
                            self.details_page = 0;
                        }
                    },
                );
            });
            egui::ScrollArea::vertical()
                .id_salt("access_argument_details")
                .max_height(88.0)
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(detail).monospace()).wrap(),
                    );
                });
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(self.details_page > 0, egui::Button::new("◀ Página"))
                    .clicked()
                {
                    self.details_page -= 1;
                }
                ui.label(format!(
                    "Página {} de {} · até {} caracteres por página",
                    self.details_page + 1,
                    page_count,
                    TOKEN_DETAIL_PAGE_CHARS
                ));
                if ui
                    .add_enabled(
                        self.details_page + 1 < page_count,
                        egui::Button::new("Página ▶"),
                    )
                    .clicked()
                {
                    self.details_page += 1;
                }
            });
            ui.small(
                "A visualização é paginada; o matcher e a execução usam o argumento completo, sem truncamento.",
            );
        });
    }
}

fn access_window(
    provider: String,
    args: Vec<String>,
    default_minutes: u32,
    align_seconds: Option<u32>,
    permanent: PermanentPolicy,
) -> std::result::Result<AccessChoice, String> {
    let suggested_prefix_len = suggested_prefix_len(&args);
    let arg_char_counts = args
        .iter()
        .map(|argument| argument.chars().count())
        .collect::<Vec<_>>();
    let long_args = arg_char_counts
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count > TOKEN_COMPACT_CHARS).then_some(index))
        .collect::<Vec<_>>();
    let outcome = Rc::new(RefCell::new(None));
    let result = Rc::clone(&outcome);
    eframe::run_native(
        "Torii — autorização",
        native_options(ACCESS_WIDTH, ACCESS_ONCE_HEIGHT),
        Box::new(move |_| {
            Ok(Box::new(AccessApp {
                provider,
                prefix_len: args.len().max(1),
                args,
                permanent,
                opened_at: Instant::now(),
                accept_hold: HoldState::default(),
                deny_hold: HoldState::default(),
                hold: HoldState::default(),
                timed: false,
                prefix: false,
                suggested_prefix_len,
                seconds: default_minutes.max(1) * 60,
                align_seconds,
                arg_char_counts,
                long_args,
                details_arg: None,
                details_page: 0,
                requested_height: ACCESS_ONCE_HEIGHT,
                pending_height: None,
                measured_height: None,
                decision_since: None,
                outcome: result,
            }))
        }),
    )
    .map_err(|error| error.to_string())?;
    let value = outcome.borrow().unwrap_or(AccessChoice::Deny);
    Ok(value)
}

const TARGET_ACCESS_WIDTH: f32 = 720.0;
const TARGET_ACCESS_MIN_HEIGHT: f32 = 358.0;
const TARGET_ACCESS_MAX_HEIGHT: f32 = 620.0;
const TARGET_ACCESS_ACTIVE_ROW_HEIGHT: f32 = 30.0;
const TARGET_ACCESS_WARNING_HEIGHT: f32 = 82.0;
const TARGET_ACCESS_ACTIONS_HEIGHT: f32 = 38.0;
const HOLD_DURATION: Duration = Duration::from_secs(1);
/// A permanent policy decision is held far longer than a one-off approval, on
/// purpose: the seconds exist so the human can ask "is this really something I
/// want in the policy forever?" while the bar fills.
const PERMANENT_HOLD_DURATION: Duration = Duration::from_secs(5);
/// The permanent buttons stay inert for a moment after the window appears. A
/// prompt is raised by an agent's call and can land under a click already on its
/// way; the least reversible action in the window does not accept that click.
const PERMANENT_ARMING_DELAY: Duration = Duration::from_millis(1500);
const PERMANENT_BUTTON_WIDTH: f32 = 178.0;
const TARGET_ADD_BUTTON_WIDTH: f32 = 214.0;
const ALLOW_HOLD_BUTTON_WIDTH: f32 = 232.0;
const TARGET_WARNING_BG: egui::Color32 = egui::Color32::from_rgb(63, 31, 31);
const TARGET_WARNING_STROKE: egui::Color32 = egui::Color32::from_rgb(224, 108, 117);
const HOLD_PROGRESS_BG: egui::Color32 = egui::Color32::from_rgb(77, 105, 58);

/// Which of the two permanent buttons is being rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PermanentKind {
    Accept,
    Deny,
}

#[derive(Default)]
struct HoldState {
    started_at: Option<Instant>,
    blocked_until_release: bool,
    was_focused: bool,
}

struct TargetAccessApp {
    provider: String,
    requested_target: String,
    requested_binding: String,
    active_targets: Vec<ActiveTargetAuthorization>,
    minutes: u32,
    add_hold: HoldState,
    decision_since: Option<Instant>,
    outcome: Rc<RefCell<Option<TargetAccessChoice>>>,
}

impl eframe::App for TargetAccessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let decision = *self.outcome.borrow();
        if let Some(decision_since) = self.decision_since {
            let elapsed = decision_since.elapsed();
            if elapsed >= PROMPT_TERMINAL_VISIBLE_FOR {
                close(ctx);
            } else {
                ctx.request_repaint_after(PROMPT_TERMINAL_VISIBLE_FOR - elapsed);
            }
        }
        let decided = decision.is_some();
        let now = epoch_seconds();
        let active_after_add =
            active_target_count_after_add(&self.active_targets, &self.requested_target, now);
        let add_creates_multiple = active_after_add > 1;

        egui::TopBottomPanel::bottom("target_access_status")
            .resizable(false)
            .exact_height(PROMPT_STATUS_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::symmetric(6.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    match decision {
                        Some(TargetAccessChoice::Deny) => {
                            ui.label(
                                egui::RichText::new("Target negado.")
                                    .color(PROMPT_ERROR_COLOR),
                            );
                        }
                        Some(TargetAccessChoice::Replace { minutes }) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "👍 Targets anteriores desativados; novo target autorizado por {minutes} min."
                                ))
                                .color(PROMPT_SUCCESS_COLOR),
                            );
                        }
                        Some(TargetAccessChoice::Add { minutes }) => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "👍 Target adicionado por {minutes} min; os anteriores continuam ativos."
                                ))
                                .color(PROMPT_SUCCESS_COLOR),
                            );
                        }
                        None => {
                            ui.label("Revise os targets ativos antes de decidir.");
                        }
                    }
                });
            });

        egui::TopBottomPanel::bottom("target_access_actions")
            .resizable(false)
            .exact_height(TARGET_ACCESS_ACTIONS_HEIGHT)
            .show_separator_line(true)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // With nothing active, "replace" and "add" collapse into a
                    // single "authorize" action, so the Add button is hidden and
                    // the remaining button is relabelled.
                    let no_active = self.active_targets.is_empty();
                    ui.add_enabled_ui(!decided, |ui| {
                        if ui.button("Negar").clicked() {
                            self.finish_decision(ctx, TargetAccessChoice::Deny);
                        }
                    });
                    if !no_active {
                        if add_creates_multiple {
                            let label = "Segure para adicionar";
                            let slots = reserve_hold_paint(ui);
                            let response = ui
                                .add_enabled(
                                    !decided,
                                    egui::Button::new(label)
                                        .fill(egui::Color32::TRANSPARENT)
                                        .min_size(egui::vec2(
                                            TARGET_ADD_BUTTON_WIDTH,
                                            ui.spacing().interact_size.y,
                                        ))
                                        .sense(egui::Sense::click_and_drag()),
                                )
                                .on_hover_text(
                                    "Mantenha pressionado por 1 segundo para preservar os targets atuais e autorizar também o solicitado.",
                                );
                            let (pointer_down, focused) =
                                ctx.input(|input| (input.pointer.primary_down(), input.focused));
                            let pressing = !decided
                                && response.is_pointer_button_down_on()
                                && response.contains_pointer();
                            let (progress, confirmed) = hold_update(
                                &mut self.add_hold,
                                pressing,
                                pointer_down,
                                focused,
                                Instant::now(),
                            );
                            paint_hold_bar(ui, &slots, &response, progress, HOLD_PROGRESS_BG);
                            if pressing {
                                ctx.request_repaint_after(Duration::from_millis(16));
                            }
                            if confirmed {
                                self.finish_decision(
                                    ctx,
                                    TargetAccessChoice::Add {
                                        minutes: self.minutes,
                                    },
                                );
                            }
                        } else {
                            self.add_hold = HoldState::default();
                            if ui
                                .add_enabled(
                                    !decided,
                                    egui::Button::new(format!(
                                        "Adicionar por {} min",
                                        self.minutes
                                    )),
                                )
                                .on_hover_text(
                                    "Autoriza o target solicitado sem desativar autorizações existentes.",
                                )
                                .clicked()
                            {
                                self.finish_decision(
                                    ctx,
                                    TargetAccessChoice::Add {
                                        minutes: self.minutes,
                                    },
                                );
                            }
                        }
                    } else {
                        self.add_hold = HoldState::default();
                    }
                    ui.add_enabled_ui(!decided, |ui| {
                        let (label, hover) = if no_active {
                            (
                                format!("Autorizar por {} min", self.minutes),
                                "Autoriza o target solicitado.",
                            )
                        } else {
                            (
                                format!("Substituir por {} min", self.minutes),
                                "Desativa todos os targets atuais deste provider e autoriza somente o solicitado.",
                            )
                        };
                        if ui.button(label).on_hover_text(hover).clicked() {
                            self.finish_decision(
                                ctx,
                                TargetAccessChoice::Replace {
                                    minutes: self.minutes,
                                },
                            );
                        }
                    });
                });
            });

        if add_creates_multiple {
            egui::TopBottomPanel::bottom("target_access_warning")
                .resizable(false)
                .exact_height(TARGET_ACCESS_WARNING_HEIGHT)
                .show_separator_line(false)
                .frame(
                    egui::Frame::none()
                        .fill(ctx.style().visuals.panel_fill)
                        .inner_margin(egui::Margin::symmetric(8.0, 4.0)),
                )
                .show(ctx, |ui| {
                    let content_width = (ui.available_width() - 20.0).max(0.0);
                    egui::Frame::none()
                        .fill(TARGET_WARNING_BG)
                        .stroke(egui::Stroke::new(1.0_f32, TARGET_WARNING_STROKE))
                        .inner_margin(egui::Margin::same(10.0))
                        .show(ui, |ui| {
                            ui.set_min_width(content_width);
                            ui.label(
                                egui::RichText::new(format!(
                                    "ATENÇÃO: adicionar deixará {active_after_add} targets autorizados ao mesmo tempo."
                                ))
                                .strong()
                                .color(PROMPT_ERROR_COLOR),
                            );
                            ui.label(
                                "O agente poderá escolher qualquer um deles durante a validade de cada autorização.",
                            );
                            ui.label(
                                egui::RichText::new(
                                    "Para manter os atuais, segure o botão Adicionar por 1 segundo.",
                                )
                                .strong(),
                            );
                        });
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            ui.heading(format!(
                "Torii — autorização de target ({})",
                self.provider
            ));
            ui.label("O agente solicitou acesso temporário a outro target.");
            ui.group(|ui| {
                ui.set_min_width(ui.available_width());
                ui.label(egui::RichText::new("Target solicitado").strong());
                ui.label(
                    egui::RichText::new(&self.requested_target)
                        .monospace()
                        .strong()
                        .color(BOUNDARY_ACCENT),
                );
                ui.label(egui::RichText::new("Binding humano").small().strong());
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(&self.requested_binding).monospace(),
                    )
                    .wrap(),
                );
                ui.horizontal(|ui| {
                    ui.label("Duração da autorização:");
                    ui.add(egui::DragValue::new(&mut self.minutes).range(1..=1440));
                    ui.label("minutos");
                });
            });

            ui.label(egui::RichText::new("Targets autorizados agora").strong());
            if self.active_targets.is_empty() {
                ui.label("Nenhum target autorizado.");
            } else {
                ui.group(|ui| {
                    ui.set_min_width(ui.available_width());
                    egui::ScrollArea::vertical()
                        .id_salt("active_target_authorizations")
                        .max_height(180.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            for (index, active) in self.active_targets.iter().enumerate() {
                                if index > 0 {
                                    ui.separator();
                                }
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(&active.target).monospace().strong(),
                                    );
                                    ui.label("·");
                                    ui.label(target_expiration_label(
                                        active.expires_at_epoch,
                                        now,
                                    ));
                                    ui.add_space(8.0);
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&active.display_binding)
                                                .monospace()
                                                .small(),
                                        )
                                        .wrap(),
                                    );
                                });
                            }
                        });
                });
            }

            ui.separator();
            ui.small(if self.active_targets.is_empty() {
                "Autorizar concede um lease temporário a este target. A política do Jasper e os denies explícitos continuam valendo em cada target."
            } else {
                "Substituir remove as autorizações atuais deste provider. Adicionar as preserva. A política do Jasper e os denies explícitos continuam valendo em cada target."
            });
        });
    }
}

impl TargetAccessApp {
    fn finish_decision(&mut self, ctx: &egui::Context, decision: TargetAccessChoice) {
        self.add_hold = HoldState::default();
        *self.outcome.borrow_mut() = Some(decision);
        self.decision_since = Some(Instant::now());
        ctx.request_repaint_after(PROMPT_TERMINAL_VISIBLE_FOR);
    }
}

fn target_access_window(
    provider: String,
    requested_target: String,
    requested_binding: String,
    active_targets: Vec<ActiveTargetAuthorization>,
    default_minutes: u32,
) -> TargetAccessChoice {
    let height = target_access_window_height(active_targets.len());
    let outcome = Rc::new(RefCell::new(None));
    let result = Rc::clone(&outcome);
    let _ = eframe::run_native(
        "Torii — autorização de target",
        native_options(TARGET_ACCESS_WIDTH, height),
        Box::new(move |_| {
            Ok(Box::new(TargetAccessApp {
                provider,
                requested_target,
                requested_binding,
                active_targets,
                minutes: default_minutes.clamp(1, 1440),
                add_hold: HoldState::default(),
                decision_since: None,
                outcome: result,
            }))
        }),
    );
    let choice = outcome.borrow().unwrap_or(TargetAccessChoice::Deny);
    choice
}

fn target_access_window_height(active_count: usize) -> f32 {
    let warning_height = if active_count == 0 {
        0.0
    } else {
        TARGET_ACCESS_WARNING_HEIGHT
    };
    (TARGET_ACCESS_MIN_HEIGHT
        + active_count.min(6) as f32 * TARGET_ACCESS_ACTIVE_ROW_HEIGHT
        + warning_height)
        .min(TARGET_ACCESS_MAX_HEIGHT)
}

fn hold_update(
    state: &mut HoldState,
    pressing_button: bool,
    pointer_down: bool,
    focused: bool,
    now: Instant,
) -> (f32, bool) {
    hold_update_for(
        state,
        pressing_button,
        pointer_down,
        focused,
        now,
        HOLD_DURATION,
    )
}

/// The same gesture with an explicit duration. A permanent policy decision uses a
/// deliberately long hold: the seconds are the deliberation, not a formality.
fn hold_update_for(
    state: &mut HoldState,
    pressing_button: bool,
    pointer_down: bool,
    focused: bool,
    now: Instant,
    duration: Duration,
) -> (f32, bool) {
    let lost_focus = state.was_focused && !focused;
    state.was_focused = focused;
    if lost_focus && state.started_at.is_some() {
        state.started_at = None;
        state.blocked_until_release = true;
        return (0.0, false);
    }
    if !pointer_down {
        state.started_at = None;
        state.blocked_until_release = false;
        return (0.0, false);
    }
    if state.blocked_until_release {
        return (0.0, false);
    }
    if !pressing_button {
        if state.started_at.take().is_some() {
            state.blocked_until_release = true;
        }
        return (0.0, false);
    }
    let started_at = *state.started_at.get_or_insert(now);
    let progress = (now.saturating_duration_since(started_at).as_secs_f32()
        / duration.as_secs_f32())
    .clamp(0.0, 1.0);
    if progress >= 1.0 {
        state.started_at = None;
        state.blocked_until_release = true;
        (1.0, true)
    } else {
        (progress, false)
    }
}

/// Paint slots reserved before a hold button is added, so the progress bar lands
/// *under* the label the button draws itself. Painting it afterwards would cover
/// the label and force a second rendering of the same text on top — which reads
/// as two labels stacked and tints the one underneath.
struct HoldPaint {
    background: egui::layers::ShapeIdx,
    bar: egui::layers::ShapeIdx,
}

/// Reserve the two slots. Must be called before the button is added: shapes are
/// drawn in the order they enter the layer, and `set` fills a slot in place
/// without moving it.
fn reserve_hold_paint(ui: &egui::Ui) -> HoldPaint {
    HoldPaint {
        background: ui.painter().add(egui::Shape::Noop),
        bar: ui.painter().add(egui::Shape::Noop),
    }
}

/// A hold button is added with no fill of its own, so these two slots are its
/// whole background: the normal widget fill, then the progress bar over it. The
/// label egui rendered is never touched.
fn paint_hold_bar(
    ui: &egui::Ui,
    slots: &HoldPaint,
    response: &egui::Response,
    progress: f32,
    fill_color: egui::Color32,
) {
    let visuals = ui.style().interact(response);
    ui.painter().set(
        slots.background,
        egui::epaint::RectShape::filled(response.rect, visuals.rounding, visuals.weak_bg_fill),
    );
    if progress <= 0.0 {
        return;
    }
    let track = response.rect.shrink(2.0);
    let bar = egui::Rect::from_min_max(
        track.min,
        egui::pos2(track.left() + track.width() * progress, track.bottom()),
    );
    ui.painter().set(
        slots.bar,
        egui::epaint::RectShape::filled(bar, egui::Rounding::ZERO, fill_color),
    );
}

/// How long the window lingers after a decision, so the human reads what it did.
fn terminal_visible_for(decision: AccessChoice) -> Duration {
    match decision {
        AccessChoice::AllowAlways { .. } | AccessChoice::DenyAlways { .. } => {
            PERMANENT_TERMINAL_VISIBLE_FOR
        }
        _ => PROMPT_TERMINAL_VISIBLE_FOR,
    }
}

fn epoch_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn active_target_count_after_add(
    active_targets: &[ActiveTargetAuthorization],
    requested_target: &str,
    now: u64,
) -> usize {
    let mut targets = active_targets
        .iter()
        .filter(|active| active.expires_at_epoch > now)
        .map(|active| active.target.as_str())
        .collect::<Vec<_>>();
    targets.push(requested_target);
    targets.sort_unstable();
    targets.dedup();
    targets.len()
}

fn target_expiration_label(expires_at_epoch: u64, now: u64) -> String {
    let remaining = expires_at_epoch.saturating_sub(now);
    if remaining == 0 {
        return "expirada".into();
    }
    let minutes = remaining.div_ceil(60);
    if minutes < 60 {
        return format!("expira em {minutes} min");
    }
    let hours = minutes / 60;
    let extra_minutes = minutes % 60;
    if extra_minutes == 0 {
        format!("expira em {hours} h")
    } else {
        format!("expira em {hours} h {extra_minutes} min")
    }
}

struct AuthApp {
    provider: String,
    fields: Vec<AuthField>,
    values: HashMap<String, String>,
    error: Option<String>,
    pending: Option<mpsc::Receiver<(AuthValidationOutcome, HashMap<String, String>)>>,
    success_since: Option<Instant>,
    validation: AuthValidation,
    form_height: f32,
    outcome: Rc<RefCell<Option<HashMap<String, String>>>>,
    invalid_attempts: Rc<RefCell<u32>>,
}

enum AuthValidationOutcome {
    Accepted,
    Rejected,
    Failed,
}

const AUTH_MULTILINE_INPUT_HEIGHT: f32 = 72.0;
const PROMPT_ACTIONS_HEIGHT: f32 = 32.0;
const PROMPT_STATUS_BAR_HEIGHT: f32 = 24.0;
const PROMPT_ERROR_COLOR: egui::Color32 = egui::Color32::from_rgb(224, 108, 117);
const PROMPT_SUCCESS_COLOR: egui::Color32 = egui::Color32::from_rgb(152, 195, 121);
const PROMPT_TERMINAL_VISIBLE_FOR: Duration = Duration::from_millis(650);
const PERMANENT_TERMINAL_VISIBLE_FOR: Duration = Duration::from_millis(2200);

impl eframe::App for AuthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_validation(ctx);
        let validating = self.pending.is_some();
        let succeeded = self.success_since.is_some();
        let busy = validating || succeeded;
        if validating {
            ctx.set_cursor_icon(egui::CursorIcon::Wait);
        }

        egui::TopBottomPanel::bottom("auth_status")
            .resizable(false)
            .exact_height(PROMPT_STATUS_BAR_HEIGHT)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.extreme_bg_color)
                    .inner_margin(egui::Margin::symmetric(6.0, 0.0)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if validating {
                        ui.add(egui::Spinner::new().size(16.0));
                        ui.label("Validando sessão…");
                    } else if succeeded {
                        ui.label(
                            egui::RichText::new("👍 Sessão validada.").color(PROMPT_SUCCESS_COLOR),
                        );
                    } else if let Some(error) = &self.error {
                        ui.add(
                            egui::Label::new(egui::RichText::new(error).color(PROMPT_ERROR_COLOR))
                                .truncate(),
                        )
                        .on_hover_text(error);
                    } else {
                        ui.label("Pronto para validar.");
                    }
                });
            });

        egui::TopBottomPanel::bottom("auth_actions")
            .resizable(false)
            .exact_height(PROMPT_ACTIONS_HEIGHT)
            .show_separator_line(true)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        if ui.button("Validar e usar").clicked() {
                            self.start_validation(ctx);
                        }
                    });
                    ui.add_enabled_ui(!succeeded, |ui| {
                        if ui.button("Cancelar").clicked() {
                            close(ctx);
                        }
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("Torii — autenticação ({})", self.provider));
            ui.label("A nova sessão só substituirá a anterior após validação.");
            ui.separator();
            ui.add_enabled_ui(!busy, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(self.form_height)
                    .show(ui, |ui| {
                        for field in &self.fields {
                            let label = if field.label.is_empty() {
                                &field.name
                            } else {
                                &field.label
                            };
                            ui.label(label);
                            let value = self.values.entry(field.name.clone()).or_default();
                            let response = if field.multiline {
                                egui::ScrollArea::vertical()
                                    .id_salt(("auth_multiline", field.name.as_str()))
                                    .max_height(AUTH_MULTILINE_INPUT_HEIGHT)
                                    .min_scrolled_height(AUTH_MULTILINE_INPUT_HEIGHT)
                                    .scroll_bar_visibility(
                                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded,
                                    )
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::TextEdit::multiline(value)
                                                .password(field.secret)
                                                .desired_rows(4)
                                                .desired_width(f32::INFINITY),
                                        )
                                    })
                                    .inner
                            } else {
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .password(field.secret)
                                        .desired_width(f32::INFINITY),
                                )
                            };
                            if response.changed() {
                                self.error = None;
                            }
                        }
                    });
                ui.add_space(1.0);
                if ui.button("Colar atribuições do clipboard").clicked() {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        if let Ok(text) = clipboard.get_text() {
                            let allowed: Vec<String> =
                                self.fields.iter().map(|field| field.name.clone()).collect();
                            if let Ok(values) =
                                crate::config::env_file::parse_allowed(&text, &allowed)
                            {
                                self.values.extend(values);
                            }
                        }
                    }
                    self.error = None;
                }
            });
        });
    }
}

impl AuthApp {
    fn start_validation(&mut self, ctx: &egui::Context) {
        let missing: Vec<&str> = self
            .fields
            .iter()
            .filter(|field| {
                field.required
                    && self
                        .values
                        .get(&field.name)
                        .is_none_or(|value| value.trim().is_empty())
            })
            .map(|field| field.name.as_str())
            .collect();
        if !missing.is_empty() {
            self.error = Some(format!("Campos obrigatórios: {}", missing.join(", ")));
            return;
        }

        self.error = None;
        let fields = self.values.clone();
        let validation = self.validation.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let auth_env = crate::runtime::exec::interpolate_environment(
                &validation.environment_templates,
                &fields,
            );
            let outcome = match validation.command {
                None => AuthValidationOutcome::Accepted,
                Some(command) => match tokio::runtime::Runtime::new() {
                    Ok(runtime) => match runtime.block_on(crate::runtime::exec::validate_command(
                        &command,
                        &validation.args,
                        &validation.persistent_env,
                        &auth_env,
                    )) {
                        Ok(true) => AuthValidationOutcome::Accepted,
                        Ok(false) => AuthValidationOutcome::Rejected,
                        Err(_) => AuthValidationOutcome::Failed,
                    },
                    Err(_) => AuthValidationOutcome::Failed,
                },
            };
            let _ = tx.send((outcome, fields));
        });
        self.pending = Some(rx);
        ctx.request_repaint();
    }

    fn poll_validation(&mut self, ctx: &egui::Context) {
        if let Some(success_since) = self.success_since {
            let elapsed = success_since.elapsed();
            if elapsed >= PROMPT_TERMINAL_VISIBLE_FOR {
                self.success_since = None;
                close(ctx);
            } else {
                ctx.request_repaint_after(PROMPT_TERMINAL_VISIBLE_FOR - elapsed);
            }
            return;
        }

        let Some(rx) = &self.pending else { return };
        match rx.try_recv() {
            Ok((AuthValidationOutcome::Accepted, fields)) => {
                self.pending = None;
                *self.outcome.borrow_mut() = Some(fields);
                self.success_since = Some(Instant::now());
                self.error = None;
                ctx.request_repaint_after(PROMPT_TERMINAL_VISIBLE_FOR);
            }
            Ok((AuthValidationOutcome::Rejected, _)) => {
                self.pending = None;
                *self.invalid_attempts.borrow_mut() += 1;
                self.error = Some("Sessão recusada. Revise os dados e tente novamente.".into());
            }
            Ok((AuthValidationOutcome::Failed, _)) => {
                self.pending = None;
                self.error = Some("Não foi possível executar a validação.".into());
            }
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.error = Some("A validação foi interrompida.".into());
            }
        }
    }
}

fn auth_form_height(fields: &[AuthField]) -> f32 {
    fields
        .iter()
        .map(|field| if field.multiline { 92.0 } else { 38.0 })
        .sum::<f32>()
        .clamp(60.0, 280.0)
}

fn auth_window_height(fields: &[AuthField]) -> f32 {
    (140.0 + auth_form_height(fields)).clamp(240.0, 460.0)
}

fn auth_window(
    provider: String,
    fields: Vec<AuthField>,
    error: Option<String>,
    validation: AuthValidation,
) -> AuthPromptResult {
    let height = auth_window_height(&fields);
    let form_height = auth_form_height(&fields);
    let outcome = Rc::new(RefCell::new(None));
    let result = Rc::clone(&outcome);
    let invalid_attempts = Rc::new(RefCell::new(0));
    let app_invalid_attempts = Rc::clone(&invalid_attempts);
    let _ = eframe::run_native(
        "Torii — autenticação",
        native_options(620.0, height),
        Box::new(move |_| {
            Ok(Box::new(AuthApp {
                provider,
                fields,
                values: HashMap::new(),
                error,
                pending: None,
                success_since: None,
                validation,
                form_height,
                outcome: result,
                invalid_attempts: app_invalid_attempts,
            }))
        }),
    );
    let fields = outcome.borrow().clone();
    let invalid_attempts = *invalid_attempts.borrow();
    AuthPromptResult {
        fields,
        invalid_attempts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An authorization app in the flow where the bug bit: temporary access, so
    /// the duration control sits at the bottom of the body.
    fn access_app(args: Vec<String>) -> AccessApp {
        let arg_char_counts = args
            .iter()
            .map(|argument| argument.chars().count())
            .collect::<Vec<_>>();
        let long_args = arg_char_counts
            .iter()
            .enumerate()
            .filter_map(|(index, count)| (*count > TOKEN_COMPACT_CHARS).then_some(index))
            .collect::<Vec<_>>();
        let args_len = args.len();
        AccessApp {
            provider: "aws".into(),
            prefix_len: args_len.max(1),
            args,
            permanent: PermanentPolicy::blocked("test scope", "test", args_len),
            opened_at: Instant::now(),
            accept_hold: HoldState::default(),
            deny_hold: HoldState::default(),
            hold: HoldState::default(),
            timed: true,
            prefix: false,
            suggested_prefix_len: None,
            seconds: 15 * 60,
            align_seconds: Some(270),
            arg_char_counts,
            long_args,
            details_arg: None,
            details_page: 0,
            requested_height: ACCESS_ONCE_HEIGHT,
            pending_height: None,
            measured_height: None,
            decision_since: None,
            outcome: Rc::new(RefCell::new(None)),
        }
    }

    /// Lay the body out headless at the real window width and report what it
    /// measured, the same number the window height follows.
    fn measured_body_height(args: Vec<String>) -> f32 {
        let ctx = egui::Context::default();
        let mut app = access_app(args);
        let mut measured = 0.0;
        // The first frame loads fonts; the second one measures a settled layout.
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(ACCESS_WIDTH, ACCESS_ONCE_HEIGHT),
                )),
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let body = egui::ScrollArea::vertical()
                        .id_salt("access_body")
                        .auto_shrink([false, false])
                        .show(ui, |ui| app.render_body(ui, ctx, false));
                    measured = body.content_size.y;
                });
            });
        }
        measured
    }

    fn field(multiline: bool) -> AuthField {
        AuthField {
            name: "FIELD".into(),
            label: String::new(),
            secret: false,
            required: true,
            multiline,
        }
    }

    #[test]
    fn authentication_window_height_tracks_form_content_and_is_bounded() {
        let compact = auth_window_height(&[field(false)]);
        let aws_form = auth_window_height(&[field(false), field(false), field(true)]);
        let large = auth_window_height(&vec![field(true); 12]);

        assert_eq!(compact, 240.0);
        assert!(aws_form < 340.0);
        assert!(aws_form > compact);
        assert_eq!(large, 420.0);
    }

    #[test]
    fn access_tokens_are_rendered_without_shell_ambiguity() {
        assert_eq!(display_token("pods -A", usize::MAX), "\"pods -A\"");
        assert_eq!(display_token("", usize::MAX), "\"\"");
        assert_eq!(display_token("a\nb", usize::MAX), "\"a\\nb\"");
    }

    #[test]
    fn duration_is_presented_with_or_without_leftover_seconds() {
        assert_eq!(spelled_duration(300), "5 min");
        assert_eq!(spelled_duration(270), "4 min 30 s");
        assert_eq!(spelled_duration(45), "45 s");
        assert_eq!(compact_duration(300), "5");
        assert_eq!(compact_duration(270), "4:30");
        assert_eq!(compact_duration(3605), "60:05");
    }

    #[test]
    fn access_window_height_expands_only_with_the_selected_flow() {
        assert_eq!(access_height(false, false, false), ACCESS_ONCE_HEIGHT);
        assert_eq!(access_height(true, false, false), ACCESS_TIMED_EXACT_HEIGHT);
        assert_eq!(access_height(true, true, false), ACCESS_TIMED_PREFIX_HEIGHT);
        assert_eq!(access_height(false, false, true), 450.0);
        assert_eq!(access_height(true, true, true), ACCESS_MAX_HEIGHT);
    }

    /// Argument pills wrap into an unpredictable number of lines, so the flow
    /// height is a floor and never a ceiling: a taller body must grow the window
    /// instead of pushing the duration control under the bottom panels.
    #[test]
    fn access_window_height_grows_with_the_measured_body() {
        assert_eq!(
            access_window_height(ACCESS_ONCE_HEIGHT, None),
            ACCESS_ONCE_HEIGHT
        );
        assert_eq!(
            access_window_height(ACCESS_ONCE_HEIGHT, Some(200.0)),
            ACCESS_ONCE_HEIGHT,
            "a short body must not shrink the window below its flow"
        );
        assert_eq!(
            access_window_height(ACCESS_ONCE_HEIGHT, Some(ACCESS_ONCE_HEIGHT + 70.0)),
            ACCESS_ONCE_HEIGHT + 70.0
        );
        assert_eq!(
            access_window_height(ACCESS_TIMED_PREFIX_HEIGHT, Some(2000.0)),
            ACCESS_MAX_HEIGHT
        );
    }

    /// The floor-and-cap logic only helps if the measurement itself reacts to
    /// wrapped pills, so measure a real body: a long invocation must come out
    /// taller than a short one at the same window width.
    #[test]
    fn a_long_invocation_measures_a_taller_body() {
        let short = measured_body_height(vec!["s3".into(), "ls".into()]);
        let wrapped = measured_body_height(
            (0..24)
                .map(|index| format!("--parametro-de-nome-longo-{index}"))
                .collect(),
        );

        assert!(short > 0.0, "the short body measured nothing");
        assert!(
            wrapped > short,
            "wrapped pills must measure taller: {wrapped} vs {short}"
        );
    }

    #[test]
    fn prefix_suggestion_stops_before_the_first_option() {
        let args = ["get", "pods", "-n", "financeiro"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(suggested_prefix_len(&args), Some(2));

        let deep = ["network", "vnet", "subnet", "list", "--group", "x"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(suggested_prefix_len(&deep), Some(4));
    }

    #[test]
    fn prefix_suggestion_falls_back_when_the_boundary_is_too_early_or_absent() {
        let interleaved = ["get", "-n", "financeiro", "pods"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(suggested_prefix_len(&interleaved), None);

        let leading = ["--profile", "dev", "sts", "get-caller-identity"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(suggested_prefix_len(&leading), None);

        let no_options = ["get", "pods"].map(str::to_owned).to_vec();
        assert_eq!(suggested_prefix_len(&no_options), None);
    }

    #[test]
    fn access_resize_keeps_the_current_window_center() {
        let expanded = centered_resize_position(360.0, egui::pos2(100.0, 200.0), 620.0);
        assert_eq!(expanded, egui::pos2(100.0, 70.0));

        let compact = centered_resize_position(620.0, expanded, 360.0);
        assert_eq!(compact, egui::pos2(100.0, 200.0));
    }

    #[test]
    fn compact_token_shows_both_ends_and_size() {
        let value = format!("begin-{}-end", "x".repeat(128));
        let label = compact_token_label(&value, 3);

        assert!(label.starts_with("#4 \"begin-"));
        assert!(label.contains("-end\""));
        assert!(label.ends_with(" B"));
        assert!(!label.contains(&"x".repeat(64)));
    }

    #[test]
    fn large_token_details_are_utf8_safe_and_paged() {
        let value = "🦀".repeat(TOKEN_DETAIL_PAGE_CHARS + 2);
        let first = token_detail_page(&value, 0);
        let second = token_detail_page(&value, 1);

        assert_eq!(
            serde_json::from_str::<String>(&first)
                .unwrap()
                .chars()
                .count(),
            TOKEN_DETAIL_PAGE_CHARS
        );
        assert_eq!(serde_json::from_str::<String>(&second).unwrap(), "🦀🦀");
        assert!(bounded_token_preview(&value, 4).ends_with("…\""));
    }

    #[test]
    fn adding_a_target_counts_only_distinct_unexpired_authorizations() {
        let active = vec![
            ActiveTargetAuthorization {
                target: "dev".into(),
                display_binding: "profile dev · conta 111122223333".into(),
                expires_at_epoch: 200,
            },
            ActiveTargetAuthorization {
                target: "dev".into(),
                display_binding: "profile dev · conta 111122223333".into(),
                expires_at_epoch: 250,
            },
            ActiveTargetAuthorization {
                target: "expired".into(),
                display_binding: "context cluster-antigo · lifecycle kube-auth".into(),
                expires_at_epoch: 99,
            },
        ];

        assert_eq!(active_target_count_after_add(&active, "dev", 100), 1);
        assert_eq!(active_target_count_after_add(&active, "prod", 100), 2);
    }

    #[test]
    fn target_access_private_prompt_payload_carries_human_bindings() {
        let request = PromptRequest::TargetAccess {
            provider: "aws_profile".into(),
            requested_target: "prod".into(),
            requested_binding: "profile cli-prd · conta 123456789012".into(),
            active_targets: vec![ActiveTargetAuthorization {
                target: "dev".into(),
                display_binding: "profile cli-dev · conta 210987654321".into(),
                expires_at_epoch: 200,
            }],
            default_minutes: 15,
        };

        let payload = serde_json::to_value(request).unwrap();
        assert_eq!(payload["kind"], "target_access");
        assert_eq!(
            payload["requested_binding"],
            "profile cli-prd · conta 123456789012"
        );
        assert_eq!(
            payload["active_targets"][0]["display_binding"],
            "profile cli-dev · conta 210987654321"
        );
    }

    #[test]
    fn target_expiration_is_presented_as_a_relative_duration() {
        assert_eq!(target_expiration_label(100, 100), "expirada");
        assert_eq!(target_expiration_label(101, 100), "expira em 1 min");
        assert_eq!(target_expiration_label(3_701, 100), "expira em 1 h 1 min");
    }

    #[test]
    fn target_access_window_height_is_bounded() {
        assert_eq!(target_access_window_height(0), TARGET_ACCESS_MIN_HEIGHT);
        assert_eq!(
            target_access_window_height(1),
            TARGET_ACCESS_MIN_HEIGHT
                + TARGET_ACCESS_ACTIVE_ROW_HEIGHT
                + TARGET_ACCESS_WARNING_HEIGHT
        );
        assert_eq!(
            target_access_window_height(100),
            TARGET_ACCESS_MIN_HEIGHT
                + 6.0 * TARGET_ACCESS_ACTIVE_ROW_HEIGHT
                + TARGET_ACCESS_WARNING_HEIGHT
        );
        assert!(target_access_window_height(100) <= TARGET_ACCESS_MAX_HEIGHT);
    }

    #[test]
    fn target_add_hold_requires_the_full_duration_without_release() {
        let now = Instant::now();
        let mut state = HoldState::default();

        assert_eq!(hold_update(&mut state, true, true, true, now), (0.0, false));
        assert_eq!(
            hold_update(&mut state, true, true, true, now + HOLD_DURATION / 2,),
            (0.5, false)
        );
        assert!(
            !hold_update(
                &mut state,
                true,
                true,
                true,
                now + HOLD_DURATION - Duration::from_millis(1),
            )
            .1
        );
        assert_eq!(
            hold_update(&mut state, true, true, true, now + HOLD_DURATION,),
            (1.0, true)
        );
        assert_eq!(
            hold_update(
                &mut state,
                true,
                true,
                true,
                now + HOLD_DURATION + Duration::from_secs(1),
            ),
            (0.0, false)
        );
    }

    /// Os cinco segundos do botão permanente são o ponto do botão: ele não pode
    /// confirmar no tempo do gesto curto de "permitir uma vez".
    #[test]
    fn the_permanent_hold_takes_its_full_five_seconds() {
        let now = Instant::now();
        let mut state = HoldState::default();

        hold_update_for(&mut state, true, true, true, now, PERMANENT_HOLD_DURATION);
        assert!(
            !hold_update_for(
                &mut state,
                true,
                true,
                true,
                now + HOLD_DURATION,
                PERMANENT_HOLD_DURATION,
            )
            .1,
            "um segundo não pode confirmar uma decisão permanente"
        );
        let (progress, confirmed) = hold_update_for(
            &mut state,
            true,
            true,
            true,
            now + PERMANENT_HOLD_DURATION - Duration::from_millis(1),
            PERMANENT_HOLD_DURATION,
        );
        assert!(!confirmed);
        assert!(progress > 0.99, "{progress}");
        assert_eq!(
            hold_update_for(
                &mut state,
                true,
                true,
                true,
                now + PERMANENT_HOLD_DURATION,
                PERMANENT_HOLD_DURATION,
            ),
            (1.0, true)
        );
    }

    /// Soltar no meio do caminho descarta o progresso, e voltar a pressionar sem
    /// soltar o botão do mouse não recomeça a contagem.
    #[test]
    fn the_permanent_hold_restarts_from_zero_after_a_release() {
        let now = Instant::now();
        let mut state = HoldState::default();

        hold_update_for(&mut state, true, true, true, now, PERMANENT_HOLD_DURATION);
        assert_eq!(
            hold_update_for(
                &mut state,
                false,
                true,
                true,
                now + Duration::from_secs(4),
                PERMANENT_HOLD_DURATION,
            ),
            (0.0, false)
        );
        assert_eq!(
            hold_update_for(
                &mut state,
                true,
                true,
                true,
                now + Duration::from_secs(9),
                PERMANENT_HOLD_DURATION,
            ),
            (0.0, false),
            "o ponteiro nunca foi solto: a contagem não recomeça"
        );
        hold_update_for(
            &mut state,
            false,
            false,
            true,
            now + Duration::from_secs(10),
            PERMANENT_HOLD_DURATION,
        );
        let (progress, confirmed) = hold_update_for(
            &mut state,
            true,
            true,
            true,
            now + Duration::from_secs(11),
            PERMANENT_HOLD_DURATION,
        );
        assert_eq!((progress, confirmed), (0.0, false));
    }

    #[test]
    fn a_permanent_decision_stays_visible_longer_than_a_one_off() {
        assert!(
            terminal_visible_for(AccessChoice::AllowAlways { prefix_len: 1 })
                > terminal_visible_for(AccessChoice::AllowOnce)
        );
        assert_eq!(
            terminal_visible_for(AccessChoice::DenyAlways { prefix_len: 1 }),
            PERMANENT_TERMINAL_VISIBLE_FOR
        );
    }

    #[test]
    fn a_blocked_permanent_policy_offers_neither_button() {
        let blocked = PermanentPolicy::blocked("escopo", "motivo", 3);
        for prefix_len in 1..=3 {
            let option = blocked.at(prefix_len).unwrap();
            assert!(!option.allows_accept());
            assert!(!option.allows_deny());
            assert_eq!(option.shared_block(), Some("motivo"));
        }
        // Uma fronteira fora do vetor não oferece nada, em vez de entrar em pânico.
        assert!(blocked.at(0).is_none());
        assert!(blocked.at(4).is_none());
        // Uma regra vazia nunca é oferecida, mesmo sem motivo registrado.
        let empty = crate::control::PermanentOption {
            rule: String::new(),
            accept_blocked: None,
            deny_blocked: None,
        };
        assert!(!empty.allows_accept());
        assert!(!empty.allows_deny());
    }

    #[test]
    fn target_add_hold_cancels_until_the_pointer_is_released() {
        let now = Instant::now();
        let mut state = HoldState::default();

        hold_update(&mut state, true, true, true, now);
        assert_eq!(
            hold_update(&mut state, false, true, true, now + Duration::from_secs(1),),
            (0.0, false)
        );
        assert_eq!(
            hold_update(&mut state, true, true, true, now + Duration::from_secs(5),),
            (0.0, false)
        );
        hold_update(&mut state, false, false, true, now + Duration::from_secs(6));
        assert_eq!(
            hold_update(&mut state, true, true, true, now + Duration::from_secs(7),),
            (0.0, false)
        );
    }

    #[test]
    fn target_add_hold_cancels_when_the_window_loses_focus() {
        let now = Instant::now();
        let mut state = HoldState::default();

        hold_update(&mut state, true, true, true, now);
        assert_eq!(
            hold_update(&mut state, true, true, false, now + Duration::from_secs(4),),
            (0.0, false)
        );
        assert_eq!(
            hold_update(&mut state, true, true, true, now + Duration::from_secs(5),),
            (0.0, false)
        );
    }

    #[test]
    fn target_add_hold_starts_with_the_click_that_focuses_the_window() {
        let now = Instant::now();
        let mut state = HoldState::default();

        assert_eq!(
            hold_update(&mut state, true, true, false, now),
            (0.0, false)
        );
        assert_eq!(
            hold_update(&mut state, true, true, true, now + HOLD_DURATION / 2,),
            (0.5, false)
        );
        assert_eq!(
            hold_update(&mut state, true, true, true, now + HOLD_DURATION,),
            (1.0, true)
        );
    }
}
