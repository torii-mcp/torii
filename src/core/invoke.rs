use serde::Serialize;
use std::path::Path;

use crate::audit;
use crate::config::{env_file, AuthPaths, ConfigPaths, Settings};
use crate::control::{self, AccessChoice};
use crate::error::{Error, Result};
use crate::jasper::grants;
use crate::jasper::rules::{self, Evaluation};
use crate::jasper::{DecisionResult, PolicyDecision};
use crate::providers::auth::session;
use crate::providers::{Provider, ProviderRegistry, TargetMode};
use crate::runtime::exec::{self, ExecutionResult};

#[derive(Debug, Clone, Serialize)]
pub struct InvocationResult {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub decision: PolicyDecision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution: Option<ExecutionResult>,
}

#[derive(Clone)]
pub struct Invoker {
    paths: ConfigPaths,
    settings: Settings,
    registry: ProviderRegistry,
}

struct InvocationScope {
    target: Option<String>,
    audit_scope: String,
    rules: std::path::PathBuf,
    grants: std::path::PathBuf,
    auth_paths: AuthPaths,
    auth_lock: std::sync::Arc<tokio::sync::Mutex<()>>,
    target_args: Vec<String>,
    target_env: Option<std::path::PathBuf>,
}

impl Invoker {
    pub fn new(paths: ConfigPaths, settings: Settings, registry: ProviderRegistry) -> Self {
        Self {
            paths,
            settings,
            registry,
        }
    }

    pub fn registry(&self) -> &ProviderRegistry {
        &self.registry
    }

    pub async fn invoke(
        &self,
        tool: &str,
        target_name: Option<&str>,
        args: &[String],
    ) -> Result<InvocationResult> {
        if args.is_empty() {
            return Err(Error::InvalidArguments(
                "args must contain at least one string".into(),
            ));
        }
        let provider = self
            .registry
            .get(tool)
            .ok_or_else(|| Error::ProviderNotFound(tool.into()))?;
        let scope = resolve_scope(&provider, target_name, args)?;
        let grant_rule = grants::derive_rule(args, &provider.config.policy.grant_rule);
        let audit_rule = args
            .iter()
            .take(2)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" ");
        audit::log(&self.paths, &scope.audit_scope, "invoke", &audit_rule, "");

        let policy = rules::load(&scope.rules)?;
        for invalid in policy.invalid_accepts(provider.config.policy.minimum_accept_tokens) {
            audit::log(
                &self.paths,
                &scope.audit_scope,
                "invalid-accept",
                invalid,
                "ignored",
            );
        }

        let decision = match policy.evaluate(args, provider.config.policy.minimum_accept_tokens) {
            Evaluation::DeniedExplicit { rule } => {
                let decision = PolicyDecision {
                    result: DecisionResult::Deny,
                    source: "explicit-deny".into(),
                    rule: Some(rule.clone()),
                };
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "denied-explicit",
                    &rule,
                    "",
                );
                return Ok(invocation_result(&provider, &scope, decision, None));
            }
            Evaluation::Allowed { rule } => {
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "allowed-by-rules",
                    &rule,
                    "",
                );
                PolicyDecision {
                    result: DecisionResult::Allow,
                    source: "rules".into(),
                    rule: Some(rule),
                }
            }
            Evaluation::Unresolved => {
                self.resolve_unresolved(
                    &scope.audit_scope,
                    &scope.grants,
                    args,
                    &grant_rule,
                    &audit_rule,
                )
                .await?
            }
        };

        if decision.result == DecisionResult::Deny {
            return Ok(invocation_result(&provider, &scope, decision, None));
        }

        // This is intentionally below every policy decision. A denied invocation never
        // reads persistent environment or authentication material.
        let mut persistent_env = env_file::load(
            &provider
                .paths
                .base()
                .join(&provider.config.environment.file),
        )?;
        if let Some(path) = &scope.target_env {
            merge_environment(&mut persistent_env, env_file::load(path)?);
        }
        let auth_env = session::ensure_valid(
            &self.paths,
            &provider,
            &scope.auth_paths,
            scope.auth_lock.as_ref(),
            &scope.audit_scope,
            &persistent_env,
            false,
        )
        .await?;
        let mut prefix = provider.config.args_prefix.clone();
        prefix.extend(scope.target_args.iter().cloned());
        let execution = exec::run_command(
            &provider.config.command,
            &prefix,
            args,
            &persistent_env,
            &auth_env,
            self.settings.max_output_bytes,
        )
        .await?;
        audit::log(
            &self.paths,
            &scope.audit_scope,
            "ran",
            &audit_rule,
            &format!("exit={}", execution.exit_code),
        );
        Ok(invocation_result(
            &provider,
            &scope,
            decision,
            Some(execution),
        ))
    }

    async fn resolve_unresolved(
        &self,
        audit_scope: &str,
        grants_path: &Path,
        args: &[String],
        grant_rule: &str,
        audit_rule: &str,
    ) -> Result<PolicyDecision> {
        let now = audit::now_epoch();
        let active = grants::load_active(grants_path, now);
        if let Some(rule) = grants::matching_grant(&active, args, now) {
            audit::log(&self.paths, audit_scope, "allowed-by-grant", audit_rule, "");
            return Ok(PolicyDecision {
                result: DecisionResult::Allow,
                source: "grant".into(),
                rule: Some(rule),
            });
        }
        let choice = control::ask_access(
            audit_scope,
            &args.join(" "),
            grant_rule,
            self.settings.default_grant_minutes,
        )
        .await?;
        match choice {
            AccessChoice::Deny => {
                audit::log(&self.paths, audit_scope, "denied-interface", audit_rule, "");
                Ok(PolicyDecision {
                    result: DecisionResult::Deny,
                    source: "human-deny".into(),
                    rule: None,
                })
            }
            AccessChoice::AllowOnce => {
                audit::log(&self.paths, audit_scope, "override-once", audit_rule, "");
                Ok(PolicyDecision {
                    result: DecisionResult::Allow,
                    source: "human-once".into(),
                    rule: Some(grant_rule.into()),
                })
            }
            AccessChoice::AllowFor(minutes) => {
                grants::add(grants_path, grant_rule, now + u64::from(minutes) * 60, now)?;
                audit::log(
                    &self.paths,
                    audit_scope,
                    "override-timed",
                    audit_rule,
                    &format!("{minutes}min"),
                );
                Ok(PolicyDecision {
                    result: DecisionResult::Allow,
                    source: "human-grant".into(),
                    rule: Some(grant_rule.into()),
                })
            }
        }
    }
}

fn resolve_scope(
    provider: &Provider,
    target_name: Option<&str>,
    args: &[String],
) -> Result<InvocationScope> {
    let Some(targeting) = &provider.config.targeting else {
        if target_name.is_some() {
            return Err(Error::InvalidArguments(format!(
                "provider tool {:?} does not accept a target",
                provider.config.tool
            )));
        }
        return Ok(InvocationScope {
            target: None,
            audit_scope: provider.config.name.clone(),
            rules: provider.paths.rules(),
            grants: provider.paths.grants(),
            auth_paths: provider.paths.auth_paths(),
            auth_lock: provider.auth_lock.clone(),
            target_args: Vec::new(),
            target_env: None,
        });
    };

    let target_name = target_name.ok_or_else(|| {
        Error::InvalidArguments(format!(
            "target is required for provider tool {:?}",
            provider.config.tool
        ))
    })?;
    let target = provider.target(target_name).ok_or_else(|| {
        let available = provider.target_names().collect::<Vec<_>>().join(", ");
        Error::InvalidArguments(format!(
            "unknown target {target_name:?} for provider tool {:?}; available: [{}]",
            provider.config.tool, available
        ))
    })?;
    if let Some(argument) = args
        .iter()
        .find(|argument| targeting.rejects_argument(argument))
    {
        return Err(Error::InvalidArguments(format!(
            "argument {argument:?} is locked by target {target_name:?}"
        )));
    }

    let target_rules = target.paths.rules();
    let rules = if target_rules.exists() {
        target_rules
    } else {
        provider.paths.rules()
    };
    let target_args = match targeting.mode {
        TargetMode::KubectlContext => vec!["--context".into(), target.config.context.clone()],
    };
    Ok(InvocationScope {
        target: Some(target.config.name.clone()),
        audit_scope: format!("{}/{}", provider.config.name, target.config.name),
        rules,
        grants: target.paths.grants(),
        auth_paths: target.paths.auth_paths(),
        auth_lock: target.auth_lock.clone(),
        target_args,
        target_env: Some(target.paths.env()),
    })
}

fn invocation_result(
    provider: &Provider,
    scope: &InvocationScope,
    decision: PolicyDecision,
    execution: Option<ExecutionResult>,
) -> InvocationResult {
    InvocationResult {
        provider: provider.config.name.clone(),
        target: scope.target.clone(),
        decision,
        execution,
    }
}

fn merge_environment(base: &mut Vec<(String, String)>, overrides: Vec<(String, String)>) {
    for (key, value) in overrides {
        if let Some((_, existing)) = base.iter_mut().find(|(candidate, _)| candidate == &key) {
            *existing = value;
        } else {
            base.push((key, value));
        }
    }
}
