use serde::Serialize;

use crate::audit;
use crate::config::{env_file, ConfigPaths, Settings};
use crate::control::{self, AccessChoice, ActiveTargetAuthorization, TargetAccessChoice};
use crate::error::{Error, Result};
use crate::jasper::grants;
use crate::jasper::rules::{self, Evaluation};
use crate::jasper::{DecisionResult, PolicyDecision};
use crate::policy;
use crate::providers::auth::session;
use crate::providers::config::IgnoreArgs;
use crate::providers::{AuthStrategy, Provider, ProviderRegistry, TargetMode};
use crate::runtime::exec::{self, ExecutionResult};
use crate::target_access::{self, ActivationMode, ActivationOutcome, GuardedActivation};

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
    /// The shared provider policy: it always applies and carries the deny floor.
    rules: std::path::PathBuf,
    /// The target's own policy, when the provider uses targets. Layered over the
    /// shared one: it adds denies and replaces accepts.
    target_rules: Option<std::path::PathBuf>,
    grants: std::path::PathBuf,
    /// Extra argv appended to the invoked command (e.g. `--context`, `--profile`).
    target_args: Vec<String>,
    /// The target's `.env` overlay for the invoked process.
    target_env: Option<std::path::PathBuf>,
    /// Values Torii injects itself (e.g. `AWS_PROFILE`).
    trusted_env: Vec<(String, String)>,
    auth: AuthScope,
}

/// Which provider authenticates this invocation, into which credential bucket,
/// the identity those credentials must carry, and an optional profile to inject.
struct AuthScope {
    provider: String,
    scope: String,
    expect: Option<String>,
    profile: Option<String>,
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

    /// Authenticate a credential scope through its identity provider, then
    /// confirm the session carries the expected identity. Returns the auth
    /// environment to inject into the invoked process.
    async fn authenticate_scope(
        &self,
        scope: &InvocationScope,
        auth_provider: &Provider,
        auth_paths: &crate::config::AuthPaths,
        auth_lock: &tokio::sync::Mutex<()>,
        removed_env: &[&str],
    ) -> Result<(Vec<(String, String)>, Vec<(String, String)>)> {
        // The identity provider authenticates with its own environment. Session
        // and identity events are audited under that provider's name, while the
        // caller records preflight markers under the invocation scope.
        let idp_env = env_file::load(
            &auth_provider
                .paths
                .base()
                .join(&auth_provider.config.environment.file),
        )?;
        let session = session::ensure_valid(
            &self.paths,
            auth_provider,
            auth_paths,
            auth_lock,
            &auth_provider.config.name,
            session::SessionEnvironment {
                persistent_env: &idp_env,
                removed_env,
            },
            false,
        )
        .await;
        let auth_env = match session {
            Ok(environment) => environment,
            // Inherited-with-validator means the human must log in outside Torii
            // (SSO/profile); surface that instead of an opaque session error.
            Err(Error::SessionInvalid { .. })
                if matches!(auth_provider.config.auth.strategy, AuthStrategy::Inherited)
                    && auth_provider.config.auth.validate.is_some() =>
            {
                return Err(Error::AwsProfileAuthenticationRequired {
                    target: scope.audit_scope.clone(),
                });
            }
            Err(error) => return Err(error),
        };

        // Confirm the session carries the identity the target expects, before we
        // let a single command touch the target.
        if let (Some(expect), Some(probe)) =
            (&scope.auth.expect, &auth_provider.config.auth.identity)
        {
            session::verify_identity(
                &self.paths,
                probe,
                auth_paths,
                &auth_provider.config.name,
                expect,
                &idp_env,
                &auth_env,
                removed_env,
            )
            .await?;
        }
        Ok((idp_env, auth_env))
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

        let policy = rules::load_effective(&scope.rules, scope.target_rules.as_deref())?;

        // Forbidden arguments close channels the policy cannot inspect (a query
        // supplied via file or stdin). Rejected in any position, before any rule
        // is evaluated, and never executed.
        if let Some(argument) = args.iter().find(|arg| provider.config.policy.rejects(arg)) {
            let decision = PolicyDecision {
                result: DecisionResult::Deny,
                source: "forbidden-arg".into(),
                rule: Some(argument.clone()),
            };
            audit::log(
                &self.paths,
                &scope.audit_scope,
                "denied-forbidden-arg",
                argument,
                "",
            );
            return Ok(invocation_result(&provider, &scope, decision, None));
        }

        let min_tokens = provider.config.policy.minimum_accept_tokens;
        for invalid in policy.invalid_accepts(min_tokens) {
            audit::log(
                &self.paths,
                &scope.audit_scope,
                "invalid-accept",
                invalid,
                "ignored",
            );
        }

        // Normalize argv for evaluation only — the executed command still uses the
        // original args. Compile up front so a malformed regex fails closed
        // (an error, never a silent allow).
        let normalized = provider.config.policy.ignore_args.normalize(args);
        let compiled = match policy.compile() {
            Ok(compiled) => compiled,
            Err(error) => {
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "invalid-rule",
                    &error.to_string(),
                    "",
                );
                return Err(error);
            }
        };
        let evaluation = compiled.evaluate(&normalized, min_tokens);
        if let Evaluation::DeniedExplicit { rule } = &evaluation {
            let decision = PolicyDecision {
                result: DecisionResult::Deny,
                source: "explicit-deny".into(),
                rule: Some(rule.clone()),
            };
            audit::log(&self.paths, &scope.audit_scope, "denied-explicit", rule, "");
            return Ok(invocation_result(&provider, &scope, decision, None));
        }

        if !self.ensure_target_access(&provider, &scope).await? {
            return Ok(target_access_denied_result(
                &provider,
                &scope,
                "target-inactive",
            ));
        }

        let decision = match evaluation {
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
                    &provider.config.policy.ignore_args,
                    &scope,
                    &policy,
                    args,
                    min_tokens,
                    &audit_rule,
                )
                .await?
            }
            Evaluation::DeniedExplicit { .. } => unreachable!("explicit deny returned above"),
        };

        if decision.result == DecisionResult::Deny {
            return Ok(invocation_result(&provider, &scope, decision, None));
        }

        if !self.target_access_is_active(&provider, &scope)? {
            audit::log(
                &self.paths,
                &scope.audit_scope,
                "target-access-lost",
                scope.target.as_deref().unwrap_or("-"),
                "before-environment",
            );
            return Ok(target_access_denied_result(
                &provider,
                &scope,
                "target-access-lost",
            ));
        }

        // This is intentionally below every policy decision. A denied invocation never
        // reads persistent environment or authentication material.
        //
        // Authentication is scoped to a (provider, bucket) pair, so targets in
        // different accounts never share a session, while targets that opt into
        // the same scope reuse one. The identity provider authenticates with its
        // own environment; the resulting credentials are injected into the
        // invoked process.
        let auth_provider = self
            .registry
            .get(&scope.auth.provider)
            .ok_or_else(|| Error::ProviderNotFound(scope.auth.provider.clone()))?;
        let auth_paths = auth_provider.paths.identity_scope(&scope.auth.scope);
        let auth_lock = auth_provider.auth_lock(&scope.auth.scope);
        let removed_owned = auth_provider.config.auth.removed_env.clone();
        let removed_env: Vec<&str> = removed_owned.iter().map(String::as_str).collect();

        audit::log(
            &self.paths,
            &scope.audit_scope,
            "preflight-provider",
            &scope.auth.provider,
            "",
        );
        let auth_env = match self
            .authenticate_scope(
                &scope,
                &auth_provider,
                &auth_paths,
                auth_lock.as_ref(),
                &removed_env,
            )
            .await
        {
            Ok((_idp_env, auth_env)) => {
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "preflight-ok",
                    &scope.auth.provider,
                    "",
                );
                auth_env
            }
            Err(error) => {
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "preflight-failed",
                    &scope.auth.provider,
                    "",
                );
                return Err(error);
            }
        };

        // Build the invoked process environment: the invoked provider's own
        // `.env`, the target overlay, then Torii's trusted injections. Ambient
        // keys the identity provider owns are stripped so injected credentials
        // and profile win deterministically.
        let mut persistent_env = env_file::load(
            &provider
                .paths
                .base()
                .join(&provider.config.environment.file),
        )?;
        if let Some(path) = &scope.target_env {
            merge_environment(&mut persistent_env, env_file::load(path)?);
        }
        strip_keys(&mut persistent_env, &removed_env);
        merge_environment(&mut persistent_env, scope.trusted_env.clone());
        // Inject the target's profile last, so it wins over any stripped ambient
        // value. Requires the identity provider to declare which var carries it.
        if let (Some(profile), Some(env)) =
            (&scope.auth.profile, &auth_provider.config.auth.profile_env)
        {
            merge_environment(&mut persistent_env, vec![(env.clone(), profile.clone())]);
        }

        if !self.target_access_is_active(&provider, &scope)? {
            audit::log(
                &self.paths,
                &scope.audit_scope,
                "target-access-lost",
                scope.target.as_deref().unwrap_or("-"),
                "after-authentication",
            );
            return Ok(target_access_denied_result(
                &provider,
                &scope,
                "target-access-lost",
            ));
        }
        let mut prefix = provider.config.args_prefix.clone();
        prefix.extend(scope.target_args.iter().cloned());
        let running = if let Some(target) = &scope.target {
            let known = target_access::known_targets(&provider)?;
            target_access::run_if_active(
                &provider.paths.target_authorizations(),
                &provider.paths.target_authorizations_lock(),
                &known,
                target,
                || {
                    exec::spawn_command_with_removed_env(
                        &provider.config.command,
                        &prefix,
                        args,
                        &persistent_env,
                        &auth_env,
                        &removed_env,
                        self.settings.max_output_bytes,
                    )
                },
            )?
        } else {
            Some(exec::spawn_command_with_removed_env(
                &provider.config.command,
                &prefix,
                args,
                &persistent_env,
                &auth_env,
                &removed_env,
                self.settings.max_output_bytes,
            )?)
        };
        let Some(running) = running else {
            audit::log(
                &self.paths,
                &scope.audit_scope,
                "target-access-lost",
                scope.target.as_deref().unwrap_or("-"),
                "at-launch",
            );
            return Ok(target_access_denied_result(
                &provider,
                &scope,
                "target-access-lost",
            ));
        };
        let execution = running.wait().await?;
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

    async fn ensure_target_access(
        &self,
        provider: &Provider,
        scope: &InvocationScope,
    ) -> Result<bool> {
        let Some(target_name) = scope.target.as_deref() else {
            return Ok(true);
        };

        // Serialize prompts inside this MCP server. The state file has its own
        // cross-process lock because the human CLI may update it concurrently.
        let _prompt_lock = provider.target_access_lock.lock().await;
        let known = target_access::known_targets(provider)?;
        let snapshot = target_access::load(
            &provider.paths.target_authorizations(),
            &known,
            audit::now_epoch(),
        )?;
        if snapshot
            .active
            .iter()
            .any(|authorization| authorization.target == target_name)
        {
            return Ok(true);
        }

        audit::log(
            &self.paths,
            &scope.audit_scope,
            "target-access-requested",
            target_name,
            "",
        );
        let requested_binding = target_access::human_binding(provider, target_name)?;
        let active_targets = snapshot
            .active
            .iter()
            .map(|authorization| {
                Ok(ActiveTargetAuthorization {
                    target: authorization.target.clone(),
                    display_binding: target_access::human_binding(provider, &authorization.target)?,
                    expires_at_epoch: authorization.expires_at,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let choice = control::ask_target_access(
            &provider.config.tool,
            target_name,
            &requested_binding,
            &active_targets,
            self.settings.default_target_minutes,
        )
        .await?;
        let (minutes, mode, event) = match choice {
            TargetAccessChoice::Deny => {
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "target-access-denied",
                    target_name,
                    "",
                );
                return Ok(false);
            }
            TargetAccessChoice::Replace { minutes } => {
                (minutes, ActivationMode::Replace, "target-access-replaced")
            }
            TargetAccessChoice::Add { minutes } => {
                (minutes, ActivationMode::Add, "target-access-added")
            }
        };
        target_access::validate_duration(minutes)?;
        match target_access::activate_if_unchanged(
            &provider.paths.target_authorizations(),
            &provider.paths.target_authorizations_lock(),
            &known,
            target_name,
            audit::now_epoch(),
            minutes,
            GuardedActivation {
                mode,
                expected_revision: &snapshot.revision,
            },
        )? {
            ActivationOutcome::Applied(active) => {
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    event,
                    target_name,
                    &format!("{minutes}min active={}", active.len()),
                );
                Ok(true)
            }
            ActivationOutcome::StateChanged => {
                let now_active = target_access::is_active(
                    &provider.paths.target_authorizations(),
                    &known,
                    target_name,
                    audit::now_epoch(),
                )?;
                audit::log(
                    &self.paths,
                    &scope.audit_scope,
                    "target-access-stale",
                    target_name,
                    if now_active {
                        "already-active"
                    } else {
                        "retry-required"
                    },
                );
                Ok(now_active)
            }
        }
    }

    fn target_access_is_active(
        &self,
        provider: &Provider,
        scope: &InvocationScope,
    ) -> Result<bool> {
        let Some(target_name) = scope.target.as_deref() else {
            return Ok(true);
        };
        let known = target_access::known_targets(provider)?;
        target_access::is_active(
            &provider.paths.target_authorizations(),
            &known,
            target_name,
            audit::now_epoch(),
        )
    }

    async fn resolve_unresolved(
        &self,
        ignore: &IgnoreArgs,
        scope: &InvocationScope,
        policy: &rules::Rules,
        args: &[String],
        minimum: usize,
        audit_rule: &str,
    ) -> Result<PolicyDecision> {
        let audit_scope = &scope.audit_scope;
        let grants_path = &scope.grants;
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
        // `now` is captured before the prompt, so an alignment offered here lands
        // exactly on the older grant's deadline no matter how long the human
        // takes to decide.
        let align_seconds = grants::oldest_active_expiry(&loaded.active)
            .map(|expiry| u32::try_from(expiry.saturating_sub(now)).unwrap_or(u32::MAX))
            .filter(|remaining| *remaining > 0);
        let permanent = permanent_options(ignore, scope, policy, args, minimum);
        let choice = control::ask_access(
            audit_scope,
            args,
            self.settings.default_grant_minutes,
            align_seconds,
            permanent,
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
                    rule: None,
                })
            }
            AccessChoice::AllowFor { seconds, selection } => {
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
                let evidence = grants::add(grants_path, &matcher, now + u64::from(seconds), now)?;
                audit::log(
                    &self.paths,
                    audit_scope,
                    "override-timed",
                    audit_rule,
                    &audit_grant_duration(seconds),
                );
                Ok(PolicyDecision {
                    result: DecisionResult::Allow,
                    source: "human-grant".into(),
                    rule: Some(evidence.reference()),
                })
            }
            // A permanent decision is a policy edit made by a human gesture in the
            // window. The window returns only the boundary it confirmed; the rule is
            // rebuilt here from the normalized argv of that prefix, and the write is
            // simulated over that same argv before it reaches disk.
            AccessChoice::AllowAlways { prefix_len } | AccessChoice::DenyAlways { prefix_len } => {
                let allowing = matches!(choice, AccessChoice::AllowAlways { .. });
                let section = if allowing {
                    policy::Section::Accept
                } else {
                    policy::Section::Deny
                };
                let prefix = normalized_prefix(ignore, args, prefix_len);
                let written = match rule_target(scope, &prefix) {
                    Ok((path, rule)) => {
                        policy::add_rule(&path, section, &rule, &prefix, minimum).map(|()| rule)
                    }
                    Err(reason) => Err(Error::InvalidArguments(reason)),
                };
                match written {
                    Ok(rule) => {
                        audit::log(
                            &self.paths,
                            audit_scope,
                            if allowing {
                                "policy-accept-added"
                            } else {
                                "policy-deny-added"
                            },
                            audit_rule,
                            &audit_rule_shape(scope, &prefix),
                        );
                        Ok(PolicyDecision {
                            result: if allowing {
                                DecisionResult::Allow
                            } else {
                                DecisionResult::Deny
                            },
                            source: if allowing {
                                "human-permanent-allow".into()
                            } else {
                                "human-permanent-deny".into()
                            },
                            rule: Some(rule),
                        })
                    }
                    // The write failed after the human already held the button. The
                    // gesture still decides this call — allow means allow once, deny
                    // means deny — and the failure is loud in audit and on stderr,
                    // never swallowed into a policy the human believes was changed.
                    Err(error) => {
                        audit::log(
                            &self.paths,
                            audit_scope,
                            "policy-write-failed",
                            audit_rule,
                            &error.to_string(),
                        );
                        eprintln!(
                            "torii: a decisão permanente não foi gravada: {error}. Esta chamada seguiu apenas o gesto ({}).",
                            if allowing { "permitida uma vez" } else { "negada" }
                        );
                        Ok(PolicyDecision {
                            result: if allowing {
                                DecisionResult::Allow
                            } else {
                                DecisionResult::Deny
                            },
                            source: if allowing {
                                "human-once".into()
                            } else {
                                "human-deny".into()
                            },
                            rule: None,
                        })
                    }
                }
            }
        }
    }
}

/// Which policy file a permanent decision would be written to, and the literal
/// rule it would add — or why neither is possible here.
///
/// The rule always comes from the **normalized** argv, the one the evaluation
/// actually sees. Building it from what the window displays would write tokens
/// that normalization strips, and a rule that never matches is worse than no
/// rule: an accept that does not allow, or a deny that does not protect.
fn rule_target(
    scope: &InvocationScope,
    normalized: &[String],
) -> std::result::Result<(std::path::PathBuf, String), String> {
    Ok((policy_path(scope)?, policy::literal_rule(normalized)?))
}

/// Which policy file a permanent decision would be written to, or why none can be.
/// Independent of the argv: it is a property of the invocation's scope.
fn policy_path(scope: &InvocationScope) -> std::result::Result<std::path::PathBuf, String> {
    match &scope.target_rules {
        // A target with a policy of its own: the rule lands in the very file that
        // produced this unresolved verdict.
        Some(path) if path.exists() => Ok(path.clone()),
        // A target that uses the shared policy: writing there would reach every
        // other target of the provider, which is not what the human is looking at.
        // Creating the target policy from here is worse — it would drop every
        // shared accept in this alias at once.
        Some(_) => Err(format!(
            "este target usa a política compartilhada do provider; uma regra gravada aqui valeria para todos os targets. Crie a política do alias com `torii policy edit {} --create`",
            scope.audit_scope.replace('/', " ")
        )),
        None => Ok(scope.rules.clone()),
    }
}

/// The normalized argv of the first `prefix_len` displayed arguments.
///
/// Normalization is applied to the slice, not to the whole argv, because it is not
/// distributive: a bare flag at the boundary has no value token after it to drop.
/// Each boundary is normalized on its own so the rule matches what that boundary
/// would really be evaluated as.
fn normalized_prefix(ignore: &IgnoreArgs, args: &[String], prefix_len: usize) -> Vec<String> {
    ignore.normalize(&args[..prefix_len.min(args.len())])
}

/// Builds the `PermanentPolicy` the window renders: where a rule would go, and one
/// option per prefix boundary the human can pick in the scope editor.
fn permanent_options(
    ignore: &IgnoreArgs,
    scope: &InvocationScope,
    policy: &rules::Rules,
    args: &[String],
    minimum: usize,
) -> control::PermanentPolicy {
    let label = match &scope.target {
        Some(target) => format!("política do target {target:?}"),
        None => "política compartilhada do provider".to_string(),
    };
    // The scope block does not depend on the boundary: it is decided once.
    if let Err(reason) = policy_path(scope) {
        return control::PermanentPolicy::blocked(label, reason, args.len());
    }
    let options = (1..=args.len())
        .map(|prefix_len| {
            let prefix = normalized_prefix(ignore, args, prefix_len);
            match rule_target(scope, &prefix) {
                Err(reason) => control::PermanentOption {
                    rule: String::new(),
                    accept_blocked: Some(reason.clone()),
                    deny_blocked: Some(reason),
                },
                Ok((_, rule)) => {
                    // Each vector is simulated on its own: an accept below the
                    // provider's minimum width is ignored during evaluation, while a
                    // deny has no width gate, so the deny button can be live at a
                    // boundary where the accept cannot.
                    let blocked = |section| {
                        policy::simulate(policy, section, &rule, &prefix, minimum)
                            .err()
                            .map(|error| error.to_string())
                    };
                    control::PermanentOption {
                        accept_blocked: blocked(policy::Section::Accept),
                        deny_blocked: blocked(policy::Section::Deny),
                        rule,
                    }
                }
            }
        })
        .collect();
    control::PermanentPolicy {
        scope: label,
        options,
    }
}

/// Audit detail for a policy write: the shape of the rule, never the rule itself.
/// The call reference already carries the first two tokens and the invariant is
/// that the full argv is not logged.
fn audit_rule_shape(scope: &InvocationScope, normalized: &[String]) -> String {
    format!(
        "{} tokens, {}",
        normalized.len(),
        if scope.target_rules.is_some() {
            "target-policy"
        } else {
            "shared-policy"
        }
    )
}

/// Whole minutes keep the historical `2min` shape; an alignment with an older
/// grant adds the leftover seconds.
fn audit_grant_duration(seconds: u32) -> String {
    if seconds.is_multiple_of(60) {
        format!("{}min", seconds / 60)
    } else {
        format!("{}min{}s", seconds / 60, seconds % 60)
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
        // A non-target provider authenticates itself, into a bucket named after
        // its own tool.
        return Ok(InvocationScope {
            target: None,
            audit_scope: provider.config.name.clone(),
            rules: provider.paths.rules(),
            target_rules: None,
            grants: provider.paths.grants(),
            target_args: Vec::new(),
            target_env: None,
            trusted_env: Vec::new(),
            auth: AuthScope {
                provider: provider.config.tool.clone(),
                scope: provider.config.tool.clone(),
                expect: None,
                profile: None,
            },
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

    let identity = &target.config.identity;
    let mut target_args = Vec::new();
    match targeting.mode {
        // Nothing is appended: the credentials of this alias's scope are the whole
        // binding, and they are injected as environment into the child process.
        TargetMode::Credentials => {}
        TargetMode::KubectlContext => {
            target_args.push("--context".into());
            target_args.push(target.config.context.clone().expect("validated context"));
        }
        TargetMode::AwsProfile => {
            let profile = identity.profile.clone().expect("validated profile");
            target_args.extend(["--profile".into(), profile]);
            if let Some(region) = &target.config.region {
                target_args.extend(["--region".into(), region.clone()]);
            }
        }
    }

    Ok(InvocationScope {
        target: Some(target.config.name.clone()),
        audit_scope: format!("{}/{}", provider.config.name, target.config.name),
        rules: provider.paths.rules(),
        target_rules: Some(target.paths.rules()),
        grants: target.paths.grants(),
        target_args,
        target_env: Some(target.paths.env()),
        trusted_env: Vec::new(),
        auth: AuthScope {
            provider: identity.provider.clone(),
            scope: target.config.credential_scope().to_string(),
            expect: identity.expect.clone(),
            profile: identity.profile.clone(),
        },
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

fn target_access_denied_result(
    provider: &Provider,
    scope: &InvocationScope,
    source: &str,
) -> InvocationResult {
    invocation_result(
        provider,
        scope,
        PolicyDecision {
            result: DecisionResult::Deny,
            source: source.into(),
            rule: None,
        },
        None,
    )
}

fn merge_environment(base: &mut Vec<(String, String)>, overrides: Vec<(String, String)>) {
    for (key, value) in overrides {
        if let Some((_, existing)) = base
            .iter_mut()
            .find(|(candidate, _)| same_environment_key(candidate, &key))
        {
            *existing = value;
        } else {
            base.push((key, value));
        }
    }
}

fn same_environment_key(left: &str, right: &str) -> bool {
    #[cfg(windows)]
    {
        left.eq_ignore_ascii_case(right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

/// Drop in-memory environment entries the identity provider owns, so a stale
/// value from a `.env` file can't override injected credentials or profile.
fn strip_keys(environment: &mut Vec<(String, String)>, keys: &[&str]) {
    environment.retain(|(key, _)| {
        !keys
            .iter()
            .any(|blocked| same_environment_key(key, blocked))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn empty_policy() -> rules::Rules {
        serde_yaml::from_str("version: '1.0'\ndeny: []\naccept: []\n").unwrap()
    }

    fn scope_with(rules_path: &Path, target_rules: Option<&Path>) -> InvocationScope {
        InvocationScope {
            target: target_rules.map(|_| "prd".to_string()),
            audit_scope: "aws/prd".into(),
            rules: rules_path.to_path_buf(),
            target_rules: target_rules.map(Path::to_path_buf),
            grants: rules_path.with_file_name("grants.yaml"),
            target_args: Vec::new(),
            target_env: None,
            trusted_env: Vec::new(),
            auth: AuthScope {
                provider: "aws".into(),
                scope: "aws".into(),
                expect: None,
                profile: None,
            },
        }
    }

    /// A janela nunca oferece gravar numa política que não é a avaliada. Num target
    /// que usa a compartilhada, uma regra gravada ali valeria para todos os outros
    /// targets do provider — que não é o que o humano está olhando.
    #[test]
    fn a_target_without_its_own_policy_offers_no_permanent_decision() {
        let temp = tempfile::TempDir::new().unwrap();
        let shared = temp.path().join("rules.yaml");
        fs::write(
            &shared,
            "version: '1.0'
deny: []
accept: []
",
        )
        .unwrap();
        let missing = temp.path().join("targets/prd/rules.yaml");

        let permanent = permanent_options(
            &IgnoreArgs::default(),
            &scope_with(&shared, Some(&missing)),
            &empty_policy(),
            &args(&["s3", "ls"]),
            2,
        );
        for prefix_len in 1..=2 {
            let option = permanent.at(prefix_len).unwrap();
            assert!(!option.allows_accept());
            assert!(!option.allows_deny());
            assert!(
                option
                    .shared_block()
                    .is_some_and(|r| r.contains("--create")),
                "{option:?}"
            );
        }
    }

    /// Uma opção por fronteira: a regra é o prefixo escolhido no editor de escopo,
    /// não o argv inteiro. É o ajuste fino antes de tornar permanente.
    #[test]
    fn every_prefix_boundary_gets_its_own_rule() {
        let temp = tempfile::TempDir::new().unwrap();
        let shared = temp.path().join("rules.yaml");
        fs::write(
            &shared,
            "version: '1.0'
deny: []
accept: []
",
        )
        .unwrap();
        let own = temp.path().join("target-rules.yaml");
        fs::write(
            &own,
            "version: '1.0'
deny: []
accept: []
",
        )
        .unwrap();
        let scope = scope_with(&shared, Some(&own));

        let permanent = permanent_options(
            &IgnoreArgs::default(),
            &scope,
            &empty_policy(),
            &args(&["s3", "ls", "meu-bucket"]),
            2,
        );
        assert_eq!(permanent.options.len(), 3);
        // Um token só fica abaixo da largura mínima de accept, mas pode ser negado.
        assert!(!permanent.at(1).unwrap().allows_accept());
        assert!(permanent.at(1).unwrap().allows_deny());
        assert_eq!(permanent.at(2).unwrap().rule, "s3 ls");
        assert_eq!(permanent.at(3).unwrap().rule, "s3 ls meu-bucket");
        assert!(permanent.at(3).unwrap().allows_accept());
        assert!(permanent.scope.contains("prd"), "{permanent:?}");

        let (path, rule) = rule_target(&scope, &args(&["s3", "ls"])).unwrap();
        assert_eq!(path, own);
        assert_eq!(rule, "s3 ls");
    }

    /// A regra de cada fronteira vem do argv normalizado daquele prefixo. Construída
    /// do que a janela mostra, ela carregaria tokens que a normalização remove e
    /// nunca casaria; normalizar o argv inteiro e cortar depois também erraria, pois
    /// uma flag na fronteira não tem valor seguinte para descartar.
    #[test]
    fn each_boundary_is_normalized_on_its_own() {
        let ignore = IgnoreArgs {
            leading: 1,
            flags: args(&["--region"]),
        };
        let shown = args(&["aws", "s3", "ls", "--region", "sa-east-1"]);
        assert_eq!(normalized_prefix(&ignore, &shown, 5), args(&["s3", "ls"]));
        assert_eq!(normalized_prefix(&ignore, &shown, 3), args(&["s3", "ls"]));
        assert_eq!(normalized_prefix(&ignore, &shown, 2), args(&["s3"]));
        // A fronteira para na flag: não há valor seguinte neste prefixo.
        assert_eq!(normalized_prefix(&ignore, &shown, 4), args(&["s3", "ls"]));

        let temp = tempfile::TempDir::new().unwrap();
        let shared = temp.path().join("rules.yaml");
        fs::write(
            &shared,
            "version: '1.0'
deny: []
accept: []
",
        )
        .unwrap();
        let permanent = permanent_options(
            &ignore,
            &scope_with(&shared, None),
            &empty_policy(),
            &shown,
            2,
        );
        assert_eq!(permanent.at(5).unwrap().rule, "s3 ls");
        assert_eq!(permanent.at(3).unwrap().rule, "s3 ls");
        // O primeiro token é descartado pela normalização: não sobra regra.
        assert!(permanent.at(1).unwrap().shared_block().is_some());
    }

    #[test]
    fn kubectl_target_authenticates_through_its_identity_provider_and_scope() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());

        let aws = paths.provider("aws");
        aws.ensure().unwrap();
        fs::write(
            aws.config(),
            r#"
version: "1"
name: aws
tool: aws
description: identity provider
command: executable-not-used
auth: { strategy: inherited }
environment: { file: .env }
"#,
        )
        .unwrap();

        let target_provider = paths.provider("kubectl");
        target_provider.ensure().unwrap();
        fs::write(
            target_provider.config(),
            r#"
version: "1"
name: kubectl
tool: kubectl
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
            "version: '1'\nname: lab\ncontext: local\nidentity:\n  provider: aws\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("kubectl").unwrap();
        let scope = resolve_scope(&provider, Some("lab"), &["get".into()]).unwrap();

        assert_eq!(scope.target_args, ["--context", "local"]);
        assert_eq!(scope.auth.provider, "aws");
        // scope defaults to the target name, isolating the credential bucket.
        assert_eq!(scope.auth.scope, "lab");
        assert!(scope.auth.expect.is_none());
        assert!(scope.auth.profile.is_none());
    }

    /// No modo `credentials` o alias não acrescenta nada à linha de comando: a
    /// credencial do escopo é o binding inteiro. Se algo fosse injetado aqui, o
    /// CLI receberia uma opção que o humano nunca viu na janela.
    #[test]
    fn credentials_target_injects_nothing_and_authenticates_in_its_own_bucket() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider_paths = paths.provider("awsx");
        provider_paths.ensure().unwrap();
        fs::write(
            provider_paths.config(),
            r#"
version: "1"
name: awsx
tool: awsx
description: AWS por balde de credencial
command: aws
targeting:
  mode: credentials
  locked_options: ["--profile"]
auth:
  strategy: environment
  fields:
    - { name: AWS_ACCESS_KEY_ID, required: true, secret: false }
  inject:
    environment: { AWS_ACCESS_KEY_ID: "${AWS_ACCESS_KEY_ID}" }
  validate: { command: aws, args: [sts, get-caller-identity] }
  identity: { command: aws, args: [sts, get-caller-identity], field: Account }
environment: { file: .env }
"#,
        )
        .unwrap();
        fs::write(
            provider_paths.rules(),
            "version: '1.0'\ndeny: []\naccept: ['s3 ls']\n",
        )
        .unwrap();
        let target = provider_paths.target("mdb-prd");
        target.ensure().unwrap();
        fs::write(
            target.config(),
            "version: '1'\nname: mdb-prd\nidentity:\n  provider: awsx\n  expect: '111122223333'\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("awsx").unwrap();
        let scope = resolve_scope(&provider, Some("mdb-prd"), &["s3".into(), "ls".into()]).unwrap();

        assert!(scope.target_args.is_empty(), "{:?}", scope.target_args);
        assert!(scope.trusted_env.is_empty());
        // O alias autentica no próprio tool, num balde com o nome do alias.
        assert_eq!(scope.auth.provider, "awsx");
        assert_eq!(scope.auth.scope, "mdb-prd");
        assert_eq!(scope.auth.expect.as_deref(), Some("111122223333"));
        assert!(scope.auth.profile.is_none());
        // A política do alias é uma camada sobre a compartilhada, não a substitui.
        assert_eq!(scope.rules, provider_paths.rules());
        assert_eq!(scope.target_rules, Some(target.rules()));
    }

    #[test]
    fn aws_profile_target_authenticates_itself_with_profile_scope_and_expected_identity() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let provider_paths = paths.provider("aws-profile");
        provider_paths.ensure().unwrap();
        fs::write(
            provider_paths.config(),
            r#"
version: "1"
name: aws-profile
tool: aws_profile
description: AWS profile target test provider
command: aws
targeting: { mode: aws_profile }
auth:
  strategy: inherited
  validate: { command: aws, args: [sts, get-caller-identity] }
  identity: { command: aws, args: [sts, get-caller-identity], field: Account }
  profile_env: AWS_PROFILE
environment: { file: .env }
"#,
        )
        .unwrap();
        fs::write(
            provider_paths.rules(),
            "version: '1.0'\ndeny: []\naccept: ['ec2 describe-instances']\n",
        )
        .unwrap();
        let target = provider_paths.target("prod");
        target.ensure().unwrap();
        fs::write(
            target.config(),
            "version: '1'\nname: prod\nregion: sa-east-1\nidentity:\n  provider: aws_profile\n  scope: production-sso\n  profile: production-sso\n  expect: '123456789012'\n",
        )
        .unwrap();

        let registry = ProviderRegistry::load(&paths).unwrap();
        let provider = registry.get("aws_profile").unwrap();
        let scope = resolve_scope(
            &provider,
            Some("prod"),
            &["ec2".into(), "describe-instances".into()],
        )
        .unwrap();

        assert_eq!(
            scope.target_args,
            ["--profile", "production-sso", "--region", "sa-east-1"]
        );
        assert_eq!(scope.auth.provider, "aws_profile");
        assert_eq!(scope.auth.scope, "production-sso");
        assert_eq!(scope.auth.profile.as_deref(), Some("production-sso"));
        assert_eq!(scope.auth.expect.as_deref(), Some("123456789012"));
    }

    #[test]
    fn strip_keys_drops_owned_entries_before_profile_injection() {
        let removed = [
            "AWS_ACCESS_KEY_ID",
            "AWS_SESSION_TOKEN",
            "AWS_PROFILE",
            "AWS_DEFAULT_REGION",
        ];
        let mut environment = vec![
            ("AWS_ACCESS_KEY_ID".into(), "stale-key".into()),
            ("AWS_SESSION_TOKEN".into(), "stale-token".into()),
            ("AWS_PROFILE".into(), "stale-profile".into()),
            ("AWS_DEFAULT_REGION".into(), "us-east-1".into()),
            ("AWS_PAGER".into(), "".into()),
        ];
        strip_keys(&mut environment, &removed);
        // Unowned keys survive; every owned key is gone.
        assert!(environment.contains(&("AWS_PAGER".into(), "".into())));
        for key in removed {
            assert!(!environment
                .iter()
                .any(|(k, _)| same_environment_key(k, key)));
        }
        // Profile injected last wins deterministically.
        merge_environment(
            &mut environment,
            vec![("AWS_PROFILE".into(), "production-sso".into())],
        );
        assert!(environment.contains(&("AWS_PROFILE".into(), "production-sso".into())));
    }
}
