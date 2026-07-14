use crate::config::{env_file, settings, ConfigPaths};
use crate::core::Invoker;
use crate::error::{Error, Result};
use crate::providers::auth::session;
use crate::providers::packages::{self, InstallStatus};
use crate::providers::ProviderRegistry;

pub fn run() -> Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("__prompt") {
        return Ok(crate::control::gui::run_child());
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|error| Error::Mcp(error.to_string()))?;
    runtime.block_on(run_async(args))
}

async fn run_async(args: Vec<String>) -> Result<i32> {
    let paths = ConfigPaths::discover()?;
    let using_default_config = std::env::var_os("TORII_CONFIG_DIR").is_none()
        && std::env::var_os("AWSGATE_CONFIG_DIR").is_none();
    if using_default_config && paths.migrate_awsgate()? {
        eprintln!(
            "Migrated AWS Gate configuration to {}. The migrated session will be validated before use.",
            paths.provider("aws").base().display()
        );
    }
    match args.as_slice() {
        [] => {
            let registry = ProviderRegistry::load(&paths)?;
            if registry.is_empty() {
                return Err(Error::InvalidArguments(format!("no providers installed under {}; run `torii provider install <source>`", paths.providers().display())));
            }
            let settings = settings::load(&paths.settings())?;
            crate::mcp::server::serve(Invoker::new(paths, settings, registry)).await?;
            Ok(0)
        }
        [command] if command == "init" => init(&paths),
        [command] if command == "config-dir" => { println!("{}", paths.base().display()); Ok(0) }
        [command, subcommand] if command == "provider" && subcommand == "list" => {
            let registry = ProviderRegistry::load(&paths)?;
            for provider in registry.providers() {
                let lock = packages::installed_lock(&provider.paths);
                let version = lock.as_ref().map_or("-", |lock| lock.package_version.as_str());
                let source = lock.as_ref().map_or("local", |lock| lock.source.as_str());
                println!("{}\t{}\t{}\t{}\t{}", provider.config.tool, provider.config.name, provider.config.command, version, source);
            }
            Ok(0)
        }
        [command, subcommand] if command == "provider" && subcommand == "search" => {
            for entry in packages::search(None).await? {
                println!("{}\t{}\t{}", entry.name, entry.version, entry.description);
            }
            Ok(0)
        }
        [command, subcommand, query] if command == "provider" && subcommand == "search" => {
            for entry in packages::search(Some(query)).await? {
                println!("{}\t{}\t{}", entry.name, entry.version, entry.description);
            }
            Ok(0)
        }
        [command, subcommand, source] if command == "provider" && subcommand == "install" => {
            let (status, installed) = packages::install(&paths, source).await?;
            match status {
                InstallStatus::Created => {
                    eprintln!("Provider {:?} {} installed with an empty rules.yaml.", installed.name, installed.package_version);
                    Ok(0)
                }
                InstallStatus::AlreadyExists => {
                    eprintln!("Provider directory already exists at {}; not overwriting.", paths.provider(&installed.name).base().display());
                    Ok(1)
                }
            }
        }
        [command, subcommand, provider, setup] if command == "provider" && subcommand == "setup" => {
            packages::setup(&paths, provider, setup)?;
            eprintln!("Setup {setup:?} applied to provider {provider:?}.");
            Ok(0)
        }
        [command, subcommand, provider] if command == "provider" && subcommand == "update" => {
            let installed = packages::update(&paths, provider).await?;
            eprintln!("Provider {:?} updated to {}; rules.yaml was preserved.", installed.name, installed.package_version);
            Ok(0)
        }
        [command, tool] if command == "reauth" => reauth(&paths, tool, None).await,
        [command, tool, target] if command == "reauth" => {
            reauth(&paths, tool, Some(target)).await
        }
        [command, subcommand, tool, name, context_flag, context]
            if command == "target" && subcommand == "add" && context_flag == "--context" =>
        {
            crate::targets::add(&paths, tool, name, context).await?;
            eprintln!("Target {name:?} added to provider tool {tool:?}.");
            Ok(0)
        }
        [command, subcommand, tool] if command == "target" && subcommand == "list" => {
            crate::targets::list(&paths, tool)?;
            Ok(0)
        }
        [command, subcommand, tool, name] if command == "target" && subcommand == "show" => {
            crate::targets::show(&paths, tool, name)?;
            Ok(0)
        }
        [command, subcommand, tool, name, force]
            if command == "target" && subcommand == "remove" && force == "--force" =>
        {
            crate::targets::remove(&paths, tool, name, true)?;
            eprintln!("Target {name:?} removed from provider tool {tool:?}.");
            Ok(0)
        }
        _ => Err(Error::InvalidArguments("usage: torii | torii init | torii reauth <provider-tool> [target] | torii provider list | torii provider search [query] | torii provider install <name|directory|archive|https-url> | torii provider setup <provider> <setup> | torii provider update <provider> | torii target add <provider-tool> <name> --context <kubectl-context> | torii target list <provider-tool> | torii target show <provider-tool> <name> | torii target remove <provider-tool> <name> --force | torii config-dir".into())),
    }
}

fn init(paths: &ConfigPaths) -> Result<i32> {
    paths.ensure()?;
    let settings_path = paths.settings();
    if !settings_path.exists() {
        std::fs::write(&settings_path, include_str!("../examples/settings.yaml")).map_err(
            |source| Error::Write {
                path: settings_path,
                source,
            },
        )?;
    }
    eprintln!(
        "Torii configuration initialized at {}. Install providers with `torii provider install <source>`.",
        paths.base().display()
    );
    Ok(0)
}

async fn reauth(paths: &ConfigPaths, tool: &str, target_name: Option<&String>) -> Result<i32> {
    let registry = ProviderRegistry::load(paths)?;
    let provider = registry
        .get(tool)
        .ok_or_else(|| Error::ProviderNotFound(tool.into()))?;
    let mut persistent = env_file::load(
        &provider
            .paths
            .base()
            .join(&provider.config.environment.file),
    )?;
    let (auth_paths, auth_lock, audit_scope) = if provider.uses_targets() {
        let name = target_name.ok_or_else(|| {
            Error::InvalidArguments(format!("target is required for provider tool {tool:?}"))
        })?;
        let target = provider.target(name).ok_or_else(|| {
            Error::InvalidArguments(format!(
                "unknown target {name:?} for provider tool {tool:?}"
            ))
        })?;
        for (key, value) in env_file::load(&target.paths.env())? {
            if let Some((_, existing)) = persistent.iter_mut().find(|(item, _)| item == &key) {
                *existing = value;
            } else {
                persistent.push((key, value));
            }
        }
        (
            target.paths.auth_paths(),
            target.auth_lock.clone(),
            format!("{}/{}", provider.config.name, target.config.name),
        )
    } else {
        if target_name.is_some() {
            return Err(Error::InvalidArguments(format!(
                "provider tool {tool:?} does not accept a target"
            )));
        }
        (
            provider.paths.auth_paths(),
            provider.auth_lock.clone(),
            provider.config.name.clone(),
        )
    };
    session::ensure_valid(
        paths,
        &provider,
        &auth_paths,
        auth_lock.as_ref(),
        &audit_scope,
        &persistent,
        true,
    )
    .await?;
    eprintln!("Session for {audit_scope:?} renewed and validated.");
    Ok(0)
}
