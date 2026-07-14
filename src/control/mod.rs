pub mod gui;

use crate::error::Result;
use crate::providers::AuthField;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessChoice {
    Deny,
    AllowOnce,
    AllowFor(u32),
}

pub async fn ask_access(
    provider: &str,
    command: &str,
    rule: &str,
    default_minutes: u32,
) -> Result<AccessChoice> {
    if gui_disabled() {
        return Ok(AccessChoice::Deny);
    }
    gui::ask_access(provider, command, rule, default_minutes).await
}

pub async fn ask_auth(
    provider: &str,
    fields: &[AuthField],
    error: Option<&str>,
) -> Result<Option<std::collections::HashMap<String, String>>> {
    if gui_disabled() {
        return Ok(None);
    }
    gui::ask_auth(provider, fields, error).await
}

pub fn gui_disabled() -> bool {
    ["TORII_NO_GUI", "AWSGATE_NO_GUI"]
        .iter()
        .any(|name| std::env::var_os(name).is_some_and(|v| !v.is_empty() && v != "0"))
}
