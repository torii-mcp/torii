use crate::config::{env_file, ConfigPaths, TargetPaths};
use crate::error::{Error, Result};
use crate::providers::registry::valid_name;
use crate::providers::{Provider, ProviderRegistry, TargetConfig, TargetMode};
use std::process::Stdio;

pub async fn add(paths: &ConfigPaths, tool: &str, name: &str, context: &str) -> Result<()> {
    if !valid_name(name) {
        return Err(Error::InvalidArguments(format!(
            "invalid target name {name:?}"
        )));
    }
    if context.trim().is_empty() || context.chars().any(|c| matches!(c, '\r' | '\n')) {
        return Err(Error::InvalidArguments(
            "context cannot be empty or contain line breaks".into(),
        ));
    }
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    let destination = provider.paths.target(name);
    if destination.base().exists() {
        return Err(Error::InvalidArguments(format!(
            "target {name:?} already exists for provider tool {tool:?}"
        )));
    }
    validate_context(&provider, context).await?;
    std::fs::create_dir_all(provider.paths.targets_dir()).map_err(|source| Error::Write {
        path: provider.paths.targets_dir(),
        source,
    })?;
    let temp = tempfile::Builder::new()
        .prefix(".target-")
        .tempdir_in(provider.paths.targets_dir())
        .map_err(|source| Error::Write {
            path: provider.paths.targets_dir(),
            source,
        })?;
    let staged = TargetPaths::new(temp.path().to_path_buf());
    staged.ensure()?;
    let config = TargetConfig {
        version: "1".into(),
        name: name.into(),
        context: context.into(),
    };
    let yaml = serde_yaml::to_string(&config).map_err(|source| Error::Yaml {
        path: staged.config(),
        source,
    })?;
    std::fs::write(staged.config(), yaml).map_err(|source| Error::Write {
        path: staged.config(),
        source,
    })?;
    std::fs::rename(temp.path(), destination.base()).map_err(|source| Error::Write {
        path: destination.base().to_path_buf(),
        source,
    })?;
    Ok(())
}

pub fn list(paths: &ConfigPaths, tool: &str) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    for target in provider.targets.values() {
        println!("{}\t{}", target.config.name, target.config.context);
    }
    Ok(())
}

pub fn show(paths: &ConfigPaths, tool: &str, name: &str) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    let target = provider.target(name).ok_or_else(|| {
        Error::InvalidArguments(format!(
            "unknown target {name:?} for provider tool {tool:?}"
        ))
    })?;
    let yaml = serde_yaml::to_string(&target.config).map_err(|source| Error::Yaml {
        path: target.paths.config(),
        source,
    })?;
    print!("{yaml}");
    Ok(())
}

pub fn remove(paths: &ConfigPaths, tool: &str, name: &str, force: bool) -> Result<()> {
    if !force {
        return Err(Error::InvalidArguments(
            "target removal requires --force".into(),
        ));
    }
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    let target = provider.target(name).ok_or_else(|| {
        Error::InvalidArguments(format!(
            "unknown target {name:?} for provider tool {tool:?}"
        ))
    })?;
    let metadata =
        std::fs::symlink_metadata(target.paths.base()).map_err(|source| Error::Read {
            path: target.paths.base().to_path_buf(),
            source,
        })?;
    if metadata.file_type().is_symlink() {
        return Err(Error::InvalidArguments(
            "refusing to remove a symlinked target directory".into(),
        ));
    }
    let expected = provider.paths.targets_dir().join(name);
    if target.paths.base() != expected {
        return Err(Error::InvalidArguments(
            "refusing to remove a target outside its provider".into(),
        ));
    }
    std::fs::remove_dir_all(target.paths.base()).map_err(|source| Error::Write {
        path: target.paths.base().to_path_buf(),
        source,
    })?;
    Ok(())
}

fn targeted_provider(registry: &ProviderRegistry, tool: &str) -> Result<std::sync::Arc<Provider>> {
    let provider = registry
        .get(tool)
        .ok_or_else(|| Error::ProviderNotFound(tool.into()))?;
    if !provider.uses_targets() {
        return Err(Error::InvalidArguments(format!(
            "provider tool {tool:?} is not target-aware"
        )));
    }
    Ok(provider)
}

async fn validate_context(provider: &Provider, context: &str) -> Result<()> {
    let mode = provider
        .config
        .targeting
        .as_ref()
        .expect("targeted provider has targeting")
        .mode;
    let args = match mode {
        TargetMode::KubectlContext => vec![
            "config".to_string(),
            "get-contexts".to_string(),
            context.to_string(),
            "-o".to_string(),
            "name".to_string(),
        ],
    };
    let persistent_env = env_file::load(
        &provider
            .paths
            .base()
            .join(&provider.config.environment.file),
    )?;
    let mut command = tokio::process::Command::new(&provider.config.command);
    command
        .args(&provider.config.args_prefix)
        .args(&args)
        .envs(persistent_env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = command.output().await.map_err(|source| Error::Spawn {
        program: provider.config.command.clone(),
        source,
    })?;
    let found = output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|candidate| candidate.trim() == context);
    if !found {
        return Err(Error::InvalidArguments(format!(
            "kubectl context {context:?} was not found"
        )));
    }
    Ok(())
}
