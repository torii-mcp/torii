pub mod gui;

use crate::error::Result;
use crate::providers::AuthField;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantSelection {
    Exact,
    Prefix { token_count: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessChoice {
    Deny,
    AllowOnce,
    AllowFor {
        minutes: u32,
        selection: GrantSelection,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthPromptResult {
    pub fields: Option<HashMap<String, String>>,
    pub invalid_attempts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthValidation {
    pub command: Option<String>,
    pub args: Vec<String>,
    pub persistent_env: Vec<(String, String)>,
    pub environment_templates: BTreeMap<String, String>,
}

pub async fn ask_access(
    provider: &str,
    args: &[String],
    default_minutes: u32,
) -> Result<AccessChoice> {
    if gui_disabled() {
        return Ok(AccessChoice::Deny);
    }
    gui::ask_access(provider, args, default_minutes).await
}

pub async fn ask_auth(
    provider: &str,
    fields: &[AuthField],
    error: Option<&str>,
    validation: AuthValidation,
) -> Result<AuthPromptResult> {
    if gui_disabled() {
        return Ok(AuthPromptResult {
            fields: None,
            invalid_attempts: 0,
        });
    }
    gui::ask_auth(provider, fields, error, validation).await
}

pub fn gui_disabled() -> bool {
    std::env::var_os("TORII_NO_GUI").is_some_and(|value| !value.is_empty() && value != "0")
}
