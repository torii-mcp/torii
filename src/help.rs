//! Help for the control-plane CLI.
//!
//! Every command answers `--help`, `-h` and `help`, before or after its words:
//! `torii help config theme`, `torii config help theme`, `torii config theme help`
//! and `torii config theme --help` print the same text. A command line that
//! does not parse prints the usage of the nearest command instead of failing
//! without one.

/// Commands that run nothing without more words, so alone they print their help.
const NEEDS_MORE: &[&[&str]] = &[
    &["provider"],
    &["target"],
    &["agent"],
    &["policy"],
    &["self"],
    &["reauth"],
    &["config"],
    &["config", "dir"],
    &["config", "theme"],
    &["config", "folded-bar-color"],
    &["config", "fbc"],
    &["config", "log-retention"],
    &["config", "log-tail"],
    &["config", "log-output"],
    &["log"],
];

/// The help the command line asks for, if it asks for one.
pub fn request(args: &[String]) -> Option<&'static str> {
    let words = args.iter().map(String::as_str).collect::<Vec<_>>();
    match words.as_slice() {
        ["help", rest @ ..] => Some(nearest(rest)),
        // A flag cannot be a value, so it also asks for help after values:
        // `torii config fbc salvia --help`.
        [rest @ .., "--help" | "-h"] => Some(nearest(rest)),
        // A trailing `help` can be a value, such as a target named `help`, so
        // it only asks when those exact words have a help of their own.
        [rest @ .., "help"] => text(rest),
        [command, "help", rest @ ..] => {
            let mut words = vec![*command];
            words.extend_from_slice(rest);
            Some(nearest(&words))
        }
        words if NEEDS_MORE.contains(&words) => text(words),
        _ => None,
    }
}

/// The help of the longest leading words that have one, down to the general
/// help: `config theme nord extra` gets the help of `config theme`.
pub fn nearest(words: &[&str]) -> &'static str {
    (0..=words.len())
        .rev()
        .find_map(|count| text(&words[..count]))
        .expect("the general help exists")
}

fn text(command: &[&str]) -> Option<&'static str> {
    match command {
        [] => Some("Torii is a controlled MCP execution boundary for infrastructure agents.\n\nUsage:\n  torii                         Start the MCP stdio server\n  torii <command> [arguments]   Run a human control-plane command\n\nCommands:\n  init                          Create the configuration root\n  reauth <tool> [target]        Renew managed authentication\n  provider <command>            Install and manage provider packages\n  policy <command>              Show or edit a provider or target policy\n  self upgrade [--check]         Upgrade the Torii binary itself\n  target <command>              Manage aliases for target-aware providers\n  agent <command>               Configure an agent integration and optional hook\n  config <command>              Show or change preferences such as the theme\n  log <command>                 Follow, search and export the audit log\n  --version, -V                 Print the Torii version\n\nHelp:\n  torii --help\n  torii <command> --help\n  torii <command> help\n  torii help <command>\n\n`reauth` is a human-only control-plane command. An allowed MCP call prompts for managed authentication automatically when its session is unavailable."),
        ["reauth"] => Some("Usage:\n  torii reauth <provider-tool> [target]\n\nForces managed authentication and validates the candidate session before replacing the current one. Use the target argument only for target-aware tools, for example `torii reauth kubectl mpce_dev`. Providers using `inherited` authentication have no renewable material managed by Torii. An aws_profile target must be authenticated by a human through its configured AWS CLI profile instead; this command does not switch or renew that profile. This command is for a human and is never exposed through MCP."),
        ["provider"] => Some("Usage:\n  torii provider <command>\n\nCommands:\n  list                         List installed providers\n  search [query]               Search the configured catalog\n  install <source>             Install a provider package\n  setup <provider> <setup>     Apply a setup to an empty policy (alias: apply)\n  upgrade <provider>            Upgrade package-managed files\n\nRun `torii provider <command> --help` for the command syntax."),
        ["provider", "list"] => Some("Usage:\n  torii provider list\n\nLists local provider tools, logical names, executables, package versions, and sources."),
        ["provider", "search"] => Some("Usage:\n  torii provider search [query]\n\nSearches the configured provider catalog. Omitting query lists all catalog entries."),
        ["provider", "install"] => Some("Usage:\n  torii provider install <name|directory|archive|https-url>\n\nInstalls a provider package without overwriting an existing provider directory. The installed policy starts empty (default deny)."),
        ["provider", "setup" | "apply"] => Some("Usage:\n  torii provider setup <provider> <setup>\n  torii provider apply <provider> <setup>\n\nApplies a package setup only when the provider rules are empty. Setup is the only package command that writes policy rules."),
        ["provider", "upgrade"] => Some("Usage:\n  torii provider upgrade <provider>\n\nUpgrades package-managed files from the source recorded in the lock, while preserving rules, environment, grants, targets, cache, and authentication."),
        ["target"] => Some("Usage:\n  torii target <command>\n\nCommands:\n  add <tool> <name> --context <context> --provider <tool> [--scope <scope>] [--expect <identity>]\n  add <tool> <name> --profile <aws-profile> --account-id <12-digit-id> [--region <region>]\n  activate <tool> <name> [--for <minutes>] [--add]\n  clear <tool>\n  status <tool>   (alias: check)\n  list <tool>\n  show <tool> <name>\n  rename <tool> <name> <new-name>\n  remove <tool> <name> --force\n\nTargets are human-managed aliases for target-aware provider tools. They are inactive by default and need a temporary human authorization before an agent can use them."),
        ["target", "add"] => Some("Usage:\n  torii target add <provider-tool> <name> --context <kubectl-context> --provider <identity-provider-tool> [--scope <scope>] [--expect <identity>]\n  torii target add <provider-tool> <name> --profile <aws-profile> --account-id <12-digit-id> [--region <region>]\n\nThe first form creates a kubectl_context alias authenticated by the given identity provider. --scope names the credential bucket (default: the target name, so targets stay isolated); --expect pins the identity checked by the provider's auth.identity probe before every call. The second form creates an aws_profile alias whose profile and expected account stay under human control."),
        ["target", "activate"] => Some("Usage:\n  torii target activate <provider-tool> <name> [--for <minutes>] [--add]\n\nTemporarily authorizes a configured target. Without `--add`, all other active targets for the provider are replaced. `--add` keeps them active too, which lets the agent choose any active target for operations allowed by policy. Duration must be 1 to 1440 minutes."),
        ["target", "clear"] => Some("Usage:\n  torii target clear <provider-tool>\n\nClears every temporary target authorization for the provider. It does not remove policy grants, credentials, configuration, or a process that already started."),
        ["target", "status" | "check"] => Some("Usage:\n  torii target status <provider-tool>\n  torii target check <provider-tool>\n\nLists active and inactive targets, expiry, and remaining authorization time. A warning is printed when more than one target is active."),
        ["target", "list"] => Some("Usage:\n  torii target list <provider-tool>\n\nLists aliases and their fixed bindings in the human control plane."),
        ["target", "show"] => Some("Usage:\n  torii target show <provider-tool> <name>\n\nPrints the target configuration."),
        ["target", "rename"] => Some("Usage:\n  torii target rename <provider-tool> <name> <new-name>\n\nRenames an alias, keeping its binding, policy, grants and environment. A target without an explicit --scope has its credential bucket pinned to the old name, so the authenticated session survives. The temporary authorization is revoked: the renamed alias starts inactive. Restart running agent clients so the MCP schema announces the new name."),
        ["target", "remove"] => Some("Usage:\n  torii target remove <provider-tool> <name> --force\n\nRevokes the target authorization and removes the target and its isolated state. `--force` is required."),
        ["agent"] => Some("Usage:\n  torii agent <command>\n\nCommands:\n  list\n  install <agent> [--hook] [--yes]\n  status <agent>                (alias: check)\n  uninstall <agent> [--hook]\n\n<agent> is codex, claude, gemini, cursor, antigravity, opencode, copilot, copilot-cli, kiro, or pi. Run `torii agent list` for what each adapter supports. The optional hook redirects direct provider CLI attempts to the corresponding MCP tool and exists only for agents whose pre-execution hook Torii manages."),
        ["self"] | ["self", "upgrade"] => Some("Usage:
  torii self upgrade [--check]

Downloads the latest published release for this platform, verifies its SHA-256 and replaces the running binary. `--check` only reports whether a newer release exists.

Providers, policy, targets, grants and credentials are untouched. Restart any agent client still running the previous binary. The published checksum guards against a corrupted download; it is not a signature."),
        ["policy"] => Some("Usage:\n  torii policy <command>\n\nCommands:\n  show <tool> [target]              Print the active policy file\n  edit <tool> [target] [--create]   Edit the policy in $VISUAL/$EDITOR\n\nPolicy is a human control-plane concern and is never exposed as an MCP tool. Rules are reread on every call, so changing them needs no restart."),
        ["policy", "show"] => Some("Usage:\n  torii policy show <provider-tool> [target]\n\nPrints the active rules file and warns about accept rules ignored for being below minimum_accept_tokens. With a target, prints the target policy that replaces the shared one, or says the shared policy applies."),
        ["policy", "edit"] => Some("Usage:\n  torii policy edit <provider-tool> [target] [--create]\n\nOpens the rules file in $VISUAL or $EDITOR (notepad on Windows, vi elsewhere) through a draft copy. The draft is parsed and every rule is compiled before it replaces the live file, so an invalid regex or malformed YAML never reaches the policy. An invalid draft is kept so no edit is lost. `--create` starts a target policy that replaces the shared one for that alias.\n\nAn editor that returns immediately must be told to wait, for example `code --wait` or `subl -w`."),
        ["agent", "list"] => Some("Usage:\n  torii agent list\n\nLists the implemented agent adapters and whether each one supports the hook."),
        ["agent", "install"] => Some("Usage:\n  torii agent install <agent> [--hook] [--yes]\n\nRegisters the Torii MCP server in the selected agent configuration. `--hook` also installs the direct-provider CLI guard and is rejected for agents without a pre-execution hook. `--yes` confirms an adapter that needs acknowledgement, such as pi, whose MCP support depends on an extension installed by the human."),
        ["agent", "status" | "check"] => Some("Usage:\n  torii agent status <agent>\n  torii agent check <agent>\n\nShows whether the MCP integration and hook are installed and managed by this Torii configuration. Agents without a pre-execution hook report the hook as unsupported."),
        ["agent", "uninstall"] => Some("Usage:\n  torii agent uninstall <agent> [--hook]\n\nRemoves the managed integration, or only its hook with `--hook`."),
        ["init"] => Some("Usage:\n  torii init\n\nCreates the configuration root and default settings without installing providers."),
        ["config"] => Some("Usage:\n  torii config <command>\n\nCommands:\n  list                      Show each preference, its value and its default\n  dir show                  Print the configuration directory\n  theme <verb>              Theme of the prompt windows: list, set, reset, test\n  folded-bar-color <verb>   Folded bar color, alias fbc: list, set, reset, test\n  log-retention <verb>      Days the audit log keeps records: list, set, reset\n  log-tail <verb>           Records `torii log tail` shows: list, set, reset\n  log-output <verb>         Whether a call's output is kept: list, set, reset\n\nPreferences live in settings.yaml. Prompt windows read them when they open, so a change needs no restart."),
        ["config", "list"] => Some("Usage:\n  torii config list\n\nShows each preference, its current value and its default. `ls` also works."),
        ["config", "dir"] | ["config", "dir", "show"] => Some("Usage:\n  torii config dir show\n\nPrints the active Torii configuration directory."),
        ["config", "theme"] => Some("Usage:\n  torii config theme list\n  torii config theme set <theme>\n  torii config theme reset\n  torii config theme test [<theme>]\n\nTheme of the prompt windows: dark (the default), light or nord. `list` marks the default and the one in use, `set` saves a theme, `reset` restores dark and `test` opens a sample authorization window in the given or the saved theme."),
        ["config", "theme", "list"] => Some("Usage:\n  torii config theme list\n\nLists the themes and marks the default and the one in use. `ls` also works."),
        ["config", "theme", "set"] => Some("Usage:\n  torii config theme set <theme>\n\nSaves the theme of the prompt windows: dark, light or nord. The next window uses it."),
        ["config", "theme", "reset"] => Some("Usage:\n  torii config theme reset\n\nRestores the default theme, dark."),
        ["config", "theme", "test"] => Some("Usage:\n  torii config theme test [<theme>]\n\nOpens a sample authorization window in <theme>, or in the saved theme, without saving anything. The request is a joke and nothing runs, whatever you answer."),
        ["config", "folded-bar-color" | "fbc"] => Some("Usage:\n  torii config folded-bar-color list\n  torii config folded-bar-color set <color>\n  torii config folded-bar-color reset\n  torii config folded-bar-color test [<color>]\n\n`fbc` is the short form of `folded-bar-color`.\n\nColor of a prompt window's title bar while the window is folded, so a parked window is easy to find on the screen. A color is a name from `list` (case and accents are ignored), `theme` for the theme's own title bar, or #rrggbb. The default is rosa-antigo. The text and border of the bar follow the color, so any value stays readable."),
        ["config", "folded-bar-color" | "fbc", "list"] => Some("Usage:\n  torii config folded-bar-color list\n\nLists the named colors and marks the default and the one in use. `ls` also works."),
        ["config", "folded-bar-color" | "fbc", "set"] => Some("Usage:\n  torii config folded-bar-color set <color>\n\nSaves the folded bar color: a name from `list`, `theme` or #rrggbb. In PowerShell, quote a #rrggbb value: `#` starts a comment."),
        ["config", "folded-bar-color" | "fbc", "reset"] => Some("Usage:\n  torii config folded-bar-color reset\n\nRestores the default folded bar color, rosa-antigo."),
        ["config", "folded-bar-color" | "fbc", "test"] => Some("Usage:\n  torii config folded-bar-color test [<color>]\n\nOpens a sample authorization window in the saved theme with <color>, or the saved color, on its folded bar, without saving anything. Fold the window to see the color."),
        ["log"] => Some("Usage:\n  torii log <command>\n\nCommands:\n  tail [-n <count>] [--follow]          Latest records, one line each; --follow keeps watching\n  search [filters] [-n <count>]         Newest records that match the filters\n  show <id> [--output]                  Everything about one record, its output with --output\n  export [filters] [--format json|md] [--file <path>] [--with-output]\n  prune [days]                          Drop records older than days, or than the retention\n\nEvery agent call is recorded: when, provider, target, the call, the agent's reason, the verdict and how it was reached, the steps, and what it returned. Human commands that change state are recorded too. The log is a SQLite database with every row sealed by a key kept in this machine's credential store, so a copied disk does not read without it. Credentials and clipboard contents are never recorded."),
        ["log", "tail"] => Some("Usage:\n  torii log tail [-n <count>] [--follow]\n\nPrints the latest records, oldest first, one line each: time, record number, scope, verdict, how it was reached, exit code, duration and the call. Without -n it shows `torii config log-tail` records (20 by default). --follow (-f) keeps watching: a call prints when it arrives, so one waiting for your answer shows up, and again when it ends. Stop with Ctrl+C."),
        ["log", "search"] => Some("Usage:\n  torii log search [filters] [-n <count>]\n\nFilters:\n  --provider <tool>       --target <name>\n  --allowed  --denied  --errors  --running   (any of them)\n  --agent    --human     agent calls or human commands only\n  --since <when>          --until <when>      30m, 2h, 7d, 2026-10-20, 2026-10-20T14:30\n  --text <words>          in the call, the agent's reason, the rule or the error\n\nPrints the newest matching records, up to -n or `torii config log-tail`. Filters combine; outcome flags add up."),
        ["log", "show"] => Some("Usage:\n  torii log show <id> [--output]\n\nEverything about one record: times, provider, target, the call as sent, the agent's reason, the verdict, the rule or grant behind it, every step with its time, and the result. --output also prints the stdout and stderr the call returned, when `log-output` kept them."),
        ["log", "export"] => Some("Usage:\n  torii log export [filters] [--format json|md] [--file <path>] [--with-output]\n\nFilters:\n  --provider <tool>       --target <name>\n  --allowed  --denied  --errors  --running   (any of them)\n  --agent    --human     agent calls or human commands only\n  --since <when>          --until <when>      30m, 2h, 7d, 2026-10-20, 2026-10-20T14:30\n  --text <words>          in the call, the agent's reason, the rule or the error\n\nWrites the matching records with their steps, oldest first, as JSON (the default) or Markdown, to <path> or to stdout. --with-output includes what each call returned. The export is plain text, not encrypted: keep it somewhere safe."),
        ["log", "prune"] => Some("Usage:\n  torii log prune [days]\n\nKeeps only the last <days> days of records, or the retention from `torii config log-retention` (90 by default) without a number. Every MCP server also prunes by the retention when it starts."),
        ["config", "log-retention"] | ["config", "log-retention", "list"] => Some("Usage:\n  torii config log-retention list\n  torii config log-retention set <days>\n  torii config log-retention reset\n\nDays the audit log keeps a record, from 1 to 3650; 90 by default. Every MCP server prunes by it when it starts, and `torii log prune` uses it without a number."),
        ["config", "log-retention", "set"] => Some("Usage:\n  torii config log-retention set <days>\n\nSaves how many days the audit log keeps, from 1 to 3650."),
        ["config", "log-retention", "reset"] => Some("Usage:\n  torii config log-retention reset\n\nRestores the default retention, 90 days."),
        ["config", "log-tail"] | ["config", "log-tail", "list"] => Some("Usage:\n  torii config log-tail list\n  torii config log-tail set <count>\n  torii config log-tail reset\n\nHow many records `torii log tail` and `torii log search` show without -n; 20 by default."),
        ["config", "log-tail", "set"] => Some("Usage:\n  torii config log-tail set <count>\n\nSaves how many records `torii log tail` shows by default, from 1 to 10000."),
        ["config", "log-tail", "reset"] => Some("Usage:\n  torii config log-tail reset\n\nRestores the default, 20 records."),
        ["config", "log-output"] | ["config", "log-output", "list"] => Some("Usage:\n  torii config log-output list\n  torii config log-output set full|none\n  torii config log-output reset\n\nWhether the audit record of a call keeps the stdout and stderr it returned: full (the default) keeps them compressed and sealed, none keeps only sizes and exit code."),
        ["config", "log-output", "set"] => Some("Usage:\n  torii config log-output set full|none\n\nSaves whether audit records keep what a call returned. Running MCP servers apply it when they restart."),
        ["config", "log-output", "reset"] => Some("Usage:\n  torii config log-output reset\n\nRestores the default, full."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{nearest, request, text};

    /// Every command the router accepts. A new command goes here with its help.
    const COMMANDS: &[&[&str]] = &[
        &[],
        &["init"],
        &["reauth"],
        &["provider"],
        &["provider", "list"],
        &["provider", "search"],
        &["provider", "install"],
        &["provider", "setup"],
        &["provider", "apply"],
        &["provider", "upgrade"],
        &["policy"],
        &["policy", "show"],
        &["policy", "edit"],
        &["target"],
        &["target", "add"],
        &["target", "activate"],
        &["target", "clear"],
        &["target", "status"],
        &["target", "check"],
        &["target", "list"],
        &["target", "show"],
        &["target", "rename"],
        &["target", "remove"],
        &["agent"],
        &["agent", "list"],
        &["agent", "install"],
        &["agent", "status"],
        &["agent", "check"],
        &["agent", "uninstall"],
        &["self"],
        &["self", "upgrade"],
        &["config"],
        &["config", "list"],
        &["config", "dir"],
        &["config", "dir", "show"],
        &["config", "theme"],
        &["config", "theme", "list"],
        &["config", "theme", "set"],
        &["config", "theme", "reset"],
        &["config", "theme", "test"],
        &["config", "folded-bar-color"],
        &["config", "folded-bar-color", "list"],
        &["config", "folded-bar-color", "set"],
        &["config", "folded-bar-color", "reset"],
        &["config", "folded-bar-color", "test"],
        &["config", "fbc"],
        &["config", "fbc", "list"],
        &["config", "fbc", "set"],
        &["config", "fbc", "reset"],
        &["config", "fbc", "test"],
        &["config", "log-retention"],
        &["config", "log-retention", "list"],
        &["config", "log-retention", "set"],
        &["config", "log-retention", "reset"],
        &["config", "log-tail"],
        &["config", "log-tail", "list"],
        &["config", "log-tail", "set"],
        &["config", "log-tail", "reset"],
        &["config", "log-output"],
        &["config", "log-output", "list"],
        &["config", "log-output", "set"],
        &["config", "log-output", "reset"],
        &["log"],
        &["log", "tail"],
        &["log", "search"],
        &["log", "show"],
        &["log", "export"],
        &["log", "prune"],
    ];

    fn ask(words: &[&str]) -> Option<&'static str> {
        request(
            &words
                .iter()
                .map(|word| word.to_string())
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn every_command_has_its_own_help() {
        for command in COMMANDS {
            assert!(text(command).is_some(), "no help for {command:?}");
        }
    }

    #[test]
    fn every_command_answers_every_form_of_help() {
        for &command in COMMANDS {
            let expected = text(command);
            assert_eq!(
                ask(&[&["help"], command].concat()),
                expected,
                "help {command:?}"
            );
            for flag in ["--help", "-h", "help"] {
                assert_eq!(
                    ask(&[command, &[flag]].concat()),
                    expected,
                    "{command:?} {flag}"
                );
            }
            if let [group, rest @ ..] = command {
                let layered = [&[*group, "help"], rest].concat();
                assert_eq!(ask(&layered), expected, "{layered:?}");
            }
        }
    }

    #[test]
    fn a_help_flag_after_values_shows_the_command() {
        assert_eq!(
            ask(&["config", "fbc", "set", "salvia", "--help"]),
            text(&["config", "fbc", "set"])
        );
        assert_eq!(
            ask(&["target", "show", "kubectl", "dev", "-h"]),
            text(&["target", "show"])
        );
    }

    #[test]
    fn a_trailing_help_word_can_still_be_a_value() {
        assert_eq!(ask(&["target", "show", "kubectl", "help"]), None);
    }

    #[test]
    fn a_group_alone_prints_its_help_and_a_runnable_command_runs() {
        assert_eq!(ask(&["config"]), text(&["config"]));
        assert_eq!(ask(&["config", "theme"]), text(&["config", "theme"]));
        assert_eq!(ask(&["config", "dir"]), text(&["config", "dir"]));
        assert_eq!(ask(&["config", "theme", "list"]), None);
        assert_eq!(ask(&["target"]), text(&["target"]));
        assert_eq!(ask(&["init"]), None);
        assert_eq!(ask(&["config", "list"]), None);
    }

    #[test]
    fn an_unknown_command_line_falls_back_to_the_nearest_help() {
        assert_eq!(
            nearest(&["config", "theme", "set", "nord", "extra"]),
            text(&["config", "theme", "set"]).unwrap()
        );
        assert_eq!(nearest(&["nope"]), text(&[]).unwrap());
    }

    #[test]
    fn layered_help_reaches_the_subcommand() {
        assert!(ask(&["provider", "help", "install"])
            .unwrap()
            .contains("empty"));
        assert!(ask(&["target", "add", "--help"])
            .unwrap()
            .contains("--profile"));
        assert!(ask(&["target", "activate", "--help"])
            .unwrap()
            .contains("--add"));
    }
}
