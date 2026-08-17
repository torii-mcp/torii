use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;
use tempfile::TempDir;

fn torii() -> Command {
    Command::new(env!("CARGO_BIN_EXE_torii"))
}

fn write_provider(config: &TempDir) {
    let provider = config.path().join("providers").join("aws");
    std::fs::create_dir_all(&provider).unwrap();
    std::fs::write(
        provider.join("provider.yaml"),
        r#"version: "1"
name: aws
tool: aws
description: AWS test provider
command: aws
policy:
  minimum_accept_tokens: 2
auth:
  strategy: inherited
environment:
  file: .env
"#,
    )
    .unwrap();
    std::fs::write(
        provider.join("rules.yaml"),
        "version: \"1.0\"\ndeny: []\naccept: []\n",
    )
    .unwrap();
}

fn hook(config: &TempDir, agent: &str, input: Value) -> std::process::Output {
    let mut child = torii()
        .args([
            "__agent-hook",
            agent,
            "--config",
            config.path().to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn codex_hook_blocks_provider_execution_and_allows_plain_data() {
    let config = TempDir::new().unwrap();
    write_provider(&config);

    let blocked = hook(
        &config,
        "codex",
        serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": "aws s3 ls" }
        }),
    );
    assert!(blocked.status.success());
    let response: Value = serde_json::from_slice(&blocked.stdout).unwrap();
    assert_eq!(response["hookSpecificOutput"]["permissionDecision"], "deny");
    assert!(response["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap()
        .contains("MCP tool \"aws\""));

    let allowed = hook(
        &config,
        "codex",
        serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": "echo aws" }
        }),
    );
    assert!(allowed.status.success());
    assert!(allowed.stdout.is_empty());
}

#[test]
fn claude_gemini_and_cursor_hooks_use_their_native_denial_contracts() {
    let config = TempDir::new().unwrap();
    write_provider(&config);

    let claude = hook(
        &config,
        "claude",
        serde_json::json!({
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": "aws s3 ls" }
        }),
    );
    let claude: Value = serde_json::from_slice(&claude.stdout).unwrap();
    assert_eq!(claude["hookSpecificOutput"]["permissionDecision"], "deny");

    let gemini = hook(
        &config,
        "gemini",
        serde_json::json!({
            "hook_event_name": "BeforeTool",
            "tool_name": "run_shell_command",
            "tool_input": { "command": "aws s3 ls" }
        }),
    );
    let gemini: Value = serde_json::from_slice(&gemini.stdout).unwrap();
    assert_eq!(gemini["decision"], "deny");

    let cursor = hook(
        &config,
        "cursor",
        serde_json::json!({
            "hook_event_name": "beforeShellExecution",
            "command": "aws s3 ls"
        }),
    );
    let cursor: Value = serde_json::from_slice(&cursor.stdout).unwrap();
    assert_eq!(cursor["permission"], "deny");

    // O Antigravity descreve a chamada em toolCall e lê a linha de CommandLine.
    let antigravity = hook(
        &config,
        "antigravity",
        serde_json::json!({
            "toolCall": {
                "name": "run_command",
                "args": { "CommandLine": "aws s3 ls", "Cwd": "/workspace" }
            },
            "stepIdx": 7
        }),
    );
    let antigravity: Value = serde_json::from_slice(&antigravity.stdout).unwrap();
    assert_eq!(antigravity["decision"], "deny");

    // Outra tool do mesmo agente não é assunto do guard.
    let unrelated = hook(
        &config,
        "antigravity",
        serde_json::json!({
            "toolCall": { "name": "view_file", "args": { "AbsolutePath": "/workspace/a" } }
        }),
    );
    assert!(unrelated.stdout.is_empty());
}

#[test]
fn antigravity_hook_lives_in_its_own_group_and_leaves_others_alone() {
    let config = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let hooks_path = home.path().join("hooks.json");
    std::fs::write(&hooks_path, r#"{"meu-linter":{"PostToolUse":[]}}"#).unwrap();

    let install = torii()
        .env("TORII_CONFIG_DIR", config.path())
        .env("TORII_ANTIGRAVITY_HOME", home.path())
        .args(["agent", "install", "antigravity", "--hook"])
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let mcp: Value = serde_json::from_str(
        &std::fs::read_to_string(home.path().join("mcp_config.json")).unwrap(),
    )
    .unwrap();
    assert!(mcp["mcpServers"]["torii"]["command"].is_string());

    let hooks: Value =
        serde_json::from_str(&std::fs::read_to_string(&hooks_path).unwrap()).unwrap();
    let entries = hooks["torii-provider-boundary"]["PreToolUse"]
        .as_array()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["matcher"], "run_command");
    assert_eq!(entries[0]["hooks"][0]["type"], "command");
    assert_eq!(hooks["torii-provider-boundary"]["enabled"], true);
    assert!(
        hooks["meu-linter"].is_object(),
        "outro grupo foi preservado"
    );

    let uninstall = torii()
        .env("TORII_CONFIG_DIR", config.path())
        .env("TORII_ANTIGRAVITY_HOME", home.path())
        .args(["agent", "uninstall", "antigravity"])
        .output()
        .unwrap();
    assert!(uninstall.status.success());
    let hooks: Value =
        serde_json::from_str(&std::fs::read_to_string(&hooks_path).unwrap()).unwrap();
    assert!(
        hooks.get("torii-provider-boundary").is_none(),
        "o grupo do Torii deve sair inteiro, sem deixar carcaça"
    );
    assert!(hooks["meu-linter"].is_object());
}

#[test]
fn codex_install_status_and_uninstall_work_in_an_isolated_home() {
    let config = TempDir::new().unwrap();
    let codex = TempDir::new().unwrap();
    let environment = [
        ("TORII_CONFIG_DIR", config.path().to_str().unwrap()),
        ("CODEX_HOME", codex.path().to_str().unwrap()),
    ];

    let install = torii()
        .args(["agent", "install", "codex", "--hook"])
        .envs(environment)
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let config_toml = std::fs::read_to_string(codex.path().join("config.toml")).unwrap();
    assert!(config_toml.contains("[mcp_servers.torii]"));
    let hooks: Value =
        serde_json::from_str(&std::fs::read_to_string(codex.path().join("hooks.json")).unwrap())
            .unwrap();
    assert_eq!(hooks["hooks"]["PreToolUse"].as_array().unwrap().len(), 1);

    let status = torii()
        .args(["agent", "status", "codex"])
        .envs(environment)
        .output()
        .unwrap();
    assert!(status.status.success());
    let status = String::from_utf8(status.stdout).unwrap();
    assert!(status.contains("mcp\tinstalled (managed by Torii)"));
    assert!(status.contains("hook\tinstalled (managed by Torii)"));

    let uninstall = torii()
        .args(["agent", "uninstall", "codex"])
        .envs(environment)
        .output()
        .unwrap();
    assert!(uninstall.status.success());
    let config_toml = std::fs::read_to_string(codex.path().join("config.toml")).unwrap();
    assert!(!config_toml.contains("mcp_servers.torii"));
}

/// Cada agente sem hook tem sua própria variável de home, arquivo e chave de servidores.
fn mcp_only_agents() -> [(&'static str, &'static str, &'static str, &'static str); 4] {
    [
        ("opencode", "TORII_OPENCODE_HOME", "opencode.json", "mcp"),
        ("copilot", "TORII_COPILOT_HOME", "mcp.json", "servers"),
        (
            "copilot-cli",
            "TORII_COPILOT_CLI_HOME",
            "mcp-config.json",
            "mcpServers",
        ),
        ("pi", "TORII_PI_HOME", "mcp.json", "mcpServers"),
    ]
}

#[test]
fn mcp_only_agents_install_in_their_native_shape_and_preserve_config() {
    for (agent, home_var, file, key) in mcp_only_agents() {
        let config = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let config_path = home.path().join(file);
        std::fs::write(&config_path, r#"{"theme":"dark"}"#).unwrap();

        let mut install = torii();
        install
            .env("TORII_CONFIG_DIR", config.path())
            .env(home_var, home.path())
            .args(["agent", "install", agent]);
        // pi exige confirmação explícita porque depende de uma extensão externa.
        if agent == "pi" {
            install.arg("--yes");
        }
        let install = install.output().unwrap();
        assert!(
            install.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&install.stderr)
        );

        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        let entry = &written[key]["torii"];
        assert!(
            entry.is_object(),
            "{agent} should declare torii under {key}"
        );
        assert_eq!(written["theme"], "dark", "{agent} must preserve other keys");
        match agent {
            "opencode" => {
                assert_eq!(entry["type"], "local", "{agent}");
                assert!(entry["command"].is_array(), "{agent} takes a command array");
                assert!(
                    entry["environment"]["TORII_CONFIG_DIR"].is_string(),
                    "{agent}"
                );
            }
            "copilot-cli" => {
                assert_eq!(entry["type"], "local", "{agent}");
                assert_eq!(entry["tools"][0], "*", "{agent}");
                assert!(entry["env"]["TORII_CONFIG_DIR"].is_string(), "{agent}");
            }
            "copilot" => {
                assert_eq!(entry["type"], "stdio", "{agent}");
                assert!(entry["env"]["TORII_CONFIG_DIR"].is_string(), "{agent}");
            }
            _ => assert!(entry["env"]["TORII_CONFIG_DIR"].is_string(), "{agent}"),
        }

        let status = torii()
            .env("TORII_CONFIG_DIR", config.path())
            .env(home_var, home.path())
            .args(["agent", "status", agent])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&status.stdout);
        assert!(
            stdout.contains("mcp\tinstalled (managed by Torii)"),
            "{agent}: {stdout}"
        );
        assert!(stdout.contains("hook\tnot supported"), "{agent}: {stdout}");

        let uninstall = torii()
            .env("TORII_CONFIG_DIR", config.path())
            .env(home_var, home.path())
            .args(["agent", "uninstall", agent])
            .output()
            .unwrap();
        assert!(
            uninstall.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&uninstall.stderr)
        );
        let written: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(written.get(key).is_none(), "{agent} should remove {key}");
        assert_eq!(written["theme"], "dark", "{agent}");
    }
}

#[test]
fn mcp_only_agents_reject_the_hook_and_pi_requires_confirmation() {
    for (agent, home_var, _, _) in mcp_only_agents() {
        let config = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let hook = torii()
            .env("TORII_CONFIG_DIR", config.path())
            .env(home_var, home.path())
            .args(["agent", "install", agent, "--hook"])
            .output()
            .unwrap();
        assert!(!hook.status.success(), "{agent} must reject --hook");
        let stderr = String::from_utf8_lossy(&hook.stderr);
        assert!(
            stderr.contains("does not offer a pre-execution hook"),
            "{agent}: {stderr}"
        );
        // Recusar --hook não pode deixar configuração pela metade.
        assert!(
            std::fs::read_dir(home.path())
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(true),
            "{agent} wrote configuration despite failing"
        );
    }

    let config = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    let unconfirmed = torii()
        .env("TORII_CONFIG_DIR", config.path())
        .env("TORII_PI_HOME", home.path())
        .args(["agent", "install", "pi"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        !unconfirmed.status.success(),
        "pi must ask before installing"
    );
    let stderr = String::from_utf8_lossy(&unconfirmed.stderr);
    assert!(stderr.contains("does not support MCP natively"), "{stderr}");
    assert!(stderr.contains("do not continue"), "{stderr}");
    assert!(
        !home.path().join("mcp.json").exists(),
        "pi wrote config without confirmation"
    );
}

#[test]
fn opencode_refuses_to_edit_a_jsonc_configuration() {
    let config = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    std::fs::write(
        home.path().join("opencode.jsonc"),
        "{\n  // comentário do humano\n  \"theme\": \"dark\"\n}\n",
    )
    .unwrap();
    let install = torii()
        .env("TORII_CONFIG_DIR", config.path())
        .env("TORII_OPENCODE_HOME", home.path())
        .args(["agent", "install", "opencode"])
        .output()
        .unwrap();
    assert!(!install.status.success());
    let stderr = String::from_utf8_lossy(&install.stderr);
    assert!(stderr.contains("opencode.jsonc"), "{stderr}");
    assert!(!home.path().join("opencode.json").exists());
}

#[test]
fn portable_agent_installers_preserve_config_and_can_be_uninstalled() {
    for agent in ["claude", "gemini", "cursor"] {
        let config = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let home_value = home.path().to_str().unwrap();
        let mut command = torii();
        command.env("TORII_CONFIG_DIR", config.path());
        match agent {
            "claude" => {
                command.env("CLAUDE_CONFIG_DIR", home.path());
                std::fs::write(home.path().join(".claude.json"), r#"{"theme":"dark"}"#).unwrap();
            }
            "gemini" => {
                command.env("GEMINI_CLI_HOME", home.path());
                std::fs::create_dir_all(home.path().join(".gemini")).unwrap();
                std::fs::write(
                    home.path().join(".gemini").join("settings.json"),
                    r#"{"theme":"dark"}"#,
                )
                .unwrap();
            }
            "cursor" => {
                command.env("TORII_CURSOR_HOME", home.path());
                std::fs::write(home.path().join("mcp.json"), r#"{"theme":"dark"}"#).unwrap();
            }
            _ => unreachable!(),
        }
        let install = command
            .args(["agent", "install", agent, "--hook"])
            .output()
            .unwrap();
        assert!(
            install.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&install.stderr)
        );

        let (mcp_path, hooks_path) = match agent {
            "claude" => (
                home.path().join(".claude.json"),
                home.path().join("settings.json"),
            ),
            "gemini" => {
                let settings = home.path().join(".gemini").join("settings.json");
                (settings.clone(), settings)
            }
            "cursor" => (home.path().join("mcp.json"), home.path().join("hooks.json")),
            _ => unreachable!(),
        };
        let mcp: Value =
            serde_json::from_str(&std::fs::read_to_string(&mcp_path).unwrap()).unwrap();
        assert!(mcp["mcpServers"]["torii"].is_object(), "{agent}");
        assert_eq!(mcp["theme"], "dark", "{agent}");
        let hooks: Value =
            serde_json::from_str(&std::fs::read_to_string(&hooks_path).unwrap()).unwrap();
        assert!(hooks["hooks"].is_object(), "{agent}");

        let mut uninstall = torii();
        uninstall
            .env("TORII_CONFIG_DIR", config.path())
            .env("CLAUDE_CONFIG_DIR", home_value)
            .env("GEMINI_CLI_HOME", home_value)
            .env("TORII_CURSOR_HOME", home_value)
            .args(["agent", "uninstall", agent]);
        let uninstall = uninstall.output().unwrap();
        assert!(
            uninstall.status.success(),
            "{agent}: {}",
            String::from_utf8_lossy(&uninstall.stderr)
        );
        let mcp: Value =
            serde_json::from_str(&std::fs::read_to_string(&mcp_path).unwrap()).unwrap();
        assert!(mcp.get("mcpServers").is_none(), "{agent}");
        assert_eq!(mcp["theme"], "dark", "{agent}");
    }
}
