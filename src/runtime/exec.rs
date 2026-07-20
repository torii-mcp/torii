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

pub struct RunningCommand {
    program: String,
    child: tokio::process::Child,
    max_output_bytes: usize,
}

impl RunningCommand {
    pub async fn wait(self) -> Result<ExecutionResult> {
        let output = self
            .child
            .wait_with_output()
            .await
            .map_err(|source| Error::Spawn {
                program: self.program,
                source,
            })?;
        let (stdout, stdout_cut) = bounded_text(&output.stdout, self.max_output_bytes);
        let remaining = self.max_output_bytes.saturating_sub(stdout.len());
        let (stderr, stderr_cut) = bounded_text(&output.stderr, remaining);
        Ok(ExecutionResult {
            exit_code: output.status.code().unwrap_or(SIGNAL_EXIT),
            stdout,
            stderr,
            truncated: stdout_cut || stderr_cut,
        })
    }
}

pub async fn run_command(
    program: &str,
    args_prefix: &[String],
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    max_output_bytes: usize,
) -> Result<ExecutionResult> {
    run_command_with_removed_env(
        program,
        args_prefix,
        args,
        persistent_env,
        auth_env,
        &[],
        max_output_bytes,
    )
    .await
}

pub async fn run_command_with_removed_env(
    program: &str,
    args_prefix: &[String],
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    removed_env: &[&str],
    max_output_bytes: usize,
) -> Result<ExecutionResult> {
    spawn_command_with_removed_env(
        program,
        args_prefix,
        args,
        persistent_env,
        auth_env,
        removed_env,
        max_output_bytes,
    )?
    .wait()
    .await
}

pub fn spawn_command_with_removed_env(
    program: &str,
    args_prefix: &[String],
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    removed_env: &[&str],
    max_output_bytes: usize,
) -> Result<RunningCommand> {
    let mut command = Command::new(program);
    command
        .args(args_prefix)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    remove_environment(&mut command, removed_env);
    command.envs(persistent_env.iter().cloned());
    command.envs(auth_env.iter().cloned());
    let child = command.spawn().map_err(|source| Error::Spawn {
        program: program.to_string(),
        source,
    })?;
    Ok(RunningCommand {
        program: program.into(),
        child,
        max_output_bytes,
    })
}

pub async fn validate_command(
    program: &str,
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
) -> Result<bool> {
    validate_command_with_removed_env(program, args, persistent_env, auth_env, &[]).await
}

pub async fn validate_command_with_removed_env(
    program: &str,
    args: &[String],
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    removed_env: &[&str],
) -> Result<bool> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    remove_environment(&mut command, removed_env);
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

fn remove_environment(command: &mut Command, removed_env: &[&str]) {
    for name in removed_env {
        command.env_remove(name);
    }
    if removed_env.contains(&"AWS_ENDPOINT_URL") {
        for (name, _) in std::env::vars_os() {
            let text = name.to_string_lossy();
            if text.to_ascii_uppercase().starts_with("AWS_ENDPOINT_URL_") {
                command.env_remove(name);
            }
        }
    }
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
