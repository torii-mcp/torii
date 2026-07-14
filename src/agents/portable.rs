use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::config::ConfigPaths;
use crate::error::{Error, Result};

const STATE_VERSION: u32 = 1;
const HOOK_MARKER: &str = "Torii provider boundary";

#[derive(Debug, Default, Serialize, Deserialize)]
struct ManagedState {
    version: u32,
    config_home: PathBuf,
    mcp_owned: bool,
    mcp_entry: Value,
    hook_owned: bool,
    hook_entry: Value,
}

struct Spec {
    name: &'static str,
    display_name: &'static str,
    config_home: PathBuf,
    mcp_path: PathBuf,
    hooks_path: PathBuf,
    hook_event: &'static str,
}

pub fn install(paths: &ConfigPaths, agent: &str, with_hook: bool) -> Result<()> {
    let spec = spec(agent)?;
    let executable = current_executable()?;
    install_at(paths, &spec, &executable, with_hook)?;
    eprintln!(
        "{} MCP integration installed at {}{}.",
        spec.display_name,
        spec.config_home.display(),
        if with_hook {
            " with the Torii shell hook"
        } else {
            ""
        }
    );
    eprintln!("Restart {} to load the integration.", spec.display_name);
    Ok(())
}

pub fn print_status(paths: &ConfigPaths, agent: &str) -> Result<()> {
    let spec = spec_from_state(paths, agent)?;
    let state = read_state(&state_path(paths, agent))?;
    let mcp = read_json_object(&spec.mcp_path)?;
    let hooks = if spec.hooks_path == spec.mcp_path {
        mcp.clone()
    } else {
        read_json_object(&spec.hooks_path)?
    };
    let mcp_entry = mcp_entry(&mcp);
    let hook_entry = find_hook(&hooks, &spec);
    let mcp_managed = mcp_entry.is_some()
        && state
            .as_ref()
            .is_some_and(|state| state.mcp_owned && mcp_entry == Some(&state.mcp_entry));
    let hook_managed = hook_entry.is_some()
        && state
            .as_ref()
            .is_some_and(|state| state.hook_owned && hook_entry == Some(&state.hook_entry));
    println!("config_home\t{}", spec.config_home.display());
    println!("mcp\t{}", status_label(mcp_entry.is_some(), mcp_managed));
    println!("hook\t{}", status_label(hook_entry.is_some(), hook_managed));
    Ok(())
}

pub fn uninstall(paths: &ConfigPaths, agent: &str, hook_only: bool) -> Result<()> {
    let spec = spec_from_state(paths, agent)?;
    let state_path = state_path(paths, agent);
    let Some(mut state) = read_state(&state_path)? else {
        return Err(Error::Agent(format!(
            "no Torii-managed {} integration was found for this configuration",
            spec.display_name
        )));
    };

    if !hook_only && state.mcp_owned {
        let mut root = read_json_object(&spec.mcp_path)?;
        match mcp_entry(&root) {
            Some(entry) if entry == &state.mcp_entry => remove_mcp_entry(&mut root)?,
            None => {}
            Some(_) => {
                return Err(Error::Agent(format!(
                    "the managed {} MCP entry was modified; refusing to remove it",
                    spec.display_name
                )));
            }
        }
        write_json(&spec.mcp_path, &root)?;
        state.mcp_owned = false;
    }

    if state.hook_owned {
        let mut root = read_json_object(&spec.hooks_path)?;
        match find_hook(&root, &spec) {
            Some(entry) if entry == &state.hook_entry => {
                remove_hook(&mut root, &spec, &state.hook_entry)?;
            }
            None => {}
            Some(_) => {
                return Err(Error::Agent(format!(
                    "the managed {} hook was modified; refusing to remove it",
                    spec.display_name
                )));
            }
        }
        write_json(&spec.hooks_path, &root)?;
        state.hook_owned = false;
    }

    if state.mcp_owned || state.hook_owned {
        write_json(
            &state_path,
            &serde_json::to_value(&state).map_err(json_error)?,
        )?;
    } else if state_path.exists() {
        std::fs::remove_file(&state_path).map_err(|source| Error::Write {
            path: state_path,
            source,
        })?;
    }

    if hook_only {
        eprintln!(
            "Torii {} hook removed; MCP integration was preserved.",
            spec.display_name
        );
    } else {
        eprintln!("Torii {} integration removed.", spec.display_name);
    }
    eprintln!("Restart {} to reload its configuration.", spec.display_name);
    Ok(())
}

fn install_at(paths: &ConfigPaths, spec: &Spec, executable: &Path, with_hook: bool) -> Result<()> {
    paths.ensure()?;
    create_dir(&spec.config_home)?;
    let state_path = state_path(paths, spec.name);
    let mut state = read_state(&state_path)?.unwrap_or_default();
    if state.version != 0 && state.version != STATE_VERSION {
        return Err(Error::Agent(format!(
            "unsupported {} integration state version {}",
            spec.display_name, state.version
        )));
    }
    if !state.config_home.as_os_str().is_empty() && state.config_home != spec.config_home {
        return Err(Error::Agent(format!(
            "this Torii configuration already manages {} at {}; uninstall it before changing its config home",
            spec.display_name,
            state.config_home.display()
        )));
    }

    let desired_mcp = desired_mcp_entry(spec, executable, paths.base())?;
    let mut mcp = read_json_object(&spec.mcp_path)?;
    match mcp_entry(&mcp) {
        None => {
            set_mcp_entry(&mut mcp, desired_mcp.clone())?;
            state.mcp_owned = true;
        }
        Some(existing) if existing == &desired_mcp => {}
        Some(existing) if state.mcp_owned && existing == &state.mcp_entry => {
            set_mcp_entry(&mut mcp, desired_mcp.clone())?;
        }
        Some(_) => {
            return Err(Error::Agent(format!(
                "{} already has a conflicting MCP server named \"torii\"; remove or rename it before installing",
                spec.display_name
            )));
        }
    }
    write_json(&spec.mcp_path, &mcp)?;

    let desired_hook = desired_hook_entry(spec, executable, paths.base())?;
    if with_hook {
        let mut hooks = read_json_object(&spec.hooks_path)?;
        if state.hook_owned {
            remove_stale_hooks(&mut hooks, spec, &desired_hook)?;
        }
        if find_hook(&hooks, spec) != Some(&desired_hook) {
            if find_hook(&hooks, spec).is_some() {
                return Err(Error::Agent(format!(
                    "{} already has a conflicting Torii shell hook",
                    spec.display_name
                )));
            }
            add_hook(&mut hooks, spec, desired_hook.clone())?;
            state.hook_owned = true;
        }
        write_json(&spec.hooks_path, &hooks)?;
    }

    state.version = STATE_VERSION;
    state.config_home = spec.config_home.clone();
    state.mcp_entry = desired_mcp;
    state.hook_entry = desired_hook;
    write_json(
        &state_path,
        &serde_json::to_value(&state).map_err(json_error)?,
    )?;
    Ok(())
}

fn spec_from_state(paths: &ConfigPaths, agent: &str) -> Result<Spec> {
    let mut spec = spec(agent)?;
    if let Some(state) = read_state(&state_path(paths, agent))? {
        spec = spec_at(agent, state.config_home)?;
    }
    Ok(spec)
}

fn spec(agent: &str) -> Result<Spec> {
    let home = dirs::home_dir().ok_or(Error::NoConfigDirectory)?;
    let config_home = match agent {
        "claude" => std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or(home),
        "gemini" => std::env::var_os("GEMINI_CLI_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or(home)
            .join(".gemini"),
        "cursor" => std::env::var_os("TORII_CURSOR_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cursor")),
        _ => return Err(Error::Agent(format!("unsupported agent {agent:?}"))),
    };
    spec_at(agent, config_home)
}

fn spec_at(agent: &str, config_home: PathBuf) -> Result<Spec> {
    match agent {
        "claude" => {
            let default_home = dirs::home_dir().ok_or(Error::NoConfigDirectory)?;
            let hooks_path = if config_home == default_home {
                config_home.join(".claude").join("settings.json")
            } else {
                config_home.join("settings.json")
            };
            Ok(Spec {
                name: "claude",
                display_name: "Claude Code",
                mcp_path: config_home.join(".claude.json"),
                hooks_path,
                config_home,
                hook_event: "PreToolUse",
            })
        }
        "gemini" => Ok(Spec {
            name: "gemini",
            display_name: "Gemini CLI",
            mcp_path: config_home.join("settings.json"),
            hooks_path: config_home.join("settings.json"),
            config_home,
            hook_event: "BeforeTool",
        }),
        "cursor" => Ok(Spec {
            name: "cursor",
            display_name: "Cursor",
            mcp_path: config_home.join("mcp.json"),
            hooks_path: config_home.join("hooks.json"),
            config_home,
            hook_event: "beforeShellExecution",
        }),
        _ => Err(Error::Agent(format!("unsupported agent {agent:?}"))),
    }
}

fn desired_mcp_entry(spec: &Spec, executable: &Path, config: &Path) -> Result<Value> {
    let mut entry = json!({
        "command": path_string(executable)?,
        "args": [],
        "env": { "TORII_CONFIG_DIR": path_string(config)? }
    });
    if spec.name == "claude" {
        entry
            .as_object_mut()
            .expect("object")
            .insert("type".into(), Value::String("stdio".into()));
    }
    Ok(entry)
}

fn desired_hook_entry(spec: &Spec, executable: &Path, config: &Path) -> Result<Value> {
    let executable = path_string(executable)?;
    let config = path_string(config)?;
    match spec.name {
        "claude" => Ok(json!({
            "matcher": "^Bash$",
            "hooks": [{
                "type": "command",
                "command": executable,
                "args": ["__agent-hook", "claude", "--config", config],
                "timeout": 5,
                "statusMessage": HOOK_MARKER
            }]
        })),
        "gemini" => Ok(json!({
            "matcher": "^run_shell_command$",
            "hooks": [{
                "name": "torii-provider-boundary",
                "type": "command",
                "command": hook_command(&executable, &config, "gemini"),
                "timeout": 5000,
                "description": HOOK_MARKER
            }]
        })),
        "cursor" => Ok(json!({
            "command": hook_command(&executable, &config, "cursor"),
            "matcher": ".*",
            "failClosed": true,
            "timeout": 5,
            "description": HOOK_MARKER
        })),
        _ => Err(Error::Agent(format!("unsupported agent {:?}", spec.name))),
    }
}

fn mcp_entry(root: &Value) -> Option<&Value> {
    root.get("mcpServers")?.get("torii")
}

fn set_mcp_entry(root: &mut Value, entry: Value) -> Result<()> {
    let object = object_mut(root, "agent configuration root")?;
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    object_mut(servers, "mcpServers")?.insert("torii".into(), entry);
    Ok(())
}

fn remove_mcp_entry(root: &mut Value) -> Result<()> {
    let Some(servers) = root.get_mut("mcpServers") else {
        return Ok(());
    };
    let servers = object_mut(servers, "mcpServers")?;
    servers.remove("torii");
    if servers.is_empty() {
        object_mut(root, "agent configuration root")?.remove("mcpServers");
    }
    Ok(())
}

fn hook_array_mut<'a>(root: &'a mut Value, spec: &Spec) -> Result<&'a mut Vec<Value>> {
    let object = object_mut(root, "agent hooks root")?;
    let hooks = object
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = object_mut(hooks, "hooks")?;
    let event = hooks
        .entry(spec.hook_event)
        .or_insert_with(|| Value::Array(Vec::new()));
    event.as_array_mut().ok_or_else(|| {
        Error::Agent(format!(
            "{}.hooks.{} must be an array",
            spec.display_name, spec.hook_event
        ))
    })
}

fn hook_array<'a>(root: &'a Value, spec: &Spec) -> Option<&'a Vec<Value>> {
    root.get("hooks")?.get(spec.hook_event)?.as_array()
}

fn find_hook<'a>(root: &'a Value, spec: &Spec) -> Option<&'a Value> {
    hook_array(root, spec)?
        .iter()
        .find(|entry| is_torii_hook(entry, spec))
}

fn is_torii_hook(entry: &Value, spec: &Spec) -> bool {
    match spec.name {
        "claude" => entry
            .get("hooks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|hook| hook.get("statusMessage").and_then(Value::as_str) == Some(HOOK_MARKER)),
        "gemini" => entry
            .get("hooks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|hook| {
                hook.get("name").and_then(Value::as_str) == Some("torii-provider-boundary")
            }),
        "cursor" => entry.get("description").and_then(Value::as_str) == Some(HOOK_MARKER),
        _ => false,
    }
}

fn add_hook(root: &mut Value, spec: &Spec, entry: Value) -> Result<()> {
    hook_array_mut(root, spec)?.push(entry);
    Ok(())
}

fn remove_hook(root: &mut Value, spec: &Spec, expected: &Value) -> Result<()> {
    hook_array_mut(root, spec)?.retain(|entry| entry != expected);
    Ok(())
}

fn remove_stale_hooks(root: &mut Value, spec: &Spec, desired: &Value) -> Result<()> {
    hook_array_mut(root, spec)?.retain(|entry| entry == desired || !is_torii_hook(entry, spec));
    Ok(())
}

fn object_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Map<String, Value>> {
    value
        .as_object_mut()
        .ok_or_else(|| Error::Agent(format!("{name} must be a JSON object")))
}

fn hook_command(executable: &str, config: &str, agent: &str) -> String {
    #[cfg(windows)]
    let quote = quote_windows;
    #[cfg(not(windows))]
    let quote = quote_unix;
    format!(
        "{} __agent-hook {} --config {}",
        quote(executable),
        agent,
        quote(config)
    )
}

#[cfg(windows)]
fn quote_windows(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(not(windows))]
fn quote_unix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn current_executable() -> Result<PathBuf> {
    std::env::current_exe()
        .map_err(|error| Error::Agent(format!("could not locate the Torii executable: {error}")))?
        .canonicalize()
        .map_err(|error| Error::Agent(format!("could not normalize the Torii executable: {error}")))
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

fn state_path(paths: &ConfigPaths, agent: &str) -> PathBuf {
    paths.base().join("agents").join(format!("{agent}.json"))
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

fn read_json_object(path: &Path) -> Result<Value> {
    let Some(source) = read_optional(path)? else {
        return Ok(Value::Object(Map::new()));
    };
    let value: Value = serde_json::from_str(&source)
        .map_err(|error| Error::Agent(format!("invalid JSON at {}: {error}", path.display())))?;
    if !value.is_object() {
        return Err(Error::Agent(format!(
            "{} must contain a JSON object",
            path.display()
        )));
    }
    Ok(value)
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

fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut output = serde_json::to_string_pretty(value).map_err(json_error)?;
    output.push('\n');
    write_atomic(path, output.as_bytes())
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

fn json_error(error: serde_json::Error) -> Error {
    Error::Agent(format!("failed to encode agent integration: {error}"))
}

fn status_label(installed: bool, managed: bool) -> &'static str {
    match (installed, managed) {
        (true, true) => "installed (managed by Torii)",
        (true, false) => "installed (not managed by this Torii configuration)",
        (false, _) => "not installed",
    }
}
