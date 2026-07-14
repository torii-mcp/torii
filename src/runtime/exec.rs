use crate::error::{Error, Result};
use serde::Serialize;
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;

const SIGNAL_EXIT: i32 = 143;

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

pub async fn run_command(
    program: &str,
    args_prefix: &[String],
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    max_output_bytes: usize,
) -> Result<ExecutionResult> {
    let mut command = Command::new(program);
    command
        .args(args_prefix)
        .args(args)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command.envs(persistent_env.iter().cloned());
    command.envs(auth_env.iter().cloned());
    let output = command.output().await.map_err(|source| Error::Spawn {
        program: program.to_string(),
        source,
    })?;
    let (stdout, stdout_cut) = bounded_text(&output.stdout, max_output_bytes);
    let remaining = max_output_bytes.saturating_sub(stdout.len());
    let (stderr, stderr_cut) = bounded_text(&output.stderr, remaining);
    Ok(ExecutionResult {
        exit_code: output.status.code().unwrap_or(SIGNAL_EXIT),
        stdout,
        stderr,
        truncated: stdout_cut || stderr_cut,
    })
}

pub async fn validate_command(
    program: &str,
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
) -> Result<bool> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.envs(persistent_env.iter().cloned());
    command.envs(auth_env.iter().cloned());
    let status = command.status().await.map_err(|source| Error::Spawn {
        program: program.to_string(),
        source,
    })?;
    Ok(status.success())
}

pub fn interpolate_environment(
    templates: &std::collections::BTreeMap<String, String>,
    fields: &HashMap<String, String>,
) -> Vec<(String, String)> {
    templates
        .iter()
        .filter_map(|(target, template)| {
            let source = template
                .strip_prefix("${")
                .and_then(|s| s.strip_suffix('}'))?;
            fields
                .get(source)
                .map(|value| (target.clone(), value.clone()))
        })
        .collect()
}

fn bounded_text(bytes: &[u8], limit: usize) -> (String, bool) {
    if bytes.len() <= limit {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let mut end = limit.min(bytes.len());
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    (String::from_utf8_lossy(&bytes[..end]).into_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_truncation_keeps_utf8_valid() {
        let (value, cut) = bounded_text("ábc".as_bytes(), 3);
        assert_eq!(value, "áb");
        assert!(cut);
    }

    #[test]
    fn environment_templates_only_use_declared_values() {
        let templates = [
            ("TOKEN".into(), "${SESSION}".into()),
            ("LITERAL".into(), "x".into()),
        ]
        .into_iter()
        .collect();
        let fields = [("SESSION".into(), "secret".into())].into_iter().collect();
        assert_eq!(
            interpolate_environment(&templates, &fields),
            vec![("TOKEN".into(), "secret".into())]
        );
    }
}
