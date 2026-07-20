use crate::audit;
use crate::config::{env_file, AuthPaths, ConfigPaths};
use crate::control;
use crate::error::{Error, Result};
use crate::providers::{AuthStrategy, IdentityProbe, Provider};
use crate::runtime::exec;
use std::collections::HashMap;
use std::io::Write;
use tokio::sync::Mutex;

pub struct SessionEnvironment<'a> {
    pub persistent_env: &'a [(String, String)],
    pub removed_env: &'a [&'a str],
}

pub async fn ensure_valid(
    root: &ConfigPaths,
    provider: &Provider,
    paths: &AuthPaths,
    auth_lock: &Mutex<()>,
    audit_scope: &str,
    environment: SessionEnvironment<'_>,
    force: bool,
) -> Result<Vec<(String, String)>> {
    let _guard = auth_lock.lock().await;
    paths.ensure()?;

    match provider.config.auth.strategy {
        AuthStrategy::Inherited => {
            ensure_inherited(
                root,
                provider,
                paths,
                audit_scope,
                environment.persistent_env,
                environment.removed_env,
                force,
            )
            .await
        }
        AuthStrategy::Environment => {
            ensure_environment(
                root,
                provider,
                paths,
                audit_scope,
                environment.persistent_env,
                environment.removed_env,
                force,
            )
            .await
        }
        strategy => Err(Error::AuthStrategyNotImplemented {
            provider: provider.config.name.clone(),
            strategy: strategy.to_string(),
        }),
    }
}

async fn ensure_inherited(
    root: &ConfigPaths,
    provider: &Provider,
    paths: &AuthPaths,
    audit_scope: &str,
    persistent_env: &[(String, String)],
    removed_env: &[&str],
    force: bool,
) -> Result<Vec<(String, String)>> {
    if force {
        return Err(Error::InvalidProvider {
            provider: provider.config.name.clone(),
            reason: "inherited authentication cannot be renewed by Torii".into(),
        });
    }
    if provider.config.auth.validate.is_none() {
        audit::log(root, audit_scope, "session-unchecked", "-", "");
        return Ok(Vec::new());
    }
    if session_cached(provider, paths) {
        return Ok(Vec::new());
    }
    if validate(provider, persistent_env, &[], removed_env).await? {
        record_success(paths);
        audit::log(root, audit_scope, "session-ok", "-", "");
        Ok(Vec::new())
    } else {
        Err(Error::SessionInvalid {
            provider: audit_scope.into(),
        })
    }
}

async fn ensure_environment(
    root: &ConfigPaths,
    provider: &Provider,
    paths: &AuthPaths,
    audit_scope: &str,
    persistent_env: &[(String, String)],
    removed_env: &[&str],
    force: bool,
) -> Result<Vec<(String, String)>> {
    if !force {
        let existing = load_auth_env(provider, paths)?;
        if session_cached(provider, paths) {
            return Ok(existing);
        }
        if !existing.is_empty()
            && validate(provider, persistent_env, &existing, removed_env).await?
        {
            record_success(paths);
            audit::log(root, audit_scope, "session-ok", "-", "");
            return Ok(existing);
        }
    }

    audit::log(
        root,
        audit_scope,
        if force {
            "reauth-forced"
        } else {
            "session-invalid"
        },
        "-",
        "",
    );
    let templates = provider.config.auth.inject.environment.clone();
    let validation = control::AuthValidation {
        command: provider
            .config
            .auth
            .validate
            .as_ref()
            .map(|spec| spec.command.clone()),
        args: provider
            .config
            .auth
            .validate
            .as_ref()
            .map_or_else(Vec::new, |spec| spec.args.clone()),
        persistent_env: persistent_env.to_vec(),
        environment_templates: templates.clone(),
    };
    let prompt =
        control::ask_auth(audit_scope, &provider.config.auth.fields, None, validation).await?;
    for _ in 0..prompt.invalid_attempts {
        audit::log(root, audit_scope, "session-candidate-invalid", "-", "");
    }
    let Some(fields) = prompt.fields else {
        return Err(Error::AuthCancelled(audit_scope.into()));
    };
    validate_required(provider, &fields)?;
    let candidate = exec::interpolate_environment(&templates, &fields);
    persist_credentials(provider, paths, &fields)?;
    record_success(paths);
    audit::log(root, audit_scope, "session-refreshed", "-", "");
    Ok(candidate)
}

/// Confirm the credentials in `auth_env` carry the identity the target expects.
///
/// Runs the identity provider's declared probe under *its own* command, parses
/// the named JSON field and compares it to `expected`. The result is cached per
/// scope so the probe does not run on every invocation. This is what turns a
/// wrong-account session from an opaque downstream failure into an explicit,
/// pre-flight error naming expected vs. observed identity.
#[allow(clippy::too_many_arguments)]
pub async fn verify_identity(
    root: &ConfigPaths,
    probe: &IdentityProbe,
    scope: &AuthPaths,
    audit_scope: &str,
    expected: &str,
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    removed_env: &[&str],
) -> Result<()> {
    if identity_cached(scope, probe.cache_ttl_seconds, expected) {
        return Ok(());
    }
    let mut args = probe.args.clone();
    // Force JSON so field extraction is deterministic, mirroring the AWS probe.
    if !args.iter().any(|arg| arg == "--output") {
        args.extend(["--output".into(), "json".into()]);
    }
    let observed = exec::run_command_with_removed_env(
        &probe.command,
        &[],
        &args,
        persistent_env,
        auth_env,
        removed_env,
        16 * 1024,
    )
    .await
    .ok()
    .filter(|out| out.exit_code == 0 && !out.truncated)
    .and_then(|out| identity_field(&out.stdout, &probe.field));
    let Some(observed) = observed else {
        audit::log(root, audit_scope, "identity-check-failed", "-", "");
        return Err(Error::IdentityCheckFailed {
            target: audit_scope.into(),
        });
    };
    if observed != expected {
        // The observed/expected identities travel only in the returned error,
        // never into the persisted audit log.
        audit::log(root, audit_scope, "identity-mismatch", "-", "");
        return Err(Error::IdentityMismatch {
            target: audit_scope.into(),
            expected: expected.into(),
            actual: observed,
        });
    }
    record_identity(scope, expected);
    audit::log(root, audit_scope, "identity-ok", "-", "");
    Ok(())
}

fn identity_field(stdout: &str, field: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(stdout).ok()?;
    match value.get(field)? {
        serde_json::Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    }
}

fn identity_cached(scope: &AuthPaths, ttl_seconds: u64, expected: &str) -> bool {
    let Ok(contents) = std::fs::read_to_string(scope.identity_cache()) else {
        return false;
    };
    // Cache is "<epoch> <identity>"; a change of expected identity misses.
    let mut parts = contents.trim().splitn(2, ' ');
    let Some(last) = parts.next().and_then(|value| value.parse::<u64>().ok()) else {
        return false;
    };
    if parts.next() != Some(expected) {
        return false;
    }
    audit::now_epoch().saturating_sub(last) < ttl_seconds
}

fn record_identity(scope: &AuthPaths, identity: &str) {
    let _ = std::fs::write(
        scope.identity_cache(),
        format!("{} {identity}", audit::now_epoch()),
    );
}

fn load_auth_env(provider: &Provider, paths: &AuthPaths) -> Result<Vec<(String, String)>> {
    let allowed: Vec<String> = provider
        .config
        .auth
        .fields
        .iter()
        .map(|field| field.name.clone())
        .collect();
    let pairs = env_file::load(&paths.credentials())?;
    let fields: HashMap<String, String> = pairs
        .into_iter()
        .filter(|(key, _)| allowed.contains(key))
        .collect();
    Ok(exec::interpolate_environment(
        &provider.config.auth.inject.environment,
        &fields,
    ))
}

async fn validate(
    provider: &Provider,
    persistent_env: &[(String, String)],
    auth_env: &[(String, String)],
    removed_env: &[&str],
) -> Result<bool> {
    let Some(spec) = &provider.config.auth.validate else {
        return Ok(true);
    };
    exec::validate_command_with_removed_env(
        &spec.command,
        &spec.args,
        persistent_env,
        auth_env,
        removed_env,
    )
    .await
}

fn validate_required(provider: &Provider, fields: &HashMap<String, String>) -> Result<()> {
    let missing: Vec<&str> = provider
        .config
        .auth
        .fields
        .iter()
        .filter(|f| f.required && fields.get(&f.name).is_none_or(|v| v.trim().is_empty()))
        .map(|f| f.name.as_str())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::InvalidArguments(format!(
            "missing required authentication fields: {}",
            missing.join(", ")
        )))
    }
}

fn persist_credentials(
    provider: &Provider,
    paths: &AuthPaths,
    fields: &HashMap<String, String>,
) -> Result<()> {
    paths.ensure()?;
    let path = paths.credentials();
    let ordered: Vec<(String, String)> = provider
        .config
        .auth
        .fields
        .iter()
        .filter_map(|field| {
            fields
                .get(&field.name)
                .map(|value| (field.name.clone(), value.clone()))
        })
        .collect();
    let mut temp =
        tempfile::NamedTempFile::new_in(paths.auth_dir()).map_err(|source| Error::Write {
            path: path.clone(),
            source,
        })?;
    temp.write_all(env_file::serialize(&ordered).as_bytes())
        .and_then(|_| temp.flush())
        .map_err(|source| Error::Write {
            path: path.clone(),
            source,
        })?;
    temp.persist(&path).map_err(|error| Error::Write {
        path,
        source: error.error,
    })?;
    Ok(())
}

fn session_cached(provider: &Provider, paths: &AuthPaths) -> bool {
    let Ok(value) = std::fs::read_to_string(paths.session_cache()) else {
        return false;
    };
    let Ok(last) = value.trim().parse::<u64>() else {
        return false;
    };
    audit::now_epoch().saturating_sub(last) < provider.config.auth.cache_ttl_seconds
}

fn record_success(paths: &AuthPaths) {
    let _ = std::fs::write(paths.session_cache(), audit::now_epoch().to_string());
}
