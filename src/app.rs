use crate::config::{env_file, settings, ConfigPaths};
use crate::core::Invoker;
use crate::error::{Error, Result};
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
            && matches!(agent.as_str(), "codex" | "claude" | "gemini" | "cursor")
            && config_flag == "--config"
        {
            return crate::agents::codex::run_hook(&ConfigPaths::new(config_dir.into()), agent);
        }
    }
    let runtime = tokio::runtime::Runtime::new().map_err(|error| Error::Mcp(error.to_string()))?;
    runtime.block_on(run_async(args))
}

async fn run_async(args: Vec<String>) -> Result<i32> {
    if let Some(help) = help_request(&args) {
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
            crate::mcp::server::serve(Invoker::new(paths, settings, registry)).await?;
            Ok(0)
        }
        [command] if command == "init" => init(&paths),
        [command] if command == "config-dir" => {
            println!("{}", paths.base().display());
            Ok(0)
        }
        [command, subcommand] if command == "agent" && subcommand == "list" => {
            println!("codex\tMCP and optional PreToolUse hook");
            println!("claude\tMCP and optional PreToolUse hook");
            println!("gemini\tMCP and optional BeforeTool hook");
            println!("cursor\tMCP and optional beforeShellExecution hook");
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
            if command == "agent" && subcommand == "status" && agent == "codex" =>
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
        [command, subcommand, agent]
            if command == "agent"
                && subcommand == "install"
                && matches!(agent.as_str(), "claude" | "gemini" | "cursor") =>
        {
            crate::agents::portable::install(&paths, agent, false)?;
            Ok(0)
        }
        [command, subcommand, agent, hook]
            if command == "agent"
                && subcommand == "install"
                && matches!(agent.as_str(), "claude" | "gemini" | "cursor")
                && hook == "--hook" =>
        {
            crate::agents::portable::install(&paths, agent, true)?;
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent"
                && subcommand == "status"
                && matches!(agent.as_str(), "claude" | "gemini" | "cursor") =>
        {
            crate::agents::portable::print_status(&paths, agent)?;
            Ok(0)
        }
        [command, subcommand, agent]
            if command == "agent"
                && subcommand == "uninstall"
                && matches!(agent.as_str(), "claude" | "gemini" | "cursor") =>
        {
            crate::agents::portable::uninstall(&paths, agent, false)?;
            Ok(0)
        }
        [command, subcommand, agent, hook]
            if command == "agent"
                && subcommand == "uninstall"
                && matches!(agent.as_str(), "claude" | "gemini" | "cursor")
                && hook == "--hook" =>
        {
            crate::agents::portable::uninstall(&paths, agent, true)?;
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
            if command == "provider" && subcommand == "setup" =>
        {
            packages::setup(&paths, provider, setup)?;
            eprintln!("Setup {setup:?} applied to provider {provider:?}.");
            Ok(0)
        }
        [command, subcommand, provider] if command == "provider" && subcommand == "update" => {
            let installed = packages::update(&paths, provider).await?;
            eprintln!(
                "Provider {:?} updated to {}; rules.yaml was preserved.",
                installed.name, installed.package_version
            );
            Ok(0)
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
        [command, subcommand, tool] if command == "target" && subcommand == "status" => {
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
        [command, subcommand, tool, name, force]
            if command == "target" && subcommand == "remove" && force == "--force" =>
        {
            crate::targets::remove(&paths, tool, name, true)?;
            eprintln!("Target {name:?} removed from provider tool {tool:?}.");
            Ok(0)
        }
        _ => Err(Error::InvalidArguments(format!(
            "unknown command; run `torii --help`\n\n{}",
            help_text(&[]).expect("root help exists")
        ))),
    }
}

fn version_text() -> String {
    format!("torii {}", env!("CARGO_PKG_VERSION"))
}

fn help_request(args: &[String]) -> Option<&'static str> {
    if args.first().is_some_and(|arg| arg == "help") {
        return help_text(&args[1..]);
    }
    if args
        .last()
        .is_some_and(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return help_text(&args[..args.len() - 1]);
    }
    if args.len() >= 2 && args[1] == "help" {
        let mut command = Vec::with_capacity(args.len() - 1);
        command.push(args[0].clone());
        command.extend_from_slice(&args[2..]);
        return help_text(&command);
    }
    None
}

fn help_text(command: &[String]) -> Option<&'static str> {
    match command.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => Some("Torii is a controlled MCP execution boundary for infrastructure agents.\n\nUsage:\n  torii                         Start the MCP stdio server\n  torii <command> [arguments]   Run a human control-plane command\n\nCommands:\n  init                          Create the configuration root\n  reauth <tool> [target]        Renew managed authentication\n  provider <command>            Install and manage provider packages\n  target <command>              Manage aliases for target-aware providers\n  agent <command>               Configure an agent integration and optional hook\n  config-dir                    Print the configuration directory\n  --version, -V                 Print the Torii version\n\nHelp:\n  torii --help\n  torii <command> --help\n  torii help <command>\n\n`reauth` is a human-only control-plane command. An allowed MCP call prompts for managed authentication automatically when its session is unavailable."),
        ["reauth"] => Some("Usage:\n  torii reauth <provider-tool> [target]\n\nForces managed authentication and validates the candidate session before replacing the current one. Use the target argument only for target-aware tools, for example `torii reauth kubectl mpce_dev`. Providers using `inherited` authentication have no renewable material managed by Torii. An aws_profile target must be authenticated by a human through its configured AWS CLI profile instead; this command does not switch or renew that profile. This command is for a human and is never exposed through MCP."),
        ["provider"] => Some("Usage:\n  torii provider <command>\n\nCommands:\n  list                         List installed providers\n  search [query]               Search the configured catalog\n  install <source>             Install a provider package\n  setup <provider> <setup>     Apply a setup to an empty policy\n  update <provider>            Update package-managed files\n\nRun `torii provider <command> --help` for the command syntax."),
        ["provider", "list"] => Some("Usage:\n  torii provider list\n\nLists local provider tools, logical names, executables, package versions, and sources."),
        ["provider", "search"] => Some("Usage:\n  torii provider search [query]\n\nSearches the configured provider catalog. Omitting query lists all catalog entries."),
        ["provider", "install"] => Some("Usage:\n  torii provider install <name|directory|archive|https-url>\n\nInstalls a provider package without overwriting an existing provider directory. The installed policy starts empty (default deny)."),
        ["provider", "setup"] => Some("Usage:\n  torii provider setup <provider> <setup>\n\nApplies a package setup only when the provider rules are empty. Setup is the only package command that writes policy rules."),
        ["provider", "update"] => Some("Usage:\n  torii provider update <provider>\n\nUpdates package-managed files while preserving rules, environment, grants, targets, cache, and authentication."),
        ["target"] => Some("Usage:\n  torii target <command>\n\nCommands:\n  add <tool> <name> --context <context> --provider <tool> [--scope <scope>] [--expect <identity>]\n  add <tool> <name> --profile <aws-profile> --account-id <12-digit-id> [--region <region>]\n  activate <tool> <name> [--for <minutes>] [--add]\n  clear <tool>\n  status <tool>\n  list <tool>\n  show <tool> <name>\n  remove <tool> <name> --force\n\nTargets are human-managed aliases for target-aware provider tools. They are inactive by default and need a temporary human authorization before an agent can use them."),
        ["target", "add"] => Some("Usage:\n  torii target add <provider-tool> <name> --context <kubectl-context> --provider <identity-provider-tool> [--scope <scope>] [--expect <identity>]\n  torii target add <provider-tool> <name> --profile <aws-profile> --account-id <12-digit-id> [--region <region>]\n\nThe first form creates a kubectl_context alias authenticated by the given identity provider. --scope names the credential bucket (default: the target name, so targets stay isolated); --expect pins the identity checked by the provider's auth.identity probe before every call. The second form creates an aws_profile alias whose profile and expected account stay under human control."),
        ["target", "activate"] => Some("Usage:\n  torii target activate <provider-tool> <name> [--for <minutes>] [--add]\n\nTemporarily authorizes a configured target. Without `--add`, all other active targets for the provider are replaced. `--add` keeps them active too, which lets the agent choose any active target for operations allowed by policy. Duration must be 1 to 1440 minutes."),
        ["target", "clear"] => Some("Usage:\n  torii target clear <provider-tool>\n\nClears every temporary target authorization for the provider. It does not remove policy grants, credentials, configuration, or a process that already started."),
        ["target", "status"] => Some("Usage:\n  torii target status <provider-tool>\n\nLists active and inactive targets, expiry, and remaining authorization time. A warning is printed when more than one target is active."),
        ["target", "list"] => Some("Usage:\n  torii target list <provider-tool>\n\nLists aliases and their fixed bindings in the human control plane."),
        ["target", "show"] => Some("Usage:\n  torii target show <provider-tool> <name>\n\nPrints the target configuration."),
        ["target", "remove"] => Some("Usage:\n  torii target remove <provider-tool> <name> --force\n\nRevokes the target authorization and removes the target and its isolated state. `--force` is required."),
        ["agent"] => Some("Usage:\n  torii agent <command>\n\nCommands:\n  list\n  install <codex|claude|gemini|cursor> [--hook]\n  status <codex|claude|gemini|cursor>\n  uninstall <codex|claude|gemini|cursor> [--hook]\n\nThe optional hook redirects direct provider CLI attempts to the corresponding MCP tool."),
        ["agent", "list"] => Some("Usage:\n  torii agent list\n\nLists the implemented agent adapters."),
        ["agent", "install"] => Some("Usage:\n  torii agent install <codex|claude|gemini|cursor> [--hook]\n\nRegisters the Torii MCP server in the selected agent configuration. `--hook` also installs the direct-provider CLI guard."),
        ["agent", "status"] => Some("Usage:\n  torii agent status <codex|claude|gemini|cursor>\n\nShows whether the MCP integration and hook are installed and managed by this Torii configuration."),
        ["agent", "uninstall"] => Some("Usage:\n  torii agent uninstall <codex|claude|gemini|cursor> [--hook]\n\nRemoves the managed integration, or only its hook with `--hook`."),
        ["init"] => Some("Usage:\n  torii init\n\nCreates the configuration root and default settings without installing providers."),
        ["config-dir"] => Some("Usage:\n  torii config-dir\n\nPrints the active Torii configuration directory."),
        _ => None,
    }
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
        paths,
        &auth_provider,
        &auth_paths,
        auth_lock.as_ref(),
        &audit_scope,
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
    use super::{help_request, parse_target_activation_options, reauth, version_text};
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
    fn help_accepts_standard_and_layered_forms() {
        assert!(help_request(&["--help".into()]).unwrap().contains("reauth"));
        assert!(help_request(&["provider".into(), "--help".into()])
            .unwrap()
            .contains("install"));
        assert!(
            help_request(&["provider".into(), "help".into(), "install".into()])
                .unwrap()
                .contains("empty")
        );
        assert!(
            help_request(&["target".into(), "add".into(), "--help".into()])
                .unwrap()
                .contains("--profile")
        );
        assert!(
            help_request(&["target".into(), "activate".into(), "--help".into()])
                .unwrap()
                .contains("--add")
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
