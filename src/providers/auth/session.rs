use crate::audit;
use crate::config::{env_file, AuthPaths, ConfigPaths};
use crate::control;
use crate::error::{Error, Result};
use crate::providers::{AuthStrategy, Provider};
use crate::runtime::exec;
use std::collections::HashMap;
use std::io::Write;
use tokio::sync::Mutex;

pub async fn ensure_valid(
    root: &ConfigPaths,
    provider: &Provider,
    paths: &AuthPaths,
    auth_lock: &Mutex<()>,
    audit_scope: &str,
    persistent_env: &[(String, String)],
    force: bool,
) -> Result<Vec<(String, String)>> {
    let _guard = auth_lock.lock().await;
    paths.ensure()?;

    match provider.config.auth.strategy {
        AuthStrategy::Inherited => {
            ensure_inherited(root, provider, paths, audit_scope, persistent_env, force).await
        }
        AuthStrategy::Environment => {
            ensure_environment(root, provider, paths, audit_scope, persistent_env, force).await
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
    force: bool,
) -> Result<Vec<(String, String)>> {
    if force {
        return Err(Error::InvalidProvider {
            provider: provider.config.name.clone(),
            reason: "inherited authentication cannot be renewed by Torii".into(),
        });
    }
    if session_cached(provider, paths) {
        return Ok(Vec::new());
    }
    if validate(provider, persistent_env, &[]).await? {
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
    force: bool,
) -> Result<Vec<(String, String)>> {
    if !force {
        let existing = load_auth_env(provider, paths)?;
        if session_cached(provider, paths) {
            return Ok(existing);
        }
        if !existing.is_empty() && validate(provider, persistent_env, &existing).await? {
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
    let mut validation_error = None;
    loop {
        let Some(fields) = control::ask_auth(
            audit_scope,
            &provider.config.auth.fields,
            validation_error.as_deref(),
        )
        .await?
        else {
            return Err(Error::AuthCancelled(audit_scope.into()));
        };
        validate_required(provider, &fields)?;
        let candidate =
            exec::interpolate_environment(&provider.config.auth.inject.environment, &fields);
        if validate(provider, persistent_env, &candidate).await? {
            persist_credentials(provider, paths, &fields)?;
            record_success(paths);
            audit::log(root, audit_scope, "session-refreshed", "-", "");
            return Ok(candidate);
        }
        audit::log(root, audit_scope, "session-candidate-invalid", "-", "");
        validation_error = Some(
            "A sessão foi recusada pelo comando de validação. Revise os dados e tente novamente."
                .to_string(),
        );
    }
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
) -> Result<bool> {
    let Some(spec) = &provider.config.auth.validate else {
        return Ok(true);
    };
    exec::validate_command(&spec.command, &spec.args, persistent_env, auth_env).await
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
