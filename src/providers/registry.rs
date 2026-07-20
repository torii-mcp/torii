use std::collections::BTreeMap;
use std::path::{Component, Path};
use std::sync::Arc;
use tokio::sync::Mutex;

use super::config::{AuthStrategy, ProviderConfig, TargetConfig, TargetMode};
use crate::config::{ConfigPaths, ProviderPaths, TargetPaths};
use crate::error::{Error, Result};

const RESERVED_MCP_TOOL_NAMES: &[&str] = &["torii_policy"];

#[derive(Debug)]
pub struct Provider {
    pub config: ProviderConfig,
    pub paths: ProviderPaths,
    /// One lock per credential scope. Serializing on the provider as a whole
    /// would make independent scopes wait on each other for no reason.
    auth_locks: std::sync::Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    pub target_access_lock: Arc<Mutex<()>>,
    pub targets: BTreeMap<String, Arc<Target>>,
}

#[derive(Debug)]
pub struct Target {
    pub config: TargetConfig,
    pub paths: TargetPaths,
}

impl Provider {
    pub fn target(&self, name: &str) -> Option<Arc<Target>> {
        self.targets.get(name).cloned()
    }

    pub fn target_names(&self) -> impl Iterator<Item = &str> {
        self.targets.keys().map(String::as_str)
    }

    pub fn uses_targets(&self) -> bool {
        self.config.targeting.is_some()
    }

    pub fn auth_lock(&self, scope: &str) -> Arc<Mutex<()>> {
        let mut locks = self.auth_locks.lock().expect("auth lock map poisoned");
        locks.entry(scope.to_string()).or_default().clone()
    }
}

#[derive(Debug, Clone)]
pub struct ProviderRegistry {
    by_tool: Arc<BTreeMap<String, Arc<Provider>>>,
}

impl ProviderRegistry {
    pub fn load(paths: &ConfigPaths) -> Result<Self> {
        let root = paths.providers();
        let mut by_tool = BTreeMap::new();
        let mut names = std::collections::HashSet::new();
        if !root.exists() {
            return Ok(Self {
                by_tool: Arc::new(by_tool),
            });
        }
        let entries = std::fs::read_dir(&root).map_err(|source| Error::Read {
            path: root.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Read {
                path: root.clone(),
                source,
            })?;
            if !entry
                .file_type()
                .map_err(|source| Error::Read {
                    path: entry.path(),
                    source,
                })?
                .is_dir()
            {
                continue;
            }
            let provider_paths = ProviderPaths::new(entry.path());
            if !provider_paths.config().exists() {
                continue;
            }
            let config = load_config(&provider_paths.config())?;
            validate(&config, provider_paths.base())?;
            if !names.insert(config.name.clone()) {
                return Err(Error::DuplicateProviderName(config.name));
            }
            let tool = config.tool.clone();
            let targets = load_targets(&config, &provider_paths)?;
            let provider = Arc::new(Provider {
                config,
                paths: provider_paths,
                auth_locks: std::sync::Mutex::new(BTreeMap::new()),
                target_access_lock: Arc::new(Mutex::new(())),
                targets,
            });
            if by_tool.insert(tool.clone(), provider).is_some() {
                return Err(Error::DuplicateTool(tool));
            }
        }
        validate_target_providers(&by_tool)?;
        Ok(Self {
            by_tool: Arc::new(by_tool),
        })
    }

    pub fn get(&self, tool: &str) -> Option<Arc<Provider>> {
        self.by_tool.get(tool).cloned()
    }
    pub fn providers(&self) -> impl Iterator<Item = &Arc<Provider>> {
        self.by_tool.values()
    }
    pub fn is_empty(&self) -> bool {
        self.by_tool.is_empty()
    }
}

fn load_config(path: &Path) -> Result<ProviderConfig> {
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

fn load_targets(
    config: &ProviderConfig,
    paths: &ProviderPaths,
) -> Result<BTreeMap<String, Arc<Target>>> {
    let mut targets = BTreeMap::new();
    if config.targeting.is_none() || !paths.targets_dir().exists() {
        return Ok(targets);
    }

    let root = paths.targets_dir();
    let entries = std::fs::read_dir(&root).map_err(|source| Error::Read {
        path: root.clone(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::Read {
            path: root.clone(),
            source,
        })?;
        if !entry
            .file_type()
            .map_err(|source| Error::Read {
                path: entry.path(),
                source,
            })?
            .is_dir()
        {
            continue;
        }
        let target_paths = TargetPaths::new(entry.path());
        if !target_paths.config().exists() {
            continue;
        }
        let target: TargetConfig = load_yaml(&target_paths.config())?;
        let mode = config
            .targeting
            .as_ref()
            .expect("targets only load for target-aware providers")
            .mode;
        validate_target(&target, &target_paths, mode)?;
        let name = target.name.clone();
        let value = Arc::new(Target {
            config: target,
            paths: target_paths,
        });
        if targets.insert(name.clone(), value).is_some() {
            return Err(Error::InvalidProvider {
                provider: config.name.clone(),
                reason: format!("duplicate target {name:?}"),
            });
        }
    }
    Ok(targets)
}

fn load_yaml<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

pub(crate) fn validate(config: &ProviderConfig, _base: &Path) -> Result<()> {
    let fail = |reason: String| Error::InvalidProvider {
        provider: config.name.clone(),
        reason,
    };
    if config.version != "1" {
        return Err(fail(format!("unsupported version {:?}", config.version)));
    }
    if config.name.trim().is_empty() {
        return Err(fail("name cannot be empty".into()));
    }
    if config.command.trim().is_empty() {
        return Err(fail("command cannot be empty".into()));
    }
    if !valid_tool_name(&config.tool) {
        return Err(fail(format!("invalid MCP tool name {:?}", config.tool)));
    }
    if RESERVED_MCP_TOOL_NAMES.contains(&config.tool.as_str()) {
        return Err(fail(format!(
            "MCP tool name {:?} is reserved by Torii",
            config.tool
        )));
    }
    let env_path = Path::new(&config.environment.file);
    if config.environment.file.is_empty()
        || env_path.is_absolute()
        || env_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(fail(
            "environment file must stay inside the provider directory".into(),
        ));
    }
    if matches!(config.auth.strategy, AuthStrategy::Environment) && config.auth.fields.is_empty() {
        return Err(fail("environment auth requires at least one field".into()));
    }
    let mut fields = std::collections::HashSet::new();
    for field in &config.auth.fields {
        if field.name.is_empty()
            || !field
                .name
                .bytes()
                .all(|b| b == b'_' || b.is_ascii_alphanumeric())
        {
            return Err(fail(format!("invalid auth field {:?}", field.name)));
        }
        if !fields.insert(field.name.as_str()) {
            return Err(fail(format!("duplicate auth field {:?}", field.name)));
        }
    }
    if matches!(config.auth.strategy, AuthStrategy::Environment) {
        if config.auth.inject.environment.is_empty() {
            return Err(fail(
                "environment auth requires auth.inject.environment".into(),
            ));
        }
        for (target, template) in &config.auth.inject.environment {
            if target.is_empty()
                || !target
                    .bytes()
                    .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
            {
                return Err(fail(format!("invalid injected environment key {target:?}")));
            }
            let source = template
                .strip_prefix("${")
                .and_then(|value| value.strip_suffix('}'))
                .ok_or_else(|| fail(format!("invalid auth template {template:?}")))?;
            if !fields.contains(source) {
                return Err(fail(format!(
                    "auth template references undeclared field {source:?}"
                )));
            }
        }
    }
    if config
        .auth
        .validate
        .as_ref()
        .is_some_and(|command| command.command.trim().is_empty())
    {
        return Err(fail("auth.validate.command cannot be empty".into()));
    }
    if let Some(targeting) = &config.targeting {
        let mut options = std::collections::HashSet::new();
        for option in &targeting.locked_options {
            if !option.starts_with("--")
                || option.contains('=')
                || option.chars().any(char::is_whitespace)
            {
                return Err(fail(format!("invalid locked option {option:?}")));
            }
            if !options.insert(option) {
                return Err(fail(format!("duplicate locked option {option:?}")));
            }
        }
        if matches!(targeting.mode, TargetMode::AwsProfile)
            && !matches!(config.auth.strategy, AuthStrategy::Inherited)
        {
            return Err(fail(
                "aws_profile targets require auth.strategy: inherited so AWS_PROFILE cannot be overridden by collected environment credentials".into(),
            ));
        }
    }
    Ok(())
}

fn validate_target(config: &TargetConfig, paths: &TargetPaths, mode: TargetMode) -> Result<()> {
    let fail = |reason: String| Error::InvalidProvider {
        provider: config.name.clone(),
        reason,
    };
    if config.version != "1" {
        return Err(fail(format!(
            "unsupported target version {:?}",
            config.version
        )));
    }
    if !valid_name(&config.name) {
        return Err(fail(format!("invalid target name {:?}", config.name)));
    }
    let directory_name = paths
        .base()
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| fail("target directory name is not valid UTF-8".into()))?;
    if directory_name != config.name {
        return Err(fail(format!(
            "target name {:?} does not match directory {directory_name:?}",
            config.name
        )));
    }
    if !valid_tool_name(&config.identity.provider) {
        return Err(fail(format!(
            "invalid identity provider tool {:?}",
            config.identity.provider
        )));
    }
    if let Some(scope) = &config.identity.scope {
        if !valid_name(scope) {
            return Err(fail(format!("invalid identity scope {scope:?}")));
        }
    }
    if config
        .identity
        .profile
        .as_ref()
        .is_some_and(|profile| profile.trim().is_empty() || has_line_break(profile))
    {
        return Err(fail(
            "identity profile cannot be empty or contain line breaks".into(),
        ));
    }
    if config
        .identity
        .expect
        .as_ref()
        .is_some_and(|expect| expect.trim().is_empty() || has_line_break(expect))
    {
        return Err(fail(
            "identity expect cannot be empty or contain line breaks".into(),
        ));
    }
    if config
        .region
        .as_ref()
        .is_some_and(|region| region.trim().is_empty() || has_line_break(region))
    {
        return Err(fail("region cannot be empty or contain line breaks".into()));
    }
    match mode {
        TargetMode::KubectlContext => {
            let context = config.context.as_deref().unwrap_or_default();
            if context.trim().is_empty() || has_line_break(context) {
                return Err(fail(
                    "context cannot be empty or contain line breaks for kubectl_context".into(),
                ));
            }
        }
        TargetMode::AwsProfile => {
            if config.context.is_some() {
                return Err(fail("aws_profile target cannot contain context".into()));
            }
            if config.identity.profile.is_none() {
                return Err(fail("aws_profile target requires identity.profile".into()));
            }
            // The expected identity for an AWS profile is an account id.
            if let Some(expect) = &config.identity.expect {
                if expect.len() != 12 || !expect.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(fail(
                        "expected_account_id must contain exactly 12 ASCII digits for aws_profile"
                            .into(),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn has_line_break(value: &str) -> bool {
    value.chars().any(|c| matches!(c, '\r' | '\n'))
}

fn validate_target_providers(providers: &BTreeMap<String, Arc<Provider>>) -> Result<()> {
    for provider in providers.values() {
        let mode = provider
            .config
            .targeting
            .as_ref()
            .map(|targeting| targeting.mode);
        for target in provider.targets.values() {
            let fail = |reason: String| Error::InvalidProvider {
                provider: provider.config.name.clone(),
                reason: format!("target {:?}: {reason}", target.config.name),
            };
            let identity_provider =
                providers.get(&target.config.identity.provider).ok_or_else(|| {
                    fail(format!(
                        "identity provider tool {:?} is not installed; install the provider before using this target",
                        target.config.identity.provider
                    ))
                })?;
            // aws_profile is the one self-managed mode: the target authenticates
            // through the very tool it belongs to, and that tool is target-aware.
            let self_managed = matches!(mode, Some(TargetMode::AwsProfile))
                && target.config.identity.provider == provider.config.tool;
            if identity_provider.uses_targets() && !self_managed {
                return Err(fail(format!(
                    "identity provider tool {:?} cannot require a target",
                    target.config.identity.provider
                )));
            }
            if matches!(mode, Some(TargetMode::AwsProfile)) && !self_managed {
                return Err(fail(format!(
                    "aws_profile target must use its owning provider tool {:?} as identity provider",
                    provider.config.tool
                )));
            }
            // `expect` is only meaningful when the identity provider can answer
            // "whose credentials are these?" — refuse the silent no-op otherwise.
            if target.config.identity.expect.is_some()
                && identity_provider.config.auth.identity.is_none()
            {
                return Err(fail(format!(
                    "identity expect is set but identity provider tool {:?} declares no auth.identity probe",
                    target.config.identity.provider
                )));
            }
        }
    }
    Ok(())
}

fn valid_tool_name(name: &str) -> bool {
    valid_name(name)
}

pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b == b'_' || b == b'-' || b == b'.' || b.is_ascii_alphanumeric())
}
