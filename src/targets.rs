use crate::audit;
use crate::config::{env_file, ConfigPaths, TargetPaths};
use crate::error::{Error, Result};
use crate::providers::registry::valid_name;
use crate::providers::{Provider, ProviderRegistry, TargetConfig, TargetIdentity, TargetMode};
use crate::target_access::{self, ActivationMode, TargetAuthorization};
use std::process::Stdio;

/// Optional identity knobs a human may set when adding a target.
#[derive(Debug, Default, Clone)]
pub struct IdentityOptions {
    /// Credential bucket; defaults to the target name (isolated by default).
    pub scope: Option<String>,
    /// Identity the session must carry (checked against the provider's probe).
    pub expect: Option<String>,
}

pub async fn add(
    paths: &ConfigPaths,
    tool: &str,
    name: &str,
    context: &str,
    identity_provider: &str,
    options: IdentityOptions,
) -> Result<()> {
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
    require_mode(&provider, TargetMode::KubectlContext)?;
    let identity_provider = validate_identity_provider(&registry, identity_provider)?;
    check_expect_probe(&identity_provider, &options)?;
    let destination = provider.paths.target(name);
    if destination.base().exists() {
        return Err(Error::InvalidArguments(format!(
            "target {name:?} already exists for provider tool {tool:?}"
        )));
    }
    validate_context(&provider, context).await?;
    let config = TargetConfig {
        version: "1".into(),
        name: name.into(),
        context: Some(context.into()),
        region: None,
        identity: TargetIdentity {
            provider: identity_provider.config.tool.clone(),
            scope: options.scope,
            profile: None,
            expect: options.expect,
        },
    };
    write_target(&provider, name, &config)
}

pub async fn add_aws_profile(
    paths: &ConfigPaths,
    tool: &str,
    name: &str,
    profile: &str,
    expected_account_id: &str,
    region: Option<&str>,
) -> Result<()> {
    if !valid_name(name) {
        return Err(Error::InvalidArguments(format!(
            "invalid target name {name:?}"
        )));
    }
    validate_profile(profile)?;
    validate_account_id(expected_account_id)?;
    if let Some(region) = region {
        validate_region(region)?;
    }
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    require_mode(&provider, TargetMode::AwsProfile)?;
    let destination = provider.paths.target(name);
    if destination.base().exists() {
        return Err(Error::InvalidArguments(format!(
            "target {name:?} already exists for provider tool {tool:?}"
        )));
    }
    let config = TargetConfig {
        version: "1".into(),
        name: name.into(),
        context: None,
        region: region.map(str::to_owned),
        identity: TargetIdentity {
            // aws_profile targets authenticate through their own tool.
            provider: provider.config.tool.clone(),
            // One bucket per profile: same profile shares a session, different
            // profiles stay isolated.
            scope: Some(profile.to_owned()),
            profile: Some(profile.into()),
            expect: Some(expected_account_id.into()),
        },
    };
    write_target(&provider, name, &config)
}

fn write_target(provider: &Provider, name: &str, config: &TargetConfig) -> Result<()> {
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
    let yaml = serde_yaml::to_string(config).map_err(|source| Error::Yaml {
        path: staged.config(),
        source,
    })?;
    std::fs::write(staged.config(), yaml).map_err(|source| Error::Write {
        path: staged.config(),
        source,
    })?;
    let destination = provider.paths.target(name);
    std::fs::rename(temp.path(), destination.base()).map_err(|source| Error::Write {
        path: destination.base().to_path_buf(),
        source,
    })?;
    Ok(())
}

fn validate_identity_provider(
    registry: &ProviderRegistry,
    tool: &str,
) -> Result<std::sync::Arc<Provider>> {
    let provider = registry
        .get(tool)
        .ok_or_else(|| {
            Error::InvalidArguments(format!(
                "identity provider tool {tool:?} is not installed; install the provider before adding this target"
            ))
        })?;
    if provider.uses_targets() {
        return Err(Error::InvalidArguments(format!(
            "identity provider tool {tool:?} cannot require a target"
        )));
    }
    Ok(provider)
}

fn check_expect_probe(identity_provider: &Provider, options: &IdentityOptions) -> Result<()> {
    if options.expect.is_some() && identity_provider.config.auth.identity.is_none() {
        return Err(Error::InvalidArguments(format!(
            "--expect requires identity provider tool {:?} to declare an auth.identity probe",
            identity_provider.config.tool
        )));
    }
    Ok(())
}

pub fn list(paths: &ConfigPaths, tool: &str) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    for target in provider.targets.values() {
        let identity = &target.config.identity;
        let scope = target.config.credential_scope();
        match target_mode(&provider)? {
            TargetMode::KubectlContext => println!(
                "{}\t{}\t{}\t{}",
                target.config.name,
                target.config.context.as_deref().expect("validated context"),
                identity.provider,
                scope,
            ),
            TargetMode::AwsProfile => println!(
                "{}\t{}\t{}\t{}",
                target.config.name,
                identity.profile.as_deref().expect("validated profile"),
                identity.expect.as_deref().unwrap_or("-"),
                scope,
            ),
        }
    }
    Ok(())
}

pub fn activate(
    paths: &ConfigPaths,
    tool: &str,
    name: &str,
    minutes: u32,
    add: bool,
) -> Result<Vec<TargetAuthorization>> {
    target_access::validate_duration(minutes)?;
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    if provider.target(name).is_none() {
        return Err(Error::InvalidArguments(format!(
            "unknown target {name:?} for provider tool {tool:?}"
        )));
    }
    let known = target_access::known_targets(&provider)?;
    let mode = if add {
        ActivationMode::Add
    } else {
        ActivationMode::Replace
    };
    let active = target_access::activate(
        &provider.paths.target_authorizations(),
        &provider.paths.target_authorizations_lock(),
        &known,
        name,
        audit::now_epoch(),
        minutes,
        mode,
    )?;
    audit::log(
        paths,
        &provider.config.name,
        if add {
            "target-access-added"
        } else {
            "target-access-replaced"
        },
        name,
        &format!("{minutes}min active={}", active.len()),
    );
    Ok(active)
}

pub fn clear_authorizations(paths: &ConfigPaths, tool: &str) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    target_access::clear(
        &provider.paths.target_authorizations(),
        &provider.paths.target_authorizations_lock(),
    )?;
    audit::log(
        paths,
        &provider.config.name,
        "target-access-cleared",
        "-",
        "all",
    );
    Ok(())
}

pub fn status(paths: &ConfigPaths, tool: &str) -> Result<()> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = targeted_provider(&registry, tool)?;
    let known = target_access::known_targets(&provider)?;
    let now = audit::now_epoch();
    let snapshot = target_access::load(&provider.paths.target_authorizations(), &known, now)?;
    for target in provider.target_names() {
        if let Some(active) = snapshot
            .active
            .iter()
            .find(|authorization| authorization.target == target)
        {
            let remaining_minutes = active.expires_at.saturating_sub(now).div_ceil(60);
            println!(
                "{}\tactive\texpires_at={}\tremaining={}min",
                target, active.expires_at, remaining_minutes
            );
        } else {
            println!("{target}\tinactive\t-\t-");
        }
    }
    warn_if_multiple(&snapshot.active);
    Ok(())
}

pub fn warn_if_multiple(active: &[TargetAuthorization]) {
    if active.len() <= 1 {
        return;
    }
    let names = active
        .iter()
        .map(|authorization| authorization.target.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!(
        "WARNING: multiple targets are authorized ({names}). The agent may choose any of them for operations allowed by policy until they expire or are cleared."
    );
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
    let known = target_access::known_targets(&provider)?;
    target_access::revoke(
        &provider.paths.target_authorizations(),
        &provider.paths.target_authorizations_lock(),
        &known,
        name,
        audit::now_epoch(),
    )?;
    audit::log(
        paths,
        &provider.config.name,
        "target-access-revoked",
        name,
        "target-removed",
    );
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

fn require_mode(provider: &Provider, expected: TargetMode) -> Result<()> {
    if target_mode(provider)? != expected {
        return Err(Error::InvalidArguments(format!(
            "provider tool {:?} does not use target mode {:?}",
            provider.config.tool, expected
        )));
    }
    Ok(())
}

fn target_mode(provider: &Provider) -> Result<TargetMode> {
    provider
        .config
        .targeting
        .as_ref()
        .map(|targeting| targeting.mode)
        .ok_or_else(|| {
            Error::InvalidArguments(format!(
                "provider tool {:?} is not target-aware",
                provider.config.tool
            ))
        })
}

fn validate_profile(profile: &str) -> Result<()> {
    if profile.trim().is_empty() || profile.chars().any(|c| matches!(c, '\r' | '\n')) {
        return Err(Error::InvalidArguments(
            "profile cannot be empty or contain line breaks".into(),
        ));
    }
    Ok(())
}

fn validate_account_id(account_id: &str) -> Result<()> {
    if account_id.len() != 12 || !account_id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::InvalidArguments(
            "account ID must contain exactly 12 ASCII digits".into(),
        ));
    }
    Ok(())
}

fn validate_region(region: &str) -> Result<()> {
    if region.trim().is_empty() || region.chars().any(|c| matches!(c, '\r' | '\n')) {
        return Err(Error::InvalidArguments(
            "region cannot be empty or contain line breaks".into(),
        ));
    }
    Ok(())
}

async fn validate_context(provider: &Provider, context: &str) -> Result<()> {
    require_mode(provider, TargetMode::KubectlContext)?;
    let args = vec![
        "config".to_string(),
        "get-contexts".to_string(),
        context.to_string(),
        "-o".to_string(),
        "name".to_string(),
    ];
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn add_aws_profile_writes_a_self_managed_target_binding() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider = paths.provider("aws-profile");
        provider.ensure().unwrap();
        fs::write(
            provider.config(),
            "version: '1'\nname: aws-profile\ntool: aws_profile\ndescription: AWS profile target test provider\ncommand: aws\ntargeting: { mode: aws_profile }\nauth: { strategy: inherited, identity: { command: aws, args: [sts, get-caller-identity], field: Account }, profile_env: AWS_PROFILE }\nenvironment: { file: .env }\n",
        )
        .unwrap();
        fs::write(provider.rules(), "version: '1.0'\ndeny: []\naccept: []\n").unwrap();

        add_aws_profile(
            &paths,
            "aws_profile",
            "prod",
            "production-sso",
            "123456789012",
            Some("sa-east-1"),
        )
        .await
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let target = registry.get("aws_profile").unwrap().target("prod").unwrap();
        assert_eq!(target.config.version, "1");
        assert_eq!(
            target.config.identity.profile.as_deref(),
            Some("production-sso")
        );
        assert_eq!(
            target.config.identity.expect.as_deref(),
            Some("123456789012")
        );
        // Profile-based targets bucket their session by profile name.
        assert_eq!(target.config.credential_scope(), "production-sso");
        assert_eq!(target.config.region.as_deref(), Some("sa-east-1"));
        assert_eq!(target.config.identity.provider, "aws_profile");
    }

    #[tokio::test]
    async fn activate_add_and_clear_touch_only_target_authorizations() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider = paths.provider("aws-profile");
        provider.ensure().unwrap();
        fs::write(
            provider.config(),
            "version: '1'\nname: aws-profile\ntool: aws_profile\ndescription: AWS profile target test provider\ncommand: aws\ntargeting: { mode: aws_profile }\nauth: { strategy: inherited, identity: { command: aws, args: [sts, get-caller-identity], field: Account }, profile_env: AWS_PROFILE }\nenvironment: { file: .env }\n",
        )
        .unwrap();
        fs::write(provider.rules(), "version: '1.0'\ndeny: []\naccept: []\n").unwrap();
        add_aws_profile(
            &paths,
            "aws_profile",
            "dev",
            "development-sso",
            "111122223333",
            None,
        )
        .await
        .unwrap();
        add_aws_profile(
            &paths,
            "aws_profile",
            "prod",
            "production-sso",
            "999900001111",
            Some("sa-east-1"),
        )
        .await
        .unwrap();

        let prod = provider.target("prod");
        fs::write(prod.env(), "PAGER=\n").unwrap();
        fs::write(prod.grants(), "preserved grant bytes").unwrap();
        let preserved = [prod.config(), prod.env(), prod.grants()]
            .map(|path| (path.clone(), fs::read(path).unwrap()));

        let active = activate(&paths, "aws_profile", "dev", 30, false).unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].target, "dev");
        let active = activate(&paths, "aws_profile", "prod", 15, true).unwrap();
        assert_eq!(
            active
                .iter()
                .map(|authorization| authorization.target.as_str())
                .collect::<Vec<_>>(),
            ["dev", "prod"]
        );

        remove(&paths, "aws_profile", "dev", true).unwrap();
        assert!(!provider.target("dev").base().exists());
        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider_after_remove = registry.get("aws_profile").unwrap();
        let known = target_access::known_targets(&provider_after_remove).unwrap();
        let active_after_remove = target_access::load(
            &provider_after_remove.paths.target_authorizations(),
            &known,
            audit::now_epoch(),
        )
        .unwrap()
        .active;
        assert_eq!(active_after_remove.len(), 1);
        assert_eq!(active_after_remove[0].target, "prod");

        clear_authorizations(&paths, "aws_profile").unwrap();
        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("aws_profile").unwrap();
        let known = target_access::known_targets(&provider).unwrap();
        assert!(target_access::load(
            &provider.paths.target_authorizations(),
            &known,
            audit::now_epoch()
        )
        .unwrap()
        .active
        .is_empty());
        for (path, expected) in preserved {
            assert_eq!(fs::read(path).unwrap(), expected);
        }
    }
}
