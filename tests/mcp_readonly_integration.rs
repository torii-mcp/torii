use std::fs;

use rmcp::{
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use serde_json::{json, Value};
use tempfile::TempDir;

fn write_readonly_provider(config: &TempDir) {
    let provider = config.path().join("providers").join("torii_reader");
    fs::create_dir_all(&provider).unwrap();
    let command = serde_json::to_string(env!("CARGO_BIN_EXE_torii")).unwrap();
    fs::write(
        provider.join("provider.yaml"),
        format!(
            r#"version: "1"
name: torii_reader
tool: torii_reader
description: Read-only MCP integration provider
command: {command}
policy:
  minimum_accept_tokens: 1
auth:
  strategy: inherited
environment:
  file: .env
"#,
        ),
    )
    .unwrap();
    fs::write(
        provider.join("rules.yaml"),
        "version: \"1.0\"\ndeny:\n  - \"agent list\"\naccept:\n  - \"config-dir\"\n",
    )
    .unwrap();
}

fn structured(result: &impl serde::Serialize) -> Value {
    serde_json::to_value(result).unwrap()["structuredContent"].clone()
}

#[tokio::test]
async fn mcp_executes_only_a_read_operation_and_denies_another_before_spawn() {
    let config = TempDir::new().unwrap();
    write_readonly_provider(&config);

    let transport = TokioChildProcess::new(
        tokio::process::Command::new(env!("CARGO_BIN_EXE_torii")).configure(|command| {
            command
                .env("TORII_CONFIG_DIR", config.path())
                .env("TORII_NO_GUI", "1");
        }),
    )
    .unwrap();
    let client = ().serve(transport).await.unwrap();

    let tools = client.list_all_tools().await.unwrap();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "torii_reader");
    assert_eq!(tools[1].name, "torii_policy");
    assert_eq!(
        tools[1]
            .annotations
            .as_ref()
            .and_then(|annotations| annotations.read_only_hint),
        Some(true)
    );

    let policy = client
        .call_tool(
            CallToolRequestParams::new("torii_policy").with_arguments(
                json!({ "provider": "torii_reader" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let policy = structured(&policy);
    assert_eq!(policy["provider"], "torii_reader");
    assert_eq!(policy["accept"], json!(["config-dir"]));
    assert_eq!(policy["deny"], json!(["agent list"]));
    assert!(policy["unmatched"]
        .as_str()
        .unwrap()
        .contains("default-deny"));

    let allowed = client
        .call_tool(
            CallToolRequestParams::new("torii_reader").with_arguments(
                json!({ "args": ["config-dir"] })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let allowed = structured(&allowed);
    assert_eq!(allowed["decision"]["result"], "allow");
    assert_eq!(allowed["execution"]["exit_code"], 0);
    assert_eq!(
        allowed["execution"]["stdout"].as_str().unwrap().trim(),
        config.path().to_str().unwrap()
    );

    let denied = client
        .call_tool(
            CallToolRequestParams::new("torii_reader").with_arguments(
                json!({ "args": ["agent", "list"] })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    let denied = structured(&denied);
    assert_eq!(denied["decision"]["result"], "deny");
    assert!(denied["execution"].is_null());

    client.cancel().await.unwrap();
}
