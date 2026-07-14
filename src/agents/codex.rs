use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::config::ConfigPaths;
use crate::error::{Error, Result};
use crate::providers::ProviderRegistry;

const STATE_VERSION: u32 = 1;
const HOOK_MARKER: &str = "Checking Torii provider boundary";
const MAX_HOOK_INPUT: u64 = 1024 * 1024;

#[derive(Debug, Default, Serialize, Deserialize)]
struct ManagedState {
    version: u32,
    codex_home: PathBuf,
    mcp_owned: bool,
    mcp_command: String,
    torii_config_dir: String,
    hook_owned: bool,
    hook_command: String,
    hook_command_windows: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Status {
    pub codex_home: PathBuf,
    pub mcp_installed: bool,
    pub mcp_managed: bool,
    pub hook_installed: bool,
    pub hook_managed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShellToken {
    Word(String),
    Separator,
}

pub fn install(paths: &ConfigPaths, with_hook: bool) -> Result<()> {
    let codex_home = discover_codex_home()?;
    let executable = current_executable()?;
    install_at(paths, &codex_home, &executable, with_hook)?;
    eprintln!(
        "Codex MCP integration installed at {}{}.",
        codex_home.display(),
        if with_hook {
            " with the Torii PreToolUse hook"
        } else {
            ""
        }
    );
    eprintln!("Restart Codex to load the integration.");
    Ok(())
}

pub fn print_status(paths: &ConfigPaths) -> Result<()> {
    let status = status_at(paths, None)?;
    println!("codex_home\t{}", status.codex_home.display());
    println!(
        "mcp\t{}",
        status_label(status.mcp_installed, status.mcp_managed)
    );
    println!(
        "hook\t{}",
        status_label(status.hook_installed, status.hook_managed)
    );
    Ok(())
}

pub fn uninstall(paths: &ConfigPaths, hook_only: bool) -> Result<()> {
    uninstall_at(paths, hook_only)?;
    if hook_only {
        eprintln!("Torii Codex hook removed; MCP integration was preserved.");
    } else {
        eprintln!("Torii Codex integration removed.");
    }
    eprintln!("Restart Codex to reload its configuration.");
    Ok(())
}

pub fn run_hook(paths: &ConfigPaths, agent: &str) -> Result<i32> {
    let mut input = String::new();
    std::io::stdin()
        .take(MAX_HOOK_INPUT)
        .read_to_string(&mut input)
        .map_err(|error| Error::Agent(format!("failed to read {agent} hook input: {error}")))?;

    let hook: Value = match serde_json::from_str(&input) {
        Ok(hook) => hook,
        Err(error) => {
            print_denial(
                agent,
                format!(
                    "Torii could not validate this shell call because the {agent} hook input was invalid: {error}"
                ),
            )?;
            return Ok(0);
        }
    };
    let Some(command) = hook_command_from_input(agent, &hook) else {
        if is_relevant_hook_event(agent, &hook) {
            print_denial(
                agent,
                "Torii could not validate this shell call because it had no command.".into(),
            )?;
        }
        return Ok(0);
    };

    let registry = match ProviderRegistry::load(paths) {
        Ok(registry) => registry,
        Err(error) => {
            print_denial(
                agent,
                format!(
                    "Torii could not load its provider registry, so this shell call was blocked: {error}"
                ),
            )?;
            return Ok(0);
        }
    };
    let providers = registry
        .providers()
        .map(|provider| {
            (
                executable_key(&provider.config.command),
                (provider.config.tool.clone(), provider.uses_targets()),
            )
        })
        .filter(|(command, _)| !command.is_empty())
        .collect::<BTreeMap<_, _>>();

    if let Some((tool, targeted)) = guarded_provider(&command, &providers) {
        let arguments = if *targeted {
            r#"{"target":"<announced-alias>","args":["..."]}"#
        } else {
            r#"{"args":["..."]}"#
        };
        print_denial(
            agent,
            format!(
                "Direct execution of a Torii provider is blocked. Use the Torii MCP tool {tool:?} with {arguments}. Pass only the arguments after the executable and do not retry through another shell or an absolute path."
            ),
        )?;
    }
    Ok(0)
}

fn is_relevant_hook_event(agent: &str, hook: &Value) -> bool {
    let event = hook.get("hook_event_name").and_then(Value::as_str);
    let tool = hook.get("tool_name").and_then(Value::as_str);
    match agent {
        "codex" | "claude" => event == Some("PreToolUse") && tool == Some("Bash"),
        "gemini" => event == Some("BeforeTool") && tool == Some("run_shell_command"),
        "cursor" => event == Some("beforeShellExecution"),
        _ => false,
    }
}

fn hook_command_from_input(agent: &str, hook: &Value) -> Option<String> {
    if !is_relevant_hook_event(agent, hook) {
        return None;
    }
    let command = if agent == "cursor" {
        hook.get("command")
    } else {
        hook.get("tool_input")
            .and_then(|input| input.get("command"))
    }?;
    match command {
        Value::String(command) => Some(command.clone()),
        Value::Array(commands) if agent == "gemini" => {
            let commands = commands
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>();
            (!commands.is_empty()).then(|| commands.join(" && "))
        }
        _ => None,
    }
}

fn install_at(
    paths: &ConfigPaths,
    codex_home: &Path,
    executable: &Path,
    with_hook: bool,
) -> Result<()> {
    paths.ensure()?;
    create_dir(codex_home)?;
    let state_path = state_path(paths);
    let mut state = read_state(&state_path)?.unwrap_or_default();
    if state.version != 0 && state.version != STATE_VERSION {
        return Err(Error::Agent(format!(
            "unsupported Codex integration state version {}",
            state.version
        )));
    }
    if !state.codex_home.as_os_str().is_empty() && state.codex_home != codex_home {
        return Err(Error::Agent(format!(
            "this Torii configuration already manages Codex at {}; uninstall it before changing CODEX_HOME",
            state.codex_home.display()
        )));
    }

    let mcp_command = path_string(executable)?;
    let config_dir = path_string(paths.base())?;
    let config_path = codex_home.join("config.toml");
    let config_source = read_optional(&config_path)?.unwrap_or_default();
    let mut config = parse_toml(&config_path, &config_source)?;
    let desired_mcp = McpEntry {
        command: mcp_command.clone(),
        config_dir: config_dir.clone(),
    };
    let previous_mcp = McpEntry {
        command: state.mcp_command.clone(),
        config_dir: state.torii_config_dir.clone(),
    };
    match read_mcp_entry(&config)? {
        None => {
            write_mcp_entry(&mut config, &desired_mcp)?;
            state.mcp_owned = true;
        }
        Some(existing) if existing == desired_mcp => {
            write_mcp_entry(&mut config, &desired_mcp)?;
        }
        Some(existing) if state.mcp_owned && existing == previous_mcp => {
            write_mcp_entry(&mut config, &desired_mcp)?;
        }
        Some(_) => {
            return Err(Error::Agent(
                "Codex already has a conflicting MCP server named \"torii\"; remove or rename it before installing".into(),
            ));
        }
    }

    let hook_command_unix = hook_command(executable, paths.base(), false)?;
    let hook_command_windows = hook_command(executable, paths.base(), true)?;
    let hooks_path = codex_home.join("hooks.json");
    let mut hooks_to_write = None;
    if with_hook {
        let hooks_source = read_optional(&hooks_path)?.unwrap_or_else(|| "{}".into());
        let mut hooks = parse_hooks(&hooks_path, &hooks_source)?;
        if state.hook_owned
            && remove_stale_hooks(&mut hooks, &hook_command_unix, &hook_command_windows)?
        {
            hooks_to_write = Some(format_json(&hooks)?);
        }
        if !has_hook(&hooks, &hook_command_unix, &hook_command_windows) {
            add_hook(&mut hooks, &hook_command_unix, &hook_command_windows)?;
            hooks_to_write = Some(format_json(&hooks)?);
            state.hook_owned = true;
        }
    }

    state.version = STATE_VERSION;
    state.codex_home = codex_home.to_path_buf();
    state.mcp_command = mcp_command;
    state.torii_config_dir = config_dir;
    state.hook_command = hook_command_unix;
    state.hook_command_windows = hook_command_windows;

    write_atomic(&config_path, config.to_string().as_bytes())?;
    if let Some(hooks) = hooks_to_write {
        write_atomic(&hooks_path, hooks.as_bytes())?;
    }
    write_atomic(&state_path, format_json(&state)?.as_bytes())?;
    Ok(())
}

fn uninstall_at(paths: &ConfigPaths, hook_only: bool) -> Result<()> {
    let state_path = state_path(paths);
    let Some(mut state) = read_state(&state_path)? else {
        return Err(Error::Agent(
            "no Torii-managed Codex integration was found for this configuration".into(),
        ));
    };
    let config_path = state.codex_home.join("config.toml");
    let hooks_path = state.codex_home.join("hooks.json");

    let mut config_to_write = None;
    if !hook_only && state.mcp_owned {
        let source = read_optional(&config_path)?.unwrap_or_default();
        let mut config = parse_toml(&config_path, &source)?;
        let expected = McpEntry {
            command: state.mcp_command.clone(),
            config_dir: state.torii_config_dir.clone(),
        };
        match read_mcp_entry(&config)? {
            Some(existing) if existing == expected => {
                remove_mcp_entry(&mut config)?;
                config_to_write = Some(config.to_string());
            }
            None => {}
            Some(_) => {
                return Err(Error::Agent(
                    "the managed Codex MCP entry was modified; refusing to remove it".into(),
                ));
            }
        }
        state.mcp_owned = false;
    }

    let mut hooks_to_write = None;
    if state.hook_owned {
        let source = read_optional(&hooks_path)?.unwrap_or_else(|| "{}".into());
        let mut hooks = parse_hooks(&hooks_path, &source)?;
        remove_hook(&mut hooks, &state.hook_command, &state.hook_command_windows)?;
        hooks_to_write = Some(format_json(&hooks)?);
        state.hook_owned = false;
    }

    if let Some(config) = config_to_write {
        write_atomic(&config_path, config.as_bytes())?;
    }
    if let Some(hooks) = hooks_to_write {
        write_atomic(&hooks_path, hooks.as_bytes())?;
    }
    if hook_only || !state.mcp_owned {
        if state.mcp_owned || state.hook_owned {
            write_atomic(&state_path, format_json(&state)?.as_bytes())?;
        } else if state_path.exists() {
            std::fs::remove_file(&state_path).map_err(|source| Error::Write {
                path: state_path,
                source,
            })?;
        }
    }
    Ok(())
}

fn status_at(paths: &ConfigPaths, codex_home: Option<&Path>) -> Result<Status> {
    let state = read_state(&state_path(paths))?;
    let home = codex_home
        .map(Path::to_path_buf)
        .or_else(|| state.as_ref().map(|state| state.codex_home.clone()))
        .map(Ok)
        .unwrap_or_else(discover_codex_home)?;
    let config_path = home.join("config.toml");
    let config = parse_toml(
        &config_path,
        &read_optional(&config_path)?.unwrap_or_default(),
    )?;
    let mcp_installed = read_mcp_entry(&config)?.is_some();
    let hooks_path = home.join("hooks.json");
    let hooks = parse_hooks(
        &hooks_path,
        &read_optional(&hooks_path)?.unwrap_or_else(|| "{}".into()),
    )?;
    let hook_installed = has_any_torii_hook(&hooks);
    Ok(Status {
        codex_home: home,
        mcp_installed,
        mcp_managed: mcp_installed && state.as_ref().is_some_and(|state| state.mcp_owned),
        hook_installed,
        hook_managed: hook_installed && state.as_ref().is_some_and(|state| state.hook_owned),
    })
}

#[derive(Debug, PartialEq, Eq)]
struct McpEntry {
    command: String,
    config_dir: String,
}

fn read_mcp_entry(document: &DocumentMut) -> Result<Option<McpEntry>> {
    let Some(servers) = document.get("mcp_servers") else {
        return Ok(None);
    };
    let servers = servers.as_table().ok_or_else(|| {
        Error::Agent("Codex mcp_servers configuration is not a TOML table".into())
    })?;
    let Some(item) = servers.get("torii") else {
        return Ok(None);
    };
    let table = item
        .as_table()
        .ok_or_else(|| Error::Agent("Codex mcp_servers.torii is not a TOML table".into()))?;
    let command = table
        .get("command")
        .and_then(Item::as_str)
        .ok_or_else(|| Error::Agent("Codex mcp_servers.torii.command is not a string".into()))?;
    let config_dir = table
        .get("env")
        .and_then(Item::as_table)
        .and_then(|env| env.get("TORII_CONFIG_DIR"))
        .and_then(Item::as_str)
        .ok_or_else(|| {
            Error::Agent("Codex mcp_servers.torii.env.TORII_CONFIG_DIR is not a string".into())
        })?;
    Ok(Some(McpEntry {
        command: command.into(),
        config_dir: config_dir.into(),
    }))
}

fn write_mcp_entry(document: &mut DocumentMut, entry: &McpEntry) -> Result<()> {
    if !document.contains_key("mcp_servers") {
        let mut servers = Table::new();
        servers.set_implicit(true);
        document["mcp_servers"] = Item::Table(servers);
    }
    let servers = document["mcp_servers"]
        .as_table_mut()
        .ok_or_else(|| Error::Agent("Codex mcp_servers is not a TOML table".into()))?;
    servers.set_implicit(true);
    let mut torii = Table::new();
    torii["command"] = value(&entry.command);
    let mut env = Table::new();
    env.set_implicit(true);
    env["TORII_CONFIG_DIR"] = value(&entry.config_dir);
    torii["env"] = Item::Table(env);
    servers["torii"] = Item::Table(torii);
    Ok(())
}

fn remove_mcp_entry(document: &mut DocumentMut) -> Result<()> {
    let Some(servers) = document.get_mut("mcp_servers") else {
        return Ok(());
    };
    let servers = servers.as_table_mut().ok_or_else(|| {
        Error::Agent("Codex mcp_servers configuration is not a TOML table".into())
    })?;
    servers.remove("torii");
    if servers.is_empty() {
        document.remove("mcp_servers");
    }
    Ok(())
}

fn parse_toml(path: &Path, source: &str) -> Result<DocumentMut> {
    source
        .parse::<DocumentMut>()
        .map_err(|error| Error::Agent(format!("invalid Codex TOML at {}: {error}", path.display())))
}

fn parse_hooks(path: &Path, source: &str) -> Result<Value> {
    let value: Value = serde_json::from_str(source).map_err(|error| {
        Error::Agent(format!(
            "invalid Codex hooks JSON at {}: {error}",
            path.display()
        ))
    })?;
    if !value.is_object() {
        return Err(Error::Agent(format!(
            "Codex hooks file {} must contain a JSON object",
            path.display()
        )));
    }
    Ok(value)
}

fn add_hook(root: &mut Value, command: &str, command_windows: &str) -> Result<()> {
    let object = root
        .as_object_mut()
        .ok_or_else(|| Error::Agent("Codex hooks root is not an object".into()))?;
    let hooks = object.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| Error::Agent("Codex hooks field is not an object".into()))?;
    let pre_tool = hooks.entry("PreToolUse").or_insert_with(|| json!([]));
    let pre_tool = pre_tool
        .as_array_mut()
        .ok_or_else(|| Error::Agent("Codex hooks.PreToolUse is not an array".into()))?;
    pre_tool.push(json!({
        "matcher": "^Bash$",
        "hooks": [{
            "type": "command",
            "command": command,
            "commandWindows": command_windows,
            "timeout": 5,
            "statusMessage": HOOK_MARKER
        }]
    }));
    Ok(())
}

fn remove_hook(root: &mut Value, command: &str, command_windows: &str) -> Result<()> {
    let Some(pre_tool) = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .and_then(|hooks| hooks.get_mut("PreToolUse"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(());
    };
    for group in pre_tool.iter_mut() {
        let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        handlers.retain(|handler| !hook_matches(handler, command, command_windows));
    }
    pre_tool.retain(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|handlers| !handlers.is_empty())
    });
    Ok(())
}

fn remove_stale_hooks(root: &mut Value, command: &str, command_windows: &str) -> Result<bool> {
    let Some(pre_tool) = root
        .get_mut("hooks")
        .and_then(Value::as_object_mut)
        .and_then(|hooks| hooks.get_mut("PreToolUse"))
        .and_then(Value::as_array_mut)
    else {
        return Ok(false);
    };
    let mut changed = false;
    for group in pre_tool.iter_mut() {
        let Some(handlers) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        let original = handlers.len();
        handlers.retain(|handler| {
            hook_matches(handler, command, command_windows) || !is_torii_hook(handler)
        });
        changed |= handlers.len() != original;
    }
    let original = pre_tool.len();
    pre_tool.retain(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|handlers| !handlers.is_empty())
    });
    changed |= pre_tool.len() != original;
    Ok(changed)
}

fn has_hook(root: &Value, command: &str, command_windows: &str) -> bool {
    hook_handlers(root).any(|handler| hook_matches(handler, command, command_windows))
}

fn has_any_torii_hook(root: &Value) -> bool {
    hook_handlers(root).any(is_torii_hook)
}

fn hook_handlers(root: &Value) -> impl Iterator<Item = &Value> {
    root.get("hooks")
        .and_then(|hooks| hooks.get("PreToolUse"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("hooks").and_then(Value::as_array))
        .flatten()
}

fn hook_matches(handler: &Value, command: &str, command_windows: &str) -> bool {
    handler.get("statusMessage").and_then(Value::as_str) == Some(HOOK_MARKER)
        && handler.get("command").and_then(Value::as_str) == Some(command)
        && handler.get("commandWindows").and_then(Value::as_str) == Some(command_windows)
}

fn is_torii_hook(handler: &Value) -> bool {
    handler.get("statusMessage").and_then(Value::as_str) == Some(HOOK_MARKER)
        && handler
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|command| command.contains("__agent-hook codex"))
}

fn guarded_provider<'a>(
    command: &str,
    providers: &'a BTreeMap<String, (String, bool)>,
) -> Option<&'a (String, bool)> {
    let segments = shell_segments(command);
    for segment in segments {
        if let Some(key) = executable_in_segment(&segment, providers.keys().map(String::as_str)) {
            return providers.get(&key);
        }
    }
    None
}

fn shell_segments(command: &str) -> Vec<Vec<String>> {
    let mut segments = vec![Vec::new()];
    for token in shell_tokens(command) {
        match token {
            ShellToken::Separator => {
                if segments.last().is_some_and(|segment| !segment.is_empty()) {
                    segments.push(Vec::new());
                }
            }
            ShellToken::Word(word) => segments.last_mut().expect("one segment").push(word),
        }
    }
    segments.retain(|segment| !segment.is_empty());
    segments
}

fn shell_tokens(command: &str) -> Vec<ShellToken> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in command.chars() {
        if escaped {
            word.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote != Some('\'') {
            escaped = true;
            word.push(character);
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                word.push(character);
            }
            continue;
        }
        match character {
            '\'' | '"' => quote = Some(character),
            ';' | '|' | '&' | '\n' | '\r' | '(' | ')' => {
                push_word(&mut tokens, &mut word);
                if !matches!(tokens.last(), Some(ShellToken::Separator)) {
                    tokens.push(ShellToken::Separator);
                }
            }
            character if character.is_whitespace() => push_word(&mut tokens, &mut word),
            _ => word.push(character),
        }
    }
    push_word(&mut tokens, &mut word);
    tokens
}

fn push_word(tokens: &mut Vec<ShellToken>, word: &mut String) {
    if !word.is_empty() {
        tokens.push(ShellToken::Word(std::mem::take(word)));
    }
}

fn executable_in_segment<'a>(
    words: &[String],
    provider_keys: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let providers = provider_keys.collect::<BTreeSet<_>>();
    let mut index = 0;
    while index < words.len() && is_assignment(&words[index]) {
        index += 1;
    }
    let first = words.get(index)?;
    let first_key = executable_key(first);
    if providers.contains(first_key.as_str()) {
        return Some(first_key);
    }

    match first_key.as_str() {
        "env" | "command" | "nohup" | "sudo" | "doas" | "time" | "nice" => {
            wrapped_executable(words, index + 1, &first_key)
                .map(executable_key)
                .filter(|key| providers.contains(key.as_str()))
        }
        "start-process" => start_process_executable(words, index + 1)
            .map(executable_key)
            .filter(|key| providers.contains(key.as_str())),
        "sh" | "bash" | "zsh" | "fish" | "pwsh" | "powershell" | "cmd" => {
            nested_shell_command(words, index + 1)
                .and_then(|nested| guarded_key(nested, &providers))
        }
        _ => None,
    }
}

fn wrapped_executable<'a>(words: &'a [String], start: usize, wrapper: &str) -> Option<&'a str> {
    let mut index = start;
    while index < words.len() {
        let word = &words[index];
        if is_assignment(word) {
            index += 1;
            continue;
        }
        if word.starts_with('-') {
            let option = word.split_once('=').map_or(word.as_str(), |(name, _)| name);
            index += 1;
            if wrapper_option_takes_value(wrapper, option) && !word.contains('=') {
                index += 1;
            }
            continue;
        }
        return Some(word);
    }
    None
}

fn wrapper_option_takes_value(wrapper: &str, option: &str) -> bool {
    match wrapper {
        "sudo" => matches!(
            option,
            "-u" | "--user"
                | "-g"
                | "--group"
                | "-h"
                | "--host"
                | "-p"
                | "--prompt"
                | "-C"
                | "--close-from"
                | "-T"
                | "--command-timeout"
                | "-R"
                | "--chroot"
                | "-D"
                | "--chdir"
        ),
        "doas" => matches!(option, "-u"),
        "env" => matches!(
            option,
            "-u" | "--unset" | "-C" | "--chdir" | "-S" | "--split-string"
        ),
        "nice" => matches!(option, "-n" | "--adjustment"),
        "time" => matches!(option, "-f" | "--format" | "-o" | "--output"),
        _ => false,
    }
}

fn start_process_executable(words: &[String], start: usize) -> Option<&str> {
    let mut index = start;
    while index < words.len() {
        let word = &words[index];
        if word.eq_ignore_ascii_case("-FilePath") {
            return words.get(index + 1).map(String::as_str);
        }
        if word.starts_with('-') {
            index += 1;
            continue;
        }
        return Some(word);
    }
    None
}

fn nested_shell_command(words: &[String], start: usize) -> Option<&str> {
    let command_flag =
        words.iter().enumerate().skip(start).find(|(_, word)| {
            matches!(word.to_ascii_lowercase().as_str(), "-c" | "-command" | "/c")
        })?;
    words.get(command_flag.0 + 1).map(String::as_str)
}

fn guarded_key(command: &str, providers: &BTreeSet<&str>) -> Option<String> {
    shell_segments(command)
        .into_iter()
        .find_map(|segment| executable_in_segment(&segment, providers.iter().copied()))
}

fn executable_key(value: &str) -> String {
    let file = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_matches(['"', '\''])
        .to_ascii_lowercase();
    [".exe", ".cmd", ".bat", ".com"]
        .iter()
        .find_map(|suffix| file.strip_suffix(suffix))
        .unwrap_or(&file)
        .to_string()
}

fn is_assignment(word: &str) -> bool {
    word.split_once('=').is_some_and(|(name, _)| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
    })
}

fn print_denial(agent: &str, reason: String) -> Result<()> {
    let output = match agent {
        "codex" | "claude" => json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": reason
            }
        }),
        "gemini" => json!({
            "decision": "deny",
            "reason": reason
        }),
        "cursor" => json!({
            "permission": "deny",
            "user_message": reason,
            "agent_message": reason
        }),
        _ => return Err(Error::Agent(format!("unsupported agent hook {agent:?}"))),
    };
    println!(
        "{}",
        serde_json::to_string(&output)
            .map_err(|error| Error::Agent(format!("failed to encode hook response: {error}")))?
    );
    Ok(())
}

fn discover_codex_home() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(path.into());
    }
    dirs::home_dir()
        .map(|home| home.join(".codex"))
        .ok_or(Error::NoConfigDirectory)
}

fn current_executable() -> Result<PathBuf> {
    std::env::current_exe()
        .map_err(|error| Error::Agent(format!("could not locate the Torii executable: {error}")))?
        .canonicalize()
        .map_err(|error| Error::Agent(format!("could not normalize the Torii executable: {error}")))
}

fn hook_command(executable: &Path, config: &Path, windows: bool) -> Result<String> {
    let executable = path_string(executable)?;
    let config = path_string(config)?;
    if windows {
        Ok(format!(
            "{} __agent-hook codex --config {}",
            quote_windows(&executable),
            quote_windows(&config)
        ))
    } else {
        Ok(format!(
            "{} __agent-hook codex --config {}",
            quote_unix(&executable),
            quote_unix(&config)
        ))
    }
}

fn quote_windows(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

fn quote_unix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn path_string(path: &Path) -> Result<String> {
    let value = path
        .to_str()
        .ok_or_else(|| Error::Agent(format!("path is not valid UTF-8: {}", path.display())))?;
    #[cfg(windows)]
    {
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return Ok(format!(r"\\{unc}"));
        }
        if let Some(local) = value.strip_prefix(r"\\?\") {
            return Ok(local.into());
        }
    }
    Ok(value.into())
}

fn state_path(paths: &ConfigPaths) -> PathBuf {
    paths.base().join("agents").join("codex.json")
}

fn read_state(path: &Path) -> Result<Option<ManagedState>> {
    let Some(source) = read_optional(path)? else {
        return Ok(None);
    };
    serde_json::from_str(&source).map(Some).map_err(|error| {
        Error::Agent(format!(
            "invalid Torii agent state at {}: {error}",
            path.display()
        ))
    })
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(source) => Ok(Some(source)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Agent(format!("path has no parent: {}", path.display())))?;
    create_dir(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temp.write_all(contents)
        .and_then(|_| temp.flush())
        .map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
    temp.persist(path).map_err(|error| Error::Write {
        path: path.to_path_buf(),
        source: error.error,
    })?;
    Ok(())
}

fn create_dir(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

fn format_json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map(|mut output| {
            output.push('\n');
            output
        })
        .map_err(|error| Error::Agent(format!("failed to encode agent integration: {error}")))
}

fn status_label(installed: bool, managed: bool) -> &'static str {
    match (installed, managed) {
        (true, true) => "installed (managed by Torii)",
        (true, false) => "installed (not managed by this Torii configuration)",
        (false, _) => "not installed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn providers() -> BTreeMap<String, (String, bool)> {
        BTreeMap::from([
            ("aws".into(), ("aws".into(), false)),
            ("kubectl".into(), ("kubectl".into(), true)),
        ])
    }

    #[test]
    fn guard_detects_direct_paths_chains_and_nested_shells() {
        let providers = providers();
        assert_eq!(
            guarded_provider("aws s3 ls", &providers),
            Some(&("aws".into(), false))
        );
        assert_eq!(
            guarded_provider(
                r#"echo ok && "C:\Program Files\AWS\aws.exe" s3 ls"#,
                &providers
            ),
            Some(&("aws".into(), false))
        );
        assert_eq!(
            guarded_provider("pwsh -Command 'kubectl get pods'", &providers),
            Some(&("kubectl".into(), true))
        );
        assert_eq!(
            guarded_provider("sudo -u root kubectl get pods", &providers),
            Some(&("kubectl".into(), true))
        );
    }

    #[test]
    fn guard_does_not_block_provider_names_used_as_data() {
        let providers = providers();
        assert_eq!(guarded_provider("echo aws", &providers), None);
        assert_eq!(guarded_provider("rg kubectl docs", &providers), None);
        assert_eq!(guarded_provider("Get-Command aws", &providers), None);
        assert_eq!(guarded_provider("sudo echo aws", &providers), None);
        assert_eq!(guarded_provider("Start-Process echo aws", &providers), None);
    }

    #[test]
    fn install_is_idempotent_and_preserves_existing_configuration() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().join("torii-config"));
        let codex = temp.path().join("codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(
            codex.join("config.toml"),
            "model = \"test-model\"\n\n[mcp_servers]\n\n[mcp_servers.existing]\ncommand = \"existing\"\n",
        )
        .unwrap();
        std::fs::write(
            codex.join("hooks.json"),
            r#"{"hooks":{"SessionStart":[{"hooks":[]}]}}"#,
        )
        .unwrap();
        let executable = temp.path().join("torii.exe");

        install_at(&paths, &codex, &executable, true).unwrap();
        install_at(&paths, &codex, &executable, true).unwrap();

        let config = std::fs::read_to_string(codex.join("config.toml")).unwrap();
        assert!(config.contains("model = \"test-model\""));
        assert!(config.contains("[mcp_servers.existing]"));
        assert_eq!(config.matches("[mcp_servers.torii]").count(), 1);
        assert!(!config.lines().any(|line| line == "[mcp_servers]"));
        let hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(codex.join("hooks.json")).unwrap())
                .unwrap();
        assert_eq!(hook_handlers(&hooks).count(), 1);
        let status = status_at(&paths, Some(&codex)).unwrap();
        assert_eq!(
            status,
            Status {
                codex_home: codex,
                mcp_installed: true,
                mcp_managed: true,
                hook_installed: true,
                hook_managed: true,
            }
        );
    }

    #[test]
    fn uninstall_hook_preserves_mcp_and_full_uninstall_preserves_other_entries() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().join("torii-config"));
        let codex = temp.path().join("codex");
        let executable = temp.path().join("torii");

        install_at(&paths, &codex, &executable, true).unwrap();
        uninstall_at(&paths, true).unwrap();
        let status = status_at(&paths, Some(&codex)).unwrap();
        assert!(status.mcp_installed);
        assert!(!status.hook_installed);

        uninstall_at(&paths, false).unwrap();
        let status = status_at(&paths, Some(&codex)).unwrap();
        assert!(!status.mcp_installed);
        assert!(!status.hook_installed);
    }

    #[test]
    fn install_refuses_to_replace_an_unmanaged_torii_server() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().join("torii-config"));
        let codex = temp.path().join("codex");
        std::fs::create_dir_all(&codex).unwrap();
        std::fs::write(
            codex.join("config.toml"),
            "[mcp_servers.torii]\ncommand = \"someone-else\"\n[mcp_servers.torii.env]\nTORII_CONFIG_DIR = \"elsewhere\"\n",
        )
        .unwrap();

        let error = install_at(&paths, &codex, &temp.path().join("torii"), false).unwrap_err();
        assert!(error.to_string().contains("conflicting MCP server"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_paths_are_written_in_application_format() {
        assert_eq!(
            path_string(Path::new(r"\\?\C:\tools\torii.exe")).unwrap(),
            r"C:\tools\torii.exe"
        );
        assert_eq!(
            path_string(Path::new(r"\\?\UNC\server\share\torii.exe")).unwrap(),
            r"\\server\share\torii.exe"
        );
    }
}
