use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;

use eframe::egui;
use serde::{Deserialize, Serialize};

use super::AccessChoice;
use crate::error::{Error, Result};
use crate::providers::AuthField;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PromptRequest {
    Access {
        provider: String,
        command: String,
        rule: String,
        default_minutes: u32,
    },
    Auth {
        provider: String,
        fields: Vec<AuthField>,
        error: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
enum PromptResponse {
    Access(AccessChoice),
    Auth(Option<HashMap<String, String>>),
    Error(String),
}

pub async fn ask_access(
    provider: &str,
    command: &str,
    rule: &str,
    default_minutes: u32,
) -> Result<AccessChoice> {
    let request = PromptRequest::Access {
        provider: provider.into(),
        command: command.into(),
        rule: rule.into(),
        default_minutes,
    };
    match invoke_child(request).await? {
        PromptResponse::Access(choice) => Ok(choice),
        PromptResponse::Error(message) => Err(Error::Prompt(message)),
        _ => Err(Error::Prompt("unexpected access prompt response".into())),
    }
}

pub async fn ask_auth(
    provider: &str,
    fields: &[AuthField],
    error: Option<&str>,
) -> Result<Option<HashMap<String, String>>> {
    let request = PromptRequest::Auth {
        provider: provider.into(),
        fields: fields.to_vec(),
        error: error.map(str::to_owned),
    };
    match invoke_child(request).await? {
        PromptResponse::Auth(fields) => Ok(fields),
        PromptResponse::Error(message) => Err(Error::Prompt(message)),
        _ => Err(Error::Prompt(
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
        let payload =
            serde_json::to_vec(&request).map_err(|error| Error::Prompt(error.to_string()))?;
        child
            .stdin
            .take()
            .ok_or_else(|| Error::Prompt("prompt stdin unavailable".into()))?
            .write_all(&payload)
            .map_err(|error| Error::Prompt(error.to_string()))?;
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
    let mut payload = String::new();
    if std::io::stdin().read_to_string(&mut payload).is_err() {
        return 1;
    }
    let response = match serde_json::from_str::<PromptRequest>(&payload) {
        Ok(PromptRequest::Access {
            provider,
            command,
            rule,
            default_minutes,
        }) => PromptResponse::Access(
            access_window(provider, command, rule, default_minutes).unwrap_or(AccessChoice::Deny),
        ),
        Ok(PromptRequest::Auth {
            provider,
            fields,
            error,
        }) => PromptResponse::Auth(auth_window(provider, fields, error).unwrap_or(None)),
        Err(error) => PromptResponse::Error(error.to_string()),
    };
    match serde_json::to_writer(std::io::stdout(), &response) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn native_options(width: f32, height: f32) -> eframe::NativeOptions {
    eframe::NativeOptions {
        centered: true,
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([width, height])
            .with_resizable(false)
            .with_minimize_button(false)
            .with_maximize_button(false)
            .with_always_on_top()
            .with_active(true),
        ..Default::default()
    }
}

fn close(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    ctx.request_repaint();
}

struct AccessApp {
    provider: String,
    command: String,
    rule: String,
    aware: bool,
    timed: bool,
    minutes: u32,
    outcome: Rc<RefCell<Option<AccessChoice>>>,
}

impl eframe::App for AccessApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("Torii — autorização ({})", self.provider));
            ui.label("A ação não está resolvida pela política.");
            ui.separator();
            ui.label("Comando:");
            egui::ScrollArea::vertical()
                .max_height(60.0)
                .show(ui, |ui| {
                    ui.monospace(&self.command);
                });
            ui.horizontal(|ui| {
                ui.label("Grant:");
                ui.strong(egui::RichText::new(&self.rule).monospace());
            });
            ui.separator();
            ui.checkbox(&mut self.aware, "Estou ciente dos riscos");
            ui.add_enabled_ui(self.aware, |ui| {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.timed, "Permitir por");
                    ui.add(egui::DragValue::new(&mut self.minutes).range(1..=1440));
                    ui.label("minutos");
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Negar").clicked() {
                    *self.outcome.borrow_mut() = Some(AccessChoice::Deny);
                    close(ctx);
                }
                ui.add_enabled_ui(self.aware, |ui| {
                    if ui.button("Permitir").clicked() {
                        *self.outcome.borrow_mut() = Some(if self.timed {
                            AccessChoice::AllowFor(self.minutes)
                        } else {
                            AccessChoice::AllowOnce
                        });
                        close(ctx);
                    }
                });
            });
        });
    }
}

fn access_window(
    provider: String,
    command: String,
    rule: String,
    default_minutes: u32,
) -> std::result::Result<AccessChoice, String> {
    let outcome = Rc::new(RefCell::new(None));
    let result = Rc::clone(&outcome);
    eframe::run_native(
        "Torii — autorização",
        native_options(650.0, 310.0),
        Box::new(move |_| {
            Ok(Box::new(AccessApp {
                provider,
                command,
                rule,
                aware: false,
                timed: false,
                minutes: default_minutes.max(1),
                outcome: result,
            }))
        }),
    )
    .map_err(|error| error.to_string())?;
    let value = outcome.borrow().unwrap_or(AccessChoice::Deny);
    Ok(value)
}

struct AuthApp {
    provider: String,
    fields: Vec<AuthField>,
    values: HashMap<String, String>,
    error: Option<String>,
    outcome: Rc<RefCell<Option<HashMap<String, String>>>>,
}

impl eframe::App for AuthApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(format!("Torii — autenticação ({})", self.provider));
            ui.label("A nova sessão só substituirá a anterior após validação.");
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(360.0)
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
                            ui.add(
                                egui::TextEdit::multiline(value)
                                    .password(field.secret)
                                    .desired_rows(4)
                                    .desired_width(f32::INFINITY),
                            )
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
            if ui.button("Colar atribuições do clipboard").clicked() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        let allowed: Vec<String> =
                            self.fields.iter().map(|field| field.name.clone()).collect();
                        if let Ok(values) = crate::config::env_file::parse_allowed(&text, &allowed)
                        {
                            self.values.extend(values);
                        }
                    }
                }
                self.error = None;
            }
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::RED, error);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Cancelar").clicked() {
                    close(ctx);
                }
                if ui.button("Validar e usar").clicked() {
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
                    if missing.is_empty() {
                        *self.outcome.borrow_mut() = Some(self.values.clone());
                        close(ctx);
                    } else {
                        self.error = Some(format!("Campos obrigatórios: {}", missing.join(", ")));
                    }
                }
            });
        });
    }
}

fn auth_window(
    provider: String,
    fields: Vec<AuthField>,
    error: Option<String>,
) -> std::result::Result<Option<HashMap<String, String>>, String> {
    let outcome = Rc::new(RefCell::new(None));
    let result = Rc::clone(&outcome);
    eframe::run_native(
        "Torii — autenticação",
        native_options(620.0, 560.0),
        Box::new(move |_| {
            Ok(Box::new(AuthApp {
                provider,
                fields,
                values: HashMap::new(),
                error,
                outcome: result,
            }))
        }),
    )
    .map_err(|error| error.to_string())?;
    let value = outcome.borrow().clone();
    Ok(value)
}
