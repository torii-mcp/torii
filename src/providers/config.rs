use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub version: String,
    pub name: String,
    pub tool: String,
    pub description: String,
    pub command: String,
    #[serde(default)]
    pub args_prefix: Vec<String>,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub environment: EnvironmentConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub targeting: Option<TargetingConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetingConfig {
    pub mode: TargetMode,
    #[serde(default)]
    pub locked_options: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetMode {
    KubectlContext,
    AwsProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub identity: TargetIdentity,
}

/// Where a target's credentials come from, and which identity they must carry.
///
/// `scope` is the credential bucket key. It defaults to the target name, so two
/// targets of the same provider never share a session by accident; targets that
/// genuinely want one session (same account, several clusters) opt in by naming
/// the same scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetIdentity {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<String>,
}

impl TargetConfig {
    pub fn credential_scope(&self) -> &str {
        self.identity.scope.as_deref().unwrap_or(&self.name)
    }
}

impl TargetingConfig {
    pub fn rejects_argument(&self, argument: &str) -> bool {
        self.mode
            .locked_options()
            .iter()
            .copied()
            .chain(self.locked_options.iter().map(String::as_str))
            .any(|option| {
                argument == option
                    || argument
                        .strip_prefix(option)
                        .is_some_and(|rest| rest.starts_with('='))
            })
    }
}

impl TargetMode {
    fn locked_options(self) -> &'static [&'static str] {
        match self {
            Self::KubectlContext => kubectl_locked_options(),
            Self::AwsProfile => aws_profile_locked_options(),
        }
    }
}

fn kubectl_locked_options() -> &'static [&'static str] {
    &[
        "--context",
        "--kubeconfig",
        "--cluster",
        "--user",
        "--token",
        "--server",
        "--username",
        "--password",
        "--client-key",
        "--client-certificate",
        "--certificate-authority",
        "--insecure-skip-tls-verify",
        "--tls-server-name",
        "--as",
        "--as-group",
        "--as-uid",
        "--as-user-extra",
    ]
}

fn aws_profile_locked_options() -> &'static [&'static str] {
    &[
        "--profile",
        "--region",
        "--endpoint-url",
        "--no-sign-request",
        "--ca-bundle",
        "--no-verify-ssl",
    ]
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyConfig {
    #[serde(default = "default_minimum_accept_tokens")]
    pub minimum_accept_tokens: usize,
    #[serde(default)]
    pub grant_rule: GrantRule,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            minimum_accept_tokens: default_minimum_accept_tokens(),
            grant_rule: GrantRule::default(),
        }
    }
}

fn default_minimum_accept_tokens() -> usize {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrantRule {
    #[serde(default)]
    pub mode: GrantMode,
    #[serde(default)]
    pub count: Option<usize>,
}

impl Default for GrantRule {
    fn default() -> Self {
        Self {
            mode: GrantMode::FirstTokens,
            count: Some(2),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GrantMode {
    #[default]
    FirstTokens,
    Exact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default)]
    pub strategy: AuthStrategy,
    #[serde(default)]
    pub fields: Vec<AuthField>,
    #[serde(default)]
    pub inject: AuthInject,
    pub validate: Option<CommandSpec>,
    /// Probe that answers "whose credentials are these?". Targets compare its
    /// result against `identity.expect`, which is what stops a live session for
    /// one account from being used against a target bound to another.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<IdentityProbe>,
    /// Name of the environment variable that carries a target's `profile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_env: Option<String>,
    /// Ambient variables that must never leak from the Torii server process into
    /// an invocation authenticated by this provider.
    #[serde(default)]
    pub removed_env: Vec<String>,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            strategy: AuthStrategy::Inherited,
            fields: Vec::new(),
            inject: AuthInject::default(),
            validate: None,
            identity: None,
            profile_env: None,
            removed_env: Vec::new(),
            cache_ttl_seconds: default_cache_ttl(),
        }
    }
}

/// Runs under the *credential* provider's own command, never under the command
/// of the provider being invoked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityProbe {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Top-level JSON field of the probe's stdout holding the identity.
    pub field: String,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: u64,
}

fn default_cache_ttl() -> u64 {
    300
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthStrategy {
    Environment,
    #[default]
    Inherited,
    SessionCommand,
    CredentialFile,
}

impl std::fmt::Display for AuthStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Environment => "environment",
            Self::Inherited => "inherited",
            Self::SessionCommand => "session_command",
            Self::CredentialFile => "credential_file",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthField {
    pub name: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub secret: bool,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default)]
    pub multiline: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthInject {
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    #[serde(default = "default_env_file")]
    pub file: String,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            file: default_env_file(),
        }
    }
}

fn default_env_file() -> String {
    ".env".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aws_profile_rejects_identity_and_endpoint_overrides() {
        let targeting = TargetingConfig {
            mode: TargetMode::AwsProfile,
            locked_options: Vec::new(),
        };
        for argument in [
            "--profile",
            "--profile=other",
            "--region=us-east-1",
            "--endpoint-url=http://localhost:4566",
            "--no-sign-request",
            "--ca-bundle=untrusted.pem",
            "--no-verify-ssl",
        ] {
            assert!(targeting.rejects_argument(argument), "{argument}");
        }
        assert!(!targeting.rejects_argument("--output=json"));
    }
}
