use std::error::Error;
use std::path::PathBuf;

use rmcp::{
    model::CallToolRequestParams,
    transport::{ConfigureCommandExt, TokioChildProcess},
    ServiceExt,
};
use serde_json::{json, Map, Value};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let request = Request::parse(std::env::args().skip(1).collect())?;
    let transport = TokioChildProcess::new(
        tokio::process::Command::new(&request.torii).configure(|command| {
            command.env("TORII_CONFIG_DIR", &request.config_dir);
        }),
    )?;
    let client = ().serve(transport).await?;

    match request.operation {
        Operation::List => {
            let tools = client.list_all_tools().await?;
            println!("{}", serde_json::to_string_pretty(&tools)?);
        }
        Operation::Call {
            tool,
            target,
            args,
            summary,
        } => {
            let mut arguments = Map::new();
            if let Some(target) = target {
                arguments.insert("target".into(), Value::String(target));
            }
            arguments.insert(
                "args".into(),
                Value::Array(args.into_iter().map(Value::String).collect()),
            );
            let result = client
                .call_tool(CallToolRequestParams::new(tool).with_arguments(arguments))
                .await?;
            if summary {
                println!("{}", serde_json::to_string_pretty(&safe_summary(&result)?)?);
            } else {
                println!("{}", serde_json::to_string_pretty(&result)?);
            }
        }
    }

    client.cancel().await?;
    Ok(())
}

struct Request {
    torii: PathBuf,
    config_dir: PathBuf,
    operation: Operation,
}

enum Operation {
    List,
    Call {
        tool: String,
        target: Option<String>,
        args: Vec<String>,
        summary: bool,
    },
}

impl Request {
    fn parse(mut args: Vec<String>) -> Result<Self, String> {
        if args.len() < 3 {
            return Err(usage());
        }
        let torii = PathBuf::from(args.remove(0));
        let config_dir = PathBuf::from(args.remove(0));
        let operation = match args.remove(0).as_str() {
            "list" if args.is_empty() => Operation::List,
            "call" => parse_call(args, false)?,
            "call-summary" => parse_call(args, true)?,
            _ => return Err(usage()),
        };
        Ok(Self {
            torii,
            config_dir,
            operation,
        })
    }
}

fn parse_call(mut args: Vec<String>, summary: bool) -> Result<Operation, String> {
    let tool = args.first().cloned().ok_or_else(usage)?;
    args.remove(0);
    let target = if args.first().map(String::as_str) == Some("--target") {
        args.remove(0);
        let target = args.first().cloned().ok_or_else(usage)?;
        args.remove(0);
        Some(target)
    } else {
        None
    };
    if args.first().map(String::as_str) != Some("--") {
        return Err(usage());
    }
    args.remove(0);
    if args.is_empty() {
        return Err(usage());
    }
    Ok(Operation::Call {
        tool,
        target,
        args,
        summary,
    })
}

fn usage() -> String {
    "usage: cargo run --example mcp_probe -- <torii-exe> <config-dir> list | call|call-summary <tool> [--target <alias>] -- <args...>".into()
}

fn safe_summary(result: &impl serde::Serialize) -> Result<Value, serde_json::Error> {
    let value = serde_json::to_value(result)?;
    Ok(safe_summary_value(&value))
}

fn safe_summary_value(value: &Value) -> Value {
    let structured = &value["structuredContent"];
    let execution = &structured["execution"];
    let error = structured["error"].as_str().unwrap_or_default();
    let execution_summary = if execution.is_object() {
        let stdout = execution["stdout"].as_str().unwrap_or_default();
        let stderr = execution["stderr"].as_str().unwrap_or_default();
        json!({
            "exit_code": execution["exit_code"],
            "stdout_present": !stdout.is_empty(),
            "stdout_bytes": stdout.len(),
            "stderr_present": !stderr.is_empty(),
            "stderr_bytes": stderr.len(),
            "stderr_class": classify_stderr(stderr),
            "truncated": execution["truncated"],
        })
    } else {
        Value::Null
    };
    json!({
        "is_error": value["isError"].as_bool().unwrap_or(false),
        "provider": structured.get("provider").cloned().unwrap_or(Value::Null),
        "target": structured.get("target").cloned().unwrap_or(Value::Null),
        "decision": {
            "result": structured["decision"]["result"],
            "source": structured["decision"]["source"],
        },
        "execution": execution_summary,
        "error_present": !error.is_empty(),
        "error_class": classify_error(error),
    })
}

fn classify_error(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.is_empty() {
        "none"
    } else if error.contains("target is required") {
        "target-required"
    } else if error.contains("unknown target") {
        "unknown-target"
    } else if error.contains("locked by target") {
        "blocked-option"
    } else if error.contains("authentication") && error.contains("cancelled") {
        "authentication-cancelled"
    } else {
        "other"
    }
}

fn classify_stderr(stderr: &str) -> &'static str {
    let stderr = stderr.to_ascii_lowercase();
    if stderr.is_empty() {
        "none"
    } else if stderr.contains("forbidden") {
        "forbidden"
    } else if stderr.contains("unauthorized")
        || stderr.contains("must be logged in")
        || stderr.contains("provide credentials")
    {
        "unauthorized"
    } else if stderr.contains("timed out")
        || stderr.contains("timeout")
        || stderr.contains("deadline exceeded")
    {
        "timeout"
    } else if stderr.contains("unable to connect")
        || stderr.contains("connection refused")
        || stderr.contains("no such host")
        || stderr.contains("dial tcp")
        || stderr.contains("tls handshake")
    {
        "network"
    } else {
        "other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn parses_list_and_calls_for_simple_and_targeted_tools() {
        let list = Request::parse(strings(&["torii", "config", "list"])).unwrap();
        assert!(matches!(list.operation, Operation::List));

        let aws = Request::parse(strings(&[
            "torii",
            "config",
            "call",
            "aws",
            "--",
            "sts",
            "get-caller-identity",
        ]))
        .unwrap();
        assert!(matches!(
            aws.operation,
            Operation::Call {
                tool,
                target: None,
                args,
                summary: false,
            } if tool == "aws" && args == strings(&["sts", "get-caller-identity"])
        ));

        let kubectl = Request::parse(strings(&[
            "torii", "config", "call", "kubectl", "--target", "lab", "--", "get", "pods",
        ]))
        .unwrap();
        assert!(matches!(
            kubectl.operation,
            Operation::Call {
                tool,
                target: Some(target),
                args,
                summary: false,
            } if tool == "kubectl" && target == "lab" && args == strings(&["get", "pods"])
        ));

        let summary = Request::parse(strings(&[
            "torii",
            "config",
            "call-summary",
            "kubectl",
            "--target",
            "lab",
            "--",
            "get",
            "pods",
        ]))
        .unwrap();
        assert!(matches!(
            summary.operation,
            Operation::Call { summary: true, .. }
        ));
    }

    #[test]
    fn rejects_calls_without_provider_arguments() {
        assert!(Request::parse(strings(&["torii", "config", "call", "aws", "--"])).is_err());
    }

    #[test]
    fn safe_summary_never_emits_stdout_or_stderr_content() {
        let result = json!({
            "isError": false,
            "structuredContent": {
                "provider": "kubectl",
                "target": "lab",
                "decision": { "result": "allow", "source": "rules" },
                "execution": {
                    "exit_code": 1,
                    "stdout": "sensitive-pod-name",
                    "stderr": "Forbidden: sensitive-resource-name",
                    "truncated": false
                }
            }
        });

        let summary = safe_summary_value(&result);
        let rendered = summary.to_string();
        assert_eq!(summary["execution"]["exit_code"], 1);
        assert_eq!(summary["execution"]["stderr_class"], "forbidden");
        assert!(!rendered.contains("sensitive-pod-name"));
        assert!(!rendered.contains("sensitive-resource-name"));
    }

    #[test]
    fn safe_summary_classifies_errors_without_emitting_the_message() {
        let result = json!({
            "isError": true,
            "structuredContent": {
                "error": "unknown target sensitive-client-name for provider kubectl"
            }
        });

        let summary = safe_summary_value(&result);
        let rendered = summary.to_string();
        assert_eq!(summary["error_class"], "unknown-target");
        assert!(!rendered.contains("sensitive-client-name"));
    }
}
