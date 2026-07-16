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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetMode {
    KubectlContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub version: String,
    pub name: String,
    pub context: String,
    pub provider: String,
}

impl TargetingConfig {
    pub fn rejects_argument(&self, argument: &str) -> bool {
        kubectl_locked_options()
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
            cache_ttl_seconds: default_cache_ttl(),
        }
    }
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
