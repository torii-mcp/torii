use crate::audit;
use crate::config::{env_file, settings, ConfigPaths};
use crate::core::Invoker;
use crate::error::{Error, Result};
use crate::policy::CreateMode;
use crate::providers::auth::session;
use crate::providers::packages::{self, InstallStatus};
use crate::providers::{AuthStrategy, ProviderRegistry};

pub fn run() -> Result<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("__prompt") {
        return Ok(crate::control::gui::run_child());
    }
    if let [command, agent, config_flag, config_dir] = args.as_slice() {
        if command == "__agent-hook"
            && matches!(
                agent.as_str(),
                "codex" | "claude" | "gemini" | "cursor" | "antigravity"
            )
            && config_flag == "--config"
        {
            return crate::agents::codex::run_hook(&ConfigPaths::new(config_dir.into()), agent);
        }
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|error| Error::Mcp(error.to_string()))?;
    runtime.block_on(run_async(args))
}

async fn run_async(args: Vec<String>) -> Result<i32> {
    if let Some(help) = crate::help::request(&args) {
        println!("{help}");
        return Ok(0);
    }
    let paths = ConfigPaths::discover()?;
    match args.as_slice() {
        [command] if matches!(command.as_str(), "--version" | "-V") => {
            println!("{}", version_text());
            Ok(0)
        }
        [] => {
            let registry = ProviderRegistry::load(&paths)?;
            if registry.is_empty() {
                return Err(Error::InvalidArguments(format!(
                    "no providers installed under {}; run `torii provider install <source>`",
                    paths.providers().display()
                )));
            }
            let settings = settings::load(&paths.settings())?;
            // Records past the retention go when a server starts; the audit
            // log never stops a server, so a failure only warns.
            if let Err(reason) = audit::prune(&paths, settings.log_retention_days) {
                eprintln!("torii: the audit log was not pruned: {reason}");
            }
            crate::mcp::server::serve(Invoker::new(paths, settings, registry)).await?;
            Ok(0)
        }
        [command] if command == "init" => init(&paths),
        [command, rest @ ..] if command == "log" => {
            crate::log_cli::run(&paths, rest)?;
            Ok(0)
        }
        [command, rest @ ..] if command == "config" => {
            crate::config_cli::run(&paths, rest)?;
            Ok(0)
        }
        [command, subcommand] if command == "agent" && subcommand == "list" => {
            println!("codex\tMCP and optional PreToolUse hook");
            println!("claude\tMCP and optional PreToolUse hook");
            println!("gemini\tMCP and optional BeforeTool hook");
            println!("cursor\tMCP and optional beforeShellExecution hook");
            println!("antigravity\tMCP and optional PreToolUse hook on run_command");
            println!("opencode\tMCP only; opencode has no pre-execution hook");
            println!("copilot\tMCP only; GitHub Copilot in VS Code has no pre-execution hook");
            println!("copilot-cli\tMCP only; GitHub Copilot CLI has no pre-execution hook");
            println!("kiro\tMCP only; Torii does not manage Kiro workspace hooks");
            println!("pi\tMCP only, and pi needs an MCP extension to read it");
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent" && subcommand == "install" && agent == "codex" =>
        {
            crate::agents::codex::install(&paths, false)?;
            Ok(0)
        }
        [command, subcommand, agent, hook]
            if command == "agent"
                && subcommand == "install"
                && agent == "codex"
                && hook == "--hook" =>
        {
            crate::agents::codex::install(&paths, true)?;
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent" && is_status(subcommand) && agent == "codex" =>
        {
            crate::agents::codex::print_status(&paths)?;
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent" && subcommand == "uninstall" && agent == "codex" =>
        {
            crate::agents::codex::uninstall(&paths, false)?;
            Ok(0)
        }
        [command, subcommand, agent, hook]
            if command == "agent"
                && subcommand == "uninstall"
                && agent == "codex"
                && hook == "--hook" =>
        {
            crate::agents::codex::uninstall(&paths, true)?;
            Ok(0)
        }
        [command, subcommand, agent, flags @ ..]
            if command == "agent" && subcommand == "install" && is_portable_agent(agent) =>
        {
            let (with_hook, assume_yes) = install_flags(flags)?;
            crate::agents::portable::install(&paths, agent, with_hook, assume_yes)?;
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent" && is_status(subcommand) && is_portable_agent(agent) =>
        {
            crate::agents::portable::print_status(&paths, agent)?;
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent" && subcommand == "uninstall" && is_portable_agent(agent) =>
        {
            crate::agents::portable::uninstall(&paths, agent, false)?;
            Ok(0)
        }
        [command, subcommand, agent, hook]
            if command == "agent"
                && subcommand == "uninstall"
                && is_portable_agent(agent)
                && hook == "--hook" =>
        {
            crate::agents::portable::uninstall(&paths, agent, true)?;
            Ok(0)
        }
        [command, subcommand] if command == "self" && subcommand == "upgrade" => {
            crate::self_upgrade::upgrade(false).await
        }
        [command, subcommand, flag]
            if command == "self" && subcommand == "upgrade" && flag == "--check" =>
        {
            crate::self_upgrade::upgrade(true).await
        }
        [command, subcommand, tool] if command == "policy" && subcommand == "show" => {
            crate::policy::show(&paths, tool, None)?;
            Ok(0)
        }
        [command, subcommand, tool, target] if command == "policy" && subcommand == "show" => {
            crate::policy::show(&paths, tool, Some(target))?;
            Ok(0)
        }
        [command, subcommand, tool] if command == "policy" && subcommand == "edit" => {
            crate::policy::edit(&paths, tool, None, CreateMode::No)?;
            Ok(0)
        }
        [command, subcommand, tool, rest @ ..] if command == "policy" && subcommand == "edit" => {
            let (target, create) = policy_edit_options(rest)?;
            crate::policy::edit(&paths, tool, target, create)?;
            Ok(0)
        }
        [command, subcommand] if command == "provider" && subcommand == "list" => {
            let registry = ProviderRegistry::load(&paths)?;
            for provider in registry.providers() {
                let lock = packages::installed_lock(&provider.paths);
                let version = lock
                    .as_ref()
                    .map_or("-", |lock| lock.package_version.as_str());
                let source = lock.as_ref().map_or("local", |lock| lock.source.as_str());
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    provider.config.tool,
                    provider.config.name,
                    provider.config.command,
                    version,
                    source
                );
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
                    eprintln!(
                        "Provider {:?} {} installed with an empty rules.yaml.",
                        installed.name, installed.package_version
                    );
                    Ok(0)
                }
                InstallStatus::AlreadyExists => {
                    eprintln!(
                        "Provider directory already exists at {}; not overwriting.",
                        paths.provider(&installed.name).base().display()
                    );
                    Ok(1)
                }
            }
        }
        [command, subcommand, provider, setup]
            if command == "provider" && matches!(subcommand.as_str(), "setup" | "apply") =>
        {
            packages::setup(&paths, provider, setup)?;
            eprintln!("Setup {setup:?} applied to provider {provider:?}.");
            Ok(0)
        }
        [command, subcommand, provider] if command == "provider" && subcommand == "upgrade" => {
            let installed = packages::upgrade(&paths, provider).await?;
            eprintln!(
                "Provider {:?} upgraded to {}; rules.yaml was preserved.",
                installed.name, installed.package_version
            );
            Ok(0)
        }
        // Nada no Torii atualiza um índice local, então trocar a versão instalada
        // chama-se upgrade em todo o produto. O nome antigo orienta em vez de sumir.
        [command, subcommand, ..]
            if matches!(command.as_str(), "provider" | "self") && subcommand == "update" =>
        {
            Err(Error::InvalidArguments(format!(
                "there is no `torii {command} update`; changing an installed version is `torii {command} upgrade`"
            )))
        }
        [command, tool] if command == "reauth" => reauth(&paths, tool, None).await,
        [command, tool, target] if command == "reauth" => reauth(&paths, tool, Some(target)).await,
        [command, subcommand, tool, name, profile_flag, profile, account_flag, account]
            if command == "target"
                && subcommand == "add"
                && profile_flag == "--profile"
                && account_flag == "--account-id" =>
        {
            crate::targets::add_aws_profile(&paths, tool, name, profile, account, None).await?;
            eprintln!("Target {name:?} added to provider tool {tool:?}.");
            Ok(0)
        }
        [command, subcommand, tool, name, profile_flag, profile, account_flag, account, region_flag, region]
            if command == "target"
                && subcommand == "add"
                && profile_flag == "--profile"
                && account_flag == "--account-id"
                && region_flag == "--region" =>
        {
            crate::targets::add_aws_profile(&paths, tool, name, profile, account, Some(region))
                .await?;
            eprintln!("Target {name:?} added to provider tool {tool:?}.");
            Ok(0)
        }
        // Sem `--context` e sem `--profile`, o alias é um balde de credencial.
        [command, subcommand, tool, name, options @ ..]
            if command == "target"
                && subcommand == "add"
                && !options.iter().any(|option| option == "--context")
                && !options.iter().any(|option| option == "--profile") =>
        {
            let (identity_provider, identity_options) = parse_credentials_add_options(options)?;
            crate::targets::add_credentials(
                &paths,
                tool,
                name,
                identity_provider.as_deref(),
                identity_options,
            )?;
            eprintln!("Target {name:?} added to provider tool {tool:?}.");
            Ok(0)
        }
        [command, subcommand, tool, name, options @ ..]
            if command == "target"
                && subcommand == "add"
                && options.iter().any(|option| option == "--context") =>
        {
            let (context, identity_provider, identity_options) =
                parse_kubectl_add_options(options)?;
            crate::targets::add(
                &paths,
                tool,
                name,
                &context,
                &identity_provider,
                identity_options,
            )
            .await?;
            eprintln!("Target {name:?} added to provider tool {tool:?}.");
            Ok(0)
        }
        [command, subcommand, tool, name, options @ ..]
            if command == "target" && subcommand == "activate" =>
        {
            let settings = settings::load(&paths.settings())?;
            let (minutes, add) =
                parse_target_activation_options(options, settings.default_target_minutes)?;
            let active = crate::targets::activate(&paths, tool, name, minutes, add)?;
            eprintln!(
                "Target {name:?} authorized for provider tool {tool:?} for {minutes} minutes."
            );
            crate::targets::warn_if_multiple(&active);
            Ok(0)
        }
        [command, subcommand, tool] if command == "target" && subcommand == "clear" => {
            crate::targets::clear_authorizations(&paths, tool)?;
            eprintln!("All temporary target authorizations cleared for provider tool {tool:?}.");
            Ok(0)
        }
        [command, subcommand, tool] if command == "target" && is_status(subcommand) => {
            crate::targets::status(&paths, tool)?;
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
        [command, subcommand, tool, name, new_name]
            if command == "target" && subcommand == "rename" =>
        {
            crate::targets::rename(&paths, tool, name, new_name)?;
            eprintln!(
                "Target {name:?} renamed to {new_name:?} for provider tool {tool:?}. It starts inactive; restart running agent clients to announce the new alias."
            );
            Ok(0)
        }
        [command, subcommand, tool, name, force]
            if command == "target" && subcommand == "remove" && force == "--force" =>
        {
            crate::targets::remove(&paths, tool, name, true)?;
            eprintln!("Target {name:?} removed from provider tool {tool:?}.");
            Ok(0)
        }
        _ => {
            let words = args.iter().map(String::as_str).collect::<Vec<_>>();
            Err(Error::Usage(format!(
                "unrecognized command line\n\n{}",
                crate::help::nearest(&words)
            )))
        }
    }
}

fn version_text() -> String {
    format!("torii {}", env!("CARGO_PKG_VERSION"))
}

/// `check` is the verb form of `status`; both stay.
fn is_status(subcommand: &str) -> bool {
    matches!(subcommand, "status" | "check")
}

fn is_portable_agent(agent: &str) -> bool {
    matches!(
        agent,
        "claude"
            | "gemini"
            | "cursor"
            | "antigravity"
            | "opencode"
            | "copilot"
            | "copilot-cli"
            | "kiro"
            | "pi"
    )
}

/// `policy edit <tool> [target] [--create]`: o alvo é posicional e `--create` só
/// faz sentido para um target, que é o único escopo que o Torii pode criar.
fn policy_edit_options(rest: &[String]) -> Result<(Option<&str>, CreateMode)> {
    let mut target = None;
    let mut create = false;
    let mut copy_shared = false;
    for argument in rest {
        match argument.as_str() {
            "--create" if !create => create = true,
            "--copy-shared" if !copy_shared => copy_shared = true,
            value if value.starts_with('-') => {
                return Err(Error::InvalidArguments(format!(
                    "unexpected argument {value:?}; expected an optional target, --create and --copy-shared"
                )));
            }
            value if target.is_none() => target = Some(value),
            value => {
                return Err(Error::InvalidArguments(format!(
                    "unexpected argument {value:?}; only one target can be edited at a time"
                )));
            }
        }
    }
    if create && target.is_none() {
        return Err(Error::InvalidArguments(
            "--create applies to a target policy; pass the target name to create it".into(),
        ));
    }
    // `--copy-shared` diz *como* criar; sozinho ele não tem o que fazer.
    if copy_shared && !create {
        return Err(Error::InvalidArguments(
            "--copy-shared only applies together with --create".into(),
        ));
    }
    let mode = match (create, copy_shared) {
        (false, _) => CreateMode::No,
        (true, false) => CreateMode::Empty,
        (true, true) => CreateMode::CopyShared,
    };
    Ok((target, mode))
}

fn install_flags(flags: &[String]) -> Result<(bool, bool)> {
    let mut with_hook = false;
    let mut assume_yes = false;
    for flag in flags {
        match flag.as_str() {
            "--hook" if !with_hook => with_hook = true,
            "--yes" if !assume_yes => assume_yes = true,
            other => {
                return Err(Error::InvalidArguments(format!(
                    "unexpected argument {other:?}; expected --hook or --yes at most once"
                )));
            }
        }
    }
    Ok((with_hook, assume_yes))
}

fn parse_target_activation_options(
    options: &[String],
    default_minutes: u32,
) -> Result<(u32, bool)> {
    let mut minutes = None;
    let mut add = false;
    let mut index = 0;
    while index < options.len() {
        match options[index].as_str() {
            "--add" if !add => {
                add = true;
                index += 1;
            }
            "--for" if minutes.is_none() => {
                let value = options.get(index + 1).ok_or_else(|| {
                    Error::InvalidArguments("--for requires a number of minutes".into())
                })?;
                minutes = Some(value.parse::<u32>().map_err(|_| {
                    Error::InvalidArguments(format!(
                        "invalid target authorization duration {value:?}"
                    ))
                })?);
                index += 2;
            }
            option => {
                return Err(Error::InvalidArguments(format!(
                    "invalid target activate option {option:?}; expected `--for <minutes>` or `--add`"
                )));
            }
        }
    }
    let minutes = minutes.unwrap_or(default_minutes);
    crate::target_access::validate_duration(minutes)?;
    Ok((minutes, add))
}

/// `target add <tool> <name> [--provider <tool>] [--scope <scope>] [--expect <id>]`
fn parse_credentials_add_options(
    options: &[String],
) -> Result<(Option<String>, crate::targets::IdentityOptions)> {
    let mut provider = None;
    let mut identity = crate::targets::IdentityOptions::default();
    let mut index = 0;
    while index < options.len() {
        let flag = options[index].as_str();
        let value = || -> Result<String> {
            let value = options
                .get(index + 1)
                .ok_or_else(|| Error::InvalidArguments(format!("{flag} requires a value")))?;
            Ok(value.clone())
        };
        match flag {
            "--provider" if provider.is_none() => provider = Some(value()?),
            "--scope" if identity.scope.is_none() => identity.scope = Some(value()?),
            "--expect" if identity.expect.is_none() => identity.expect = Some(value()?),
            option => {
                return Err(Error::InvalidArguments(format!(
                    "invalid target add option {option:?}; expected `[--provider <tool>] [--scope <scope>] [--expect <identity>]`"
                )));
            }
        }
        index += 2;
    }
    Ok((provider, identity))
}

fn parse_kubectl_add_options(
    options: &[String],
) -> Result<(String, String, crate::targets::IdentityOptions)> {
    let mut context = None;
    let mut provider = None;
    let mut identity = crate::targets::IdentityOptions::default();
    let mut index = 0;
    let take_value = |index: &mut usize, flag: &str| -> Result<String> {
        let value = options
            .get(*index + 1)
            .ok_or_else(|| Error::InvalidArguments(format!("{flag} requires a value")))?;
        *index += 2;
        Ok(value.clone())
    };
    while index < options.len() {
        match options[index].as_str() {
            "--context" if context.is_none() => {
                context = Some(take_value(&mut index, "--context")?)
            }
            "--provider" if provider.is_none() => {
                provider = Some(take_value(&mut index, "--provider")?)
            }
            "--scope" if identity.scope.is_none() => {
                identity.scope = Some(take_value(&mut index, "--scope")?)
            }
            "--expect" if identity.expect.is_none() => {
                identity.expect = Some(take_value(&mut index, "--expect")?)
            }
            option => {
                return Err(Error::InvalidArguments(format!(
                    "invalid target add option {option:?}; expected `--context <context> --provider <tool> [--scope <scope>] [--expect <identity>]`"
                )));
            }
        }
    }
    let context =
        context.ok_or_else(|| Error::InvalidArguments("target add requires --context".into()))?;
    let provider =
        provider.ok_or_else(|| Error::InvalidArguments("target add requires --provider".into()))?;
    Ok((context, provider, identity))
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
    let (auth_provider, scope, audit_scope) = if provider.uses_targets() {
        let name = target_name.ok_or_else(|| {
            Error::InvalidArguments(format!("target is required for provider tool {tool:?}"))
        })?;
        let target = provider.target(name).ok_or_else(|| {
            Error::InvalidArguments(format!(
                "unknown target {name:?} for provider tool {tool:?}"
            ))
        })?;
        let auth_provider = registry
            .get(&target.config.identity.provider)
            .ok_or_else(|| Error::ProviderNotFound(target.config.identity.provider.clone()))?;
        (
            auth_provider.clone(),
            target.config.credential_scope().to_string(),
            format!("{}/{}", provider.config.name, target.config.name),
        )
    } else {
        if target_name.is_some() {
            return Err(Error::InvalidArguments(format!(
                "provider tool {tool:?} does not accept a target"
            )));
        }
        (
            provider.clone(),
            provider.config.tool.clone(),
            provider.config.name.clone(),
        )
    };
    // Inherited-with-validator sessions live outside Torii (SSO/profile login);
    // Torii cannot renew them, so point the human at the native flow.
    if matches!(auth_provider.config.auth.strategy, AuthStrategy::Inherited)
        && auth_provider.config.auth.validate.is_some()
    {
        return Err(Error::AwsProfileAuthenticationRequired {
            target: audit_scope,
        });
    }
    let auth_paths = auth_provider.paths.identity_scope(&scope);
    let auth_lock = auth_provider.auth_lock(&scope);
    let persistent = provider_environment(&auth_provider)?;
    let removed_owned = auth_provider.config.auth.removed_env.clone();
    let removed_env: Vec<&str> = removed_owned.iter().map(String::as_str).collect();
    session::ensure_valid(
        &auth_provider,
        &auth_paths,
        auth_lock.as_ref(),
        session::SessionScope {
            audit: &audit_scope,
            requested_for: provider.uses_targets().then_some(audit_scope.as_str()),
            trail: audit::Trail::Control(paths),
        },
        session::SessionEnvironment {
            persistent_env: &persistent,
            removed_env: &removed_env,
        },
        true,
    )
    .await?;
    eprintln!("Session for {audit_scope:?} renewed and validated.");
    Ok(0)
}

fn provider_environment(provider: &crate::providers::Provider) -> Result<Vec<(String, String)>> {
    env_file::load(
        &provider
            .paths
            .base()
            .join(&provider.config.environment.file),
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_target_activation_options, reauth, version_text};
    use crate::config::ConfigPaths;
    use crate::error::Error;

    #[test]
    fn version_text_identifies_this_binary() {
        assert_eq!(
            version_text(),
            format!("torii {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn target_activation_options_are_order_independent_and_bounded() {
        assert_eq!(
            parse_target_activation_options(&[], 30).unwrap(),
            (30, false)
        );
        assert_eq!(
            parse_target_activation_options(&["--add".into(), "--for".into(), "45".into()], 30)
                .unwrap(),
            (45, true)
        );
        assert!(parse_target_activation_options(&["--for".into(), "0".into()], 30).is_err());
        assert!(parse_target_activation_options(&["--add".into(), "--add".into()], 30).is_err());
    }

    #[tokio::test]
    async fn reauth_does_not_attempt_to_switch_an_aws_profile_target() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider = paths.provider("aws-profile");
        provider.ensure().unwrap();
        std::fs::write(
            provider.config(),
            "version: '1'\nname: aws-profile\ntool: aws_profile\ndescription: test\ncommand: executable-that-must-not-run\ntargeting: { mode: aws_profile }\nauth: { strategy: inherited, validate: { command: executable-that-must-not-run, args: [] }, identity: { command: executable-that-must-not-run, args: [], field: Account }, profile_env: AWS_PROFILE }\n",
        )
        .unwrap();
        std::fs::write(provider.rules(), "version: '1.0'\ndeny: []\naccept: []\n").unwrap();
        let target = provider.target("prod");
        target.ensure().unwrap();
        std::fs::write(
            target.config(),
            "version: '1'\nname: prod\nidentity:\n  provider: aws_profile\n  scope: production-sso\n  profile: production-sso\n  expect: '123456789012'\n",
        )
        .unwrap();

        let name = "prod".to_string();
        let error = reauth(&paths, "aws_profile", Some(&name))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            Error::AwsProfileAuthenticationRequired { .. }
        ));
    }
}
