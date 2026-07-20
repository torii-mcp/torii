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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveTargetAuthorization {
    pub target: String,
    pub display_binding: String,
    pub expires_at_epoch: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetAccessChoice {
    Deny,
    Replace { minutes: u32 },
    Add { minutes: u32 },
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

pub async fn ask_target_access(
    provider: &str,
    requested_target: &str,
    requested_binding: &str,
    active_targets: &[ActiveTargetAuthorization],
    default_minutes: u32,
) -> Result<TargetAccessChoice> {
    if gui_disabled() {
        return Ok(TargetAccessChoice::Deny);
    }
    gui::ask_target_access(
        provider,
        requested_target,
        requested_binding,
        active_targets,
        default_minutes,
    )
    .await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_access_choice_round_trips_with_duration() {
        let choice = TargetAccessChoice::Add { minutes: 25 };
        let json = serde_json::to_string(&choice).unwrap();
        assert_eq!(
            serde_json::from_str::<TargetAccessChoice>(&json).unwrap(),
            choice
        );
    }

    #[test]
    fn target_access_is_denied_in_a_headless_child() {
        let executable = std::env::current_exe().unwrap();
        let output = std::process::Command::new(executable)
            .args([
                "--exact",
                "control::tests::headless_target_access_child",
                "--nocapture",
            ])
            .env("TORII_TARGET_ACCESS_HEADLESS_TEST", "1")
            .env("TORII_NO_GUI", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "headless child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn headless_target_access_child() {
        if std::env::var_os("TORII_TARGET_ACCESS_HEADLESS_TEST").is_none() {
            return;
        }
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let choice = runtime
            .block_on(ask_target_access(
                "aws_profile",
                "cli_prd",
                "profile cli-prd · conta 123456789012 · região sa-east-1",
                &[],
                15,
            ))
            .unwrap();
        assert_eq!(choice, TargetAccessChoice::Deny);
    }
}
