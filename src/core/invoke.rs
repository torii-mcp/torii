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
    direct_auth: Option<(AuthPaths, std::sync::Arc<tokio::sync::Mutex<()>>)>,
    target_args: Vec<String>,
    target_env: Option<std::path::PathBuf>,
    lifecycle_provider: Option<String>,
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
                self.resolve_unresolved(&scope.audit_scope, &scope.grants, args, &audit_rule)
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
        let auth_env = if let Some(lifecycle_provider) = &scope.lifecycle_provider {
            self.ensure_target_provider(&scope, lifecycle_provider)
                .await?
        } else {
            let (auth_paths, auth_lock) =
                scope
                    .direct_auth
                    .as_ref()
                    .ok_or_else(|| Error::InvalidProvider {
                        provider: provider.config.name.clone(),
                        reason: "invocation has no authentication scope".into(),
                    })?;
            session::ensure_valid(
                &self.paths,
                &provider,
                auth_paths,
                auth_lock.as_ref(),
                &scope.audit_scope,
                &persistent_env,
                false,
            )
            .await?
        };
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
        audit_rule: &str,
    ) -> Result<PolicyDecision> {
        let now = audit::now_epoch();
        let loaded = grants::load_active(grants_path, now);
        if loaded.legacy_ignored {
            audit::log(
                &self.paths,
                audit_scope,
                "legacy-grants-ignored",
                audit_rule,
                "reapproval-required",
            );
        }
        if loaded.invalid_ignored {
            audit::log(
                &self.paths,
                audit_scope,
                "invalid-grants-ignored",
                audit_rule,
                "reapproval-required",
            );
        }
        if let Some(evidence) = grants::matching_grant(&loaded.active, args, now) {
            audit::log(&self.paths, audit_scope, "allowed-by-grant", audit_rule, "");
            return Ok(PolicyDecision {
                result: DecisionResult::Allow,
                source: "grant".into(),
                rule: Some(evidence.reference()),
            });
        }
        let choice =
            control::ask_access(audit_scope, args, self.settings.default_grant_minutes).await?;
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
                    rule: None,
                })
            }
            AccessChoice::AllowFor { minutes, selection } => {
                let Some(matcher) = grants::GrantMatcher::from_selection(args, selection) else {
                    audit::log(
                        &self.paths,
                        audit_scope,
                        "denied-interface",
                        audit_rule,
                        "invalid-grant-selection",
                    );
                    return Ok(PolicyDecision {
                        result: DecisionResult::Deny,
                        source: "human-deny".into(),
                        rule: None,
                    });
                };
                let evidence =
                    grants::add(grants_path, &matcher, now + u64::from(minutes) * 60, now)?;
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
                    rule: Some(evidence.reference()),
                })
            }
        }
    }

    async fn ensure_target_provider(
        &self,
        scope: &InvocationScope,
        tool: &str,
    ) -> Result<Vec<(String, String)>> {
        audit::log(
            &self.paths,
            &scope.audit_scope,
            "preflight-provider",
            tool,
            "",
        );
        let result = async {
            let provider = self
                .registry
                .get(tool)
                .ok_or_else(|| Error::ProviderNotFound(tool.into()))?;
            let mut environment = env_file::load(
                &provider
                    .paths
                    .base()
                    .join(&provider.config.environment.file),
            )?;
            let session_env = session::ensure_valid(
                &self.paths,
                &provider,
                &provider.paths.auth_paths(),
                provider.auth_lock.as_ref(),
                &provider.config.name,
                &environment,
                false,
            )
            .await?;
            merge_environment(&mut environment, session_env);
            Ok(environment)
        }
        .await;
        audit::log(
            &self.paths,
            &scope.audit_scope,
            if result.is_ok() {
                "preflight-ok"
            } else {
                "preflight-failed"
            },
            tool,
            "",
        );
        result
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
            direct_auth: Some((provider.paths.auth_paths(), provider.auth_lock.clone())),
            target_args: Vec::new(),
            target_env: None,
            lifecycle_provider: None,
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
        direct_auth: None,
        target_args,
        target_env: Some(target.paths.env()),
        lifecycle_provider: Some(target.config.provider.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn target_provider_returns_persistent_and_session_environment() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());

        let auth = paths.provider("auth");
        auth.ensure().unwrap();
        fs::write(
            auth.config(),
            r#"
version: "1"
name: auth
tool: auth
description: authentication provider
command: executable-not-used
auth:
  strategy: environment
  fields:
    - { name: SESSION, required: true, secret: true }
  inject:
    environment: { SESSION_TOKEN: "${SESSION}" }
  validate: { command: executable-not-used, args: [] }
environment: { file: .env }
"#,
        )
        .unwrap();
        fs::write(auth.env(), "AUTH_PROFILE=test\n").unwrap();
        fs::write(auth.credentials(), "SESSION=fake-secret\n").unwrap();
        fs::write(auth.session_cache(), audit::now_epoch().to_string()).unwrap();

        let target_provider = paths.provider("target");
        target_provider.ensure().unwrap();
        fs::write(
            target_provider.config(),
            r#"
version: "1"
name: target
tool: target
description: targeted provider
command: executable-not-used
targeting: { mode: kubectl_context }
auth: { strategy: inherited }
environment: { file: .env }
"#,
        )
        .unwrap();
        fs::write(
            target_provider.rules(),
            "version: '1.0'\ndeny: []\naccept: ['get']\n",
        )
        .unwrap();
        let target = target_provider.target("lab");
        target.ensure().unwrap();
        fs::write(
            target.config(),
            "version: '1'\nname: lab\ncontext: local\nprovider: auth\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("target").unwrap();
        let scope = resolve_scope(&provider, Some("lab"), &["get".into()]).unwrap();
        let invoker = Invoker::new(paths, Settings::default(), registry);
        let environment = invoker
            .ensure_target_provider(&scope, "auth")
            .await
            .unwrap();

        assert!(environment.contains(&("AUTH_PROFILE".into(), "test".into())));
        assert!(environment.contains(&("SESSION_TOKEN".into(), "fake-secret".into())));
    }
}
