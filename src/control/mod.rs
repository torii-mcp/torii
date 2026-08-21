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
        /// Seconds, not minutes: aligning with an older lease lands on a
        /// sub-minute duration (4 min 30 s of an expiring five-minute grant).
        seconds: u32,
        selection: GrantSelection,
    },
    /// Write a rule over the first `prefix_len` arguments into the policy's
    /// `accept` and run this call. The window only returns this after a
    /// five-second press-and-hold, and the server rebuilds the rule from the
    /// boundary rather than trusting a rule string from the prompt process.
    AllowAlways {
        prefix_len: usize,
    },
    /// The same at the `deny` vector, refusing this call.
    DenyAlways {
        prefix_len: usize,
    },
}

/// What a permanent decision would write at one prefix boundary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermanentOption {
    /// The literal rule for this boundary. Empty when it cannot be written.
    pub rule: String,
    /// Why the permanent accept is unavailable at this boundary, when it is.
    pub accept_blocked: Option<String>,
    /// Why the permanent deny is unavailable at this boundary, when it is.
    pub deny_blocked: Option<String>,
}

impl PermanentOption {
    fn blocked(reason: String) -> Self {
        Self {
            rule: String::new(),
            accept_blocked: Some(reason.clone()),
            deny_blocked: Some(reason),
        }
    }

    pub fn allows_accept(&self) -> bool {
        self.accept_blocked.is_none() && !self.rule.is_empty()
    }

    pub fn allows_deny(&self) -> bool {
        self.deny_blocked.is_none() && !self.rule.is_empty()
    }

    /// The reason both buttons are unavailable, when it is the same one.
    pub fn shared_block(&self) -> Option<&str> {
        match (&self.accept_blocked, &self.deny_blocked) {
            (Some(accept), Some(deny)) if accept == deny => Some(accept),
            _ => None,
        }
    }
}

/// What a permanent decision would write, and where.
///
/// Computed by the server before the window opens, one option per prefix
/// boundary the human can choose: the window never derives a rule of its own,
/// and a boundary whose write could not work is disabled with the reason on it
/// instead of failing after the human already held the button for five seconds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermanentPolicy {
    /// Human label of the policy file that would receive the rule.
    pub scope: String,
    /// Index `k - 1` describes a rule over the first `k` displayed arguments.
    pub options: Vec<PermanentOption>,
}

impl PermanentPolicy {
    /// Nothing permanent is offered at any boundary, all for the same reason.
    pub fn blocked(scope: impl Into<String>, reason: impl Into<String>, len: usize) -> Self {
        let reason = reason.into();
        Self {
            scope: scope.into(),
            options: (0..len)
                .map(|_| PermanentOption::blocked(reason.clone()))
                .collect(),
        }
    }

    /// The option for a boundary of `prefix_len` arguments, when there is one.
    pub fn at(&self, prefix_len: usize) -> Option<&PermanentOption> {
        self.options.get(prefix_len.checked_sub(1)?)
    }
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
    align_seconds: Option<u32>,
    permanent: PermanentPolicy,
) -> Result<AccessChoice> {
    if gui_disabled() {
        return Ok(AccessChoice::Deny);
    }
    gui::ask_access(provider, args, default_minutes, align_seconds, permanent).await
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
