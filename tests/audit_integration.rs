//! The audit log through real Torii processes: several MCP servers writing at
//! once, the output the agent received kept byte for byte, and `torii log`
//! finding and showing what was written.

use std::fs;
use std::process::Command;

use rmcp::{
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use serde_json::{json, Value};
use tempfile::TempDir;
use torii::audit::record::Outcome;
use torii::audit::{Ledger, Query};
use torii::config::ConfigPaths;

const TORII: &str = env!("CARGO_BIN_EXE_torii");

/// A provider that runs Torii itself: `config dir show` is allowed and prints
/// the configuration root, `agent list` is denied.
fn reader_provider(config: &TempDir) {
    let provider = config.path().join("providers").join("torii_reader");
    fs::create_dir_all(&provider).unwrap();
    fs::write(
        provider.join("provider.yaml"),
        format!(
            "version: \"1\"\nname: torii_reader\ntool: torii_reader\ndescription: reader\ncommand: {}\npolicy:\n  minimum_accept_tokens: 1\nauth:\n  strategy: inherited\nenvironment:\n  file: .env\n",
            serde_json::to_string(TORII).unwrap()
        ),
    )
    .unwrap();
    fs::write(
        provider.join("rules.yaml"),
        "version: \"1.0\"\ndeny:\n  - \"agent list\"\naccept:\n  - \"config dir show\"\n",
    )
    .unwrap();
}

fn server(config: &TempDir) -> TokioChildProcess {
    TokioChildProcess::new(tokio::process::Command::new(TORII).configure(|command| {
        command
            .env("TORII_CONFIG_DIR", config.path())
            .env("TORII_NO_GUI", "1")
            .env("TORII_AUDIT_KEYSTORE", "file");
    }))
    .unwrap()
}

fn torii_cli(config: &TempDir, args: &[&str]) -> String {
    let output = Command::new(TORII)
        .args(args)
        .env("TORII_CONFIG_DIR", config.path())
        .env("TORII_AUDIT_KEYSTORE", "file")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "torii {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    args: &[&str],
    description: &str,
) -> Value {
    let result = client
        .call_tool(
            CallToolRequestParams::new("torii_reader").with_arguments(
                json!({ "args": args, "description": description })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    serde_json::to_value(result).unwrap()["structuredContent"].clone()
}

/// Three MCP servers at once, each with allowed and denied calls: every call
/// is one record with the verdict it got, and each record keeps its own reason.
#[tokio::test]
async fn parallel_servers_record_every_call_once() {
    let config = TempDir::new().unwrap();
    reader_provider(&config);
    let mut servers = Vec::new();
    for server_index in 0..3 {
        let transport = server(&config);
        servers.push(tokio::spawn(async move {
            let client = ().serve(transport).await.unwrap();
            for round in 0..4 {
                let reason = format!("servidor {server_index} rodada {round}");
                let allowed = call(
                    &client,
                    &["config", "dir", "show"],
                    &format!("{reason} permitida"),
                )
                .await;
                assert_eq!(allowed["decision"]["result"], "allow");
                let denied = call(&client, &["agent", "list"], &format!("{reason} negada")).await;
                assert_eq!(denied["decision"]["result"], "deny");
            }
            client.cancel().await.unwrap();
        }));
    }
    for server in servers {
        server.await.unwrap();
    }

    let paths = ConfigPaths::new(config.path().to_path_buf());
    let ledger = Ledger::open(&paths).unwrap();
    let entries = ledger.all().unwrap();
    assert_eq!(entries.len(), 3 * 4 * 2);
    let mut reasons = entries
        .iter()
        .map(|entry| entry.record.description.clone().unwrap())
        .collect::<Vec<_>>();
    reasons.sort();
    reasons.dedup();
    assert_eq!(
        reasons.len(),
        entries.len(),
        "every reason is recorded once"
    );
    for entry in &entries {
        let record = &entry.record;
        let reason = record.description.as_deref().unwrap();
        if reason.ends_with("permitida") {
            assert_eq!(record.outcome, Outcome::Allowed, "{reason}");
            assert_eq!(record.args, ["config", "dir", "show"]);
            assert_eq!(record.source.as_deref(), Some("rules"));
            assert_eq!(record.exit_code, Some(0));
            let events = ledger.events(entry.id).unwrap();
            assert_eq!(events.first().unwrap().name, "allowed-by-rules");
            assert_eq!(events.last().unwrap().name, "ran");
        } else {
            assert_eq!(record.outcome, Outcome::Denied, "{reason}");
            assert_eq!(record.args, ["agent", "list"]);
            assert_eq!(record.source.as_deref(), Some("explicit-deny"));
            assert_eq!(record.rule.as_deref(), Some("agent list"));
            assert!(ledger.output(entry.id).unwrap().is_none());
        }
        assert!(record.finished_at.unwrap() >= record.started_at);
    }
    let denied = ledger
        .search(&Query {
            outcomes: vec![Outcome::Denied],
            ..Query::default()
        })
        .unwrap();
    assert_eq!(denied.len(), 12);
}

/// The output kept in the audit log is exactly what the agent received.
#[tokio::test]
async fn the_recorded_output_is_what_the_agent_received() {
    let config = TempDir::new().unwrap();
    reader_provider(&config);
    let client = ().serve(server(&config)).await.unwrap();
    let answer = call(&client, &["config", "dir", "show"], "conferir a raiz").await;
    client.cancel().await.unwrap();

    let paths = ConfigPaths::new(config.path().to_path_buf());
    let ledger = Ledger::open(&paths).unwrap();
    let entry = ledger.newest(1).unwrap().pop().unwrap();
    let output = ledger.output(entry.id).unwrap().unwrap();
    assert_eq!(
        output.stdout,
        answer["execution"]["stdout"].as_str().unwrap()
    );
    assert_eq!(
        output.stderr,
        answer["execution"]["stderr"].as_str().unwrap()
    );
    assert_eq!(entry.record.stdout_bytes, Some(output.stdout.len() as u64));
}

/// `torii log` finds, shows and exports what the servers wrote, and the file
/// on disk shows none of it.
#[tokio::test]
async fn torii_log_finds_shows_and_exports_the_calls() {
    let config = TempDir::new().unwrap();
    reader_provider(&config);
    let client = ().serve(server(&config)).await.unwrap();
    call(
        &client,
        &["config", "dir", "show"],
        "listar a raiz para o relatório",
    )
    .await;
    call(&client, &["agent", "list"], "tentar listar agentes").await;
    client.cancel().await.unwrap();

    // A listing is a header line, then one line per record.
    let records = |listing: &str| {
        let mut lines = listing.lines();
        let header = lines.next().unwrap_or_default();
        assert!(
            header.starts_with("TIME") && header.contains("VERDICT"),
            "{listing}"
        );
        lines.count()
    };

    let tail = torii_cli(&config, &["log", "tail"]);
    assert_eq!(records(&tail), 2, "{tail}");
    assert!(tail.contains("ALLOW") && tail.contains("DENY"), "{tail}");

    let denied = torii_cli(&config, &["log", "search", "--denied"]);
    assert_eq!(records(&denied), 1, "{denied}");
    assert!(
        denied.contains("agent list") && denied.contains("deny rule"),
        "{denied}"
    );

    let by_text = torii_cli(&config, &["log", "search", "--text", "RELATÓRIO"]);
    assert_eq!(records(&by_text), 1, "{by_text}");
    let id = by_text
        .split('#')
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap();

    let shown = torii_cli(&config, &["log", "show", id, "--output"]);
    assert!(shown.contains("listar a raiz para o relatório"), "{shown}");
    assert!(shown.contains("allowed-by-rules"), "{shown}");
    assert!(shown.contains(config.path().to_str().unwrap()), "{shown}");

    let exported: Value =
        serde_json::from_str(&torii_cli(&config, &["log", "export", "--with-output"])).unwrap();
    assert_eq!(exported.as_array().unwrap().len(), 2);
    let markdown = torii_cli(&config, &["log", "export", "--format", "md", "--denied"]);
    assert!(
        markdown.contains("## #") && markdown.contains("deny rule"),
        "{markdown}"
    );

    let mut file = fs::read(config.path().join("audit.db")).unwrap();
    file.extend(fs::read(config.path().join("audit.db-wal")).unwrap_or_default());
    let file = String::from_utf8_lossy(&file);
    for readable in [
        "relatório",
        "agent list",
        "config dir show",
        "allowed-by-rules",
        "SQLite format",
    ] {
        assert!(
            !file.contains(readable),
            "{readable:?} is readable in audit.db"
        );
    }
}
