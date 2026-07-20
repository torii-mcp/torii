use std::fs;
use tempfile::TempDir;
use torii::config::{ConfigPaths, Settings};
use torii::core::Invoker;
use torii::jasper::DecisionResult;
use torii::providers::ProviderRegistry;
use torii::target_access::{self, ActivationMode};

static GUI_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn fixture(rules: &str) -> (TempDir, ConfigPaths, ProviderRegistry) {
    let temp = TempDir::new().unwrap();
    let paths = ConfigPaths::new(temp.path().to_path_buf());
    let provider = paths.provider("test");
    provider.ensure().unwrap();
    fs::write(
        provider.config(),
        r#"
version: "1"
name: test
tool: test
description: test provider
command: executable-that-must-not-run
policy:
  minimum_accept_tokens: 1
auth:
  strategy: environment
  fields:
    - { name: SECRET, required: true, secret: true }
  inject:
    environment: { SECRET: "${SECRET}" }
  validate:
    command: validator-that-must-not-run
    args: []
environment: { file: .env }
"#,
    )
    .unwrap();
    fs::write(provider.rules(), rules).unwrap();
    let registry = ProviderRegistry::load(&paths).unwrap();
    (temp, paths, registry)
}

#[tokio::test]
async fn explicit_deny_does_not_load_environment_or_authentication() {
    let (_temp, paths, registry) =
        fixture("version: '1.0'\ndeny: ['danger']\naccept: ['danger']\n");
    let provider = paths.provider("test");
    fs::write(provider.env(), "this is deliberately invalid").unwrap();
    // A non-target provider authenticates into a bucket named after its tool.
    let auth = provider.identity_scope("test");
    auth.ensure().unwrap();
    fs::write(auth.credentials(), "also invalid").unwrap();
    let result = Invoker::new(paths, Settings::default(), registry)
        .invoke("test", None, &["danger".into()])
        .await
        .unwrap();
    assert_eq!(result.decision.result, DecisionResult::Deny);
    assert!(result.execution.is_none());
}

#[tokio::test]
async fn headless_unresolved_is_default_deny_before_session_loading() {
    let (_temp, paths, registry) = fixture("version: '1.0'\ndeny: []\naccept: []\n");
    let _env_lock = GUI_ENV_LOCK.lock().await;
    std::env::set_var("TORII_NO_GUI", "1");
    let result = Invoker::new(paths, Settings::default(), registry)
        .invoke("test", None, &["unknown".into()])
        .await
        .unwrap();
    std::env::remove_var("TORII_NO_GUI");
    assert_eq!(result.decision.result, DecisionResult::Deny);
    assert!(result.execution.is_none());
}

fn targeted_fixture() -> (TempDir, ConfigPaths, ProviderRegistry) {
    let temp = TempDir::new().unwrap();
    let paths = ConfigPaths::new(temp.path().to_path_buf());
    let provider = paths.provider("kubectl");
    provider.ensure().unwrap();
    fs::write(
        provider.config(),
        r#"
version: "1"
name: kubectl
tool: kubectl
description: targeted test provider
command: executable-that-must-not-run
targeting: { mode: kubectl_context }
policy: { minimum_accept_tokens: 1 }
auth: { strategy: inherited }
environment: { file: .env }
"#,
    )
    .unwrap();
    fs::write(
        provider.rules(),
        "version: '1.0'\ndeny: ['danger']\naccept: ['get pods']\n",
    )
    .unwrap();
    add_inherited_auth_provider(&paths, Some(("rustc", &["--version"])));
    let target = provider.target("mpce_dev");
    target.ensure().unwrap();
    fs::write(
        target.config(),
        "version: '1'\nname: mpce_dev\ncontext: local-context\nidentity:\n  provider: auth\n",
    )
    .unwrap();
    let registry = ProviderRegistry::load(&paths).unwrap();
    authorize_target(&registry, "kubectl", "mpce_dev");
    (temp, paths, registry)
}

fn authorize_target(registry: &ProviderRegistry, tool: &str, target: &str) {
    let provider = registry.get(tool).unwrap();
    let known = target_access::known_targets(&provider).unwrap();
    target_access::activate(
        &provider.paths.target_authorizations(),
        &provider.paths.target_authorizations_lock(),
        &known,
        target,
        torii::audit::now_epoch(),
        30,
        ActivationMode::Replace,
    )
    .unwrap();
}

fn aws_profile_fixture(rules: &str) -> (TempDir, ConfigPaths, ProviderRegistry) {
    let temp = TempDir::new().unwrap();
    let paths = ConfigPaths::new(temp.path().to_path_buf());
    let provider = paths.provider("aws-profile");
    provider.ensure().unwrap();
    fs::write(
        provider.config(),
        r#"
version: "1"
name: aws-profile
tool: aws_profile
description: AWS profile target test provider
command: executable-that-must-not-run
targeting: { mode: aws_profile }
policy: { minimum_accept_tokens: 2 }
auth:
  strategy: inherited
  validate: { command: validator-that-must-not-run, args: [] }
  identity: { command: validator-that-must-not-run, args: [], field: Account }
  profile_env: AWS_PROFILE
environment: { file: .env }
"#,
    )
    .unwrap();
    fs::write(provider.rules(), rules).unwrap();
    let target = provider.target("prod");
    target.ensure().unwrap();
    fs::write(
        target.config(),
        "version: '1'\nname: prod\nidentity:\n  provider: aws_profile\n  scope: production-sso\n  profile: production-sso\n  expect: '123456789012'\n",
    )
    .unwrap();
    let registry = ProviderRegistry::load(&paths).unwrap();
    (temp, paths, registry)
}

fn set_target_provider(paths: &ConfigPaths, provider_tool: &str) {
    fs::write(
        paths.provider("kubectl").target("mpce_dev").config(),
        format!(
            "version: '1'\nname: mpce_dev\ncontext: local-context\nidentity:\n  provider: {provider_tool}\n"
        ),
    )
    .unwrap();
}

fn add_inherited_auth_provider(paths: &ConfigPaths, validate: Option<(&str, &[&str])>) {
    let provider = paths.provider("auth");
    provider.ensure().unwrap();
    let validate = validate.map_or_else(String::new, |(command, args)| {
        let args = args
            .iter()
            .map(|arg| format!("{arg:?}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("  validate: {{ command: {command:?}, args: [{args}] }}\n")
    });
    fs::write(
        provider.config(),
        format!(
            r#"
version: "1"
name: auth
tool: auth
description: authentication provider
command: executable-not-used
auth:
  strategy: inherited
{validate}environment: {{ file: .env }}
"#
        ),
    )
    .unwrap();
    fs::write(provider.rules(), "version: '1.0'\ndeny: []\naccept: []\n").unwrap();
}

#[tokio::test]
async fn targeted_provider_requires_a_known_target() {
    let (_temp, paths, registry) = targeted_fixture();
    let error = Invoker::new(paths, Settings::default(), registry)
        .invoke("kubectl", None, &["get".into(), "pods".into()])
        .await
        .unwrap_err();
    assert!(error.to_string().contains("target is required"));
}

#[tokio::test]
async fn target_locked_options_are_rejected_before_environment_or_spawn() {
    let (_temp, paths, registry) = targeted_fixture();
    fs::write(
        paths.provider("kubectl").target("mpce_dev").env(),
        "deliberately invalid",
    )
    .unwrap();
    let error = Invoker::new(paths, Settings::default(), registry)
        .invoke(
            "kubectl",
            Some("mpce_dev"),
            &["get".into(), "pods".into(), "--context=evil".into()],
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("is locked by target"));
}

#[tokio::test]
async fn aws_profile_override_is_rejected_before_environment_or_authentication() {
    let (_temp, paths, registry) =
        aws_profile_fixture("version: '1.0'\ndeny: []\naccept: ['ec2 describe-instances']\n");
    fs::write(
        paths.provider("aws-profile").target("prod").env(),
        "deliberately invalid",
    )
    .unwrap();

    let error = Invoker::new(paths, Settings::default(), registry)
        .invoke(
            "aws_profile",
            Some("prod"),
            &[
                "ec2".into(),
                "describe-instances".into(),
                "--profile=other".into(),
            ],
        )
        .await
        .unwrap_err();

    assert!(error.to_string().contains("is locked by target"));
}

#[tokio::test]
async fn denied_aws_profile_call_does_not_read_environment_or_check_identity() {
    let (_temp, paths, registry) =
        aws_profile_fixture("version: '1.0'\ndeny: ['danger action']\naccept: ['danger action']\n");
    fs::write(
        paths.provider("aws-profile").target("prod").env(),
        "deliberately invalid",
    )
    .unwrap();

    let result = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke(
            "aws_profile",
            Some("prod"),
            &["danger".into(), "action".into()],
        )
        .await
        .unwrap();

    assert_eq!(result.decision.result, DecisionResult::Deny);
    assert!(result.execution.is_none());
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(!audit.contains("aws-identity"));
    assert!(!audit.contains("session-ok"));
}

#[test]
fn aws_profile_target_requires_a_twelve_digit_expected_account() {
    let (_temp, paths, _registry) =
        aws_profile_fixture("version: '1.0'\ndeny: []\naccept: ['ec2 describe-instances']\n");
    fs::write(
        paths.provider("aws-profile").target("prod").config(),
        "version: '1'\nname: prod\nidentity:\n  provider: aws_profile\n  scope: production-sso\n  profile: production-sso\n  expect: not-an-account\n",
    )
    .unwrap();

    let error = ProviderRegistry::load(&paths).unwrap_err();
    assert!(error
        .to_string()
        .contains("expected_account_id must contain exactly 12 ASCII digits"));
}

#[tokio::test]
async fn explicit_deny_reports_the_selected_target() {
    let (_temp, paths, registry) = targeted_fixture();
    let provider = registry.get("kubectl").unwrap();
    target_access::clear(
        &provider.paths.target_authorizations(),
        &provider.paths.target_authorizations_lock(),
    )
    .unwrap();
    let result = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["danger".into()])
        .await
        .unwrap();
    assert_eq!(result.target.as_deref(), Some("mpce_dev"));
    assert_eq!(result.decision.result, DecisionResult::Deny);
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(!audit.contains("target-access-requested"));
}

#[tokio::test]
async fn inactive_target_is_denied_before_environment_authentication_or_preflight() {
    let (_temp, paths, registry) = targeted_fixture();
    let provider = registry.get("kubectl").unwrap();
    target_access::clear(
        &provider.paths.target_authorizations(),
        &provider.paths.target_authorizations_lock(),
    )
    .unwrap();
    fs::write(provider.paths.env(), "deliberately invalid").unwrap();
    fs::write(
        provider.target("mpce_dev").unwrap().paths.env(),
        "deliberately invalid",
    )
    .unwrap();
    fs::write(paths.provider("auth").env(), "deliberately invalid").unwrap();

    let _env_lock = GUI_ENV_LOCK.lock().await;
    std::env::set_var("TORII_NO_GUI", "1");
    let result = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["get".into(), "pods".into()])
        .await
        .unwrap();
    std::env::remove_var("TORII_NO_GUI");

    assert_eq!(result.decision.result, DecisionResult::Deny);
    assert_eq!(result.decision.source, "target-inactive");
    assert!(result.execution.is_none());
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(audit.contains("target-access-requested"));
    assert!(audit.contains("target-access-denied"));
    assert!(!audit.contains("preflight-provider"));
    assert!(!audit.contains("session-"));
}

#[tokio::test]
async fn target_rules_override_the_shared_policy() {
    let (_temp, paths, registry) = targeted_fixture();
    fs::write(
        paths.provider("kubectl").target("mpce_dev").rules(),
        "version: '1.0'\ndeny: ['get pods']\naccept: []\n",
    )
    .unwrap();
    let result = Invoker::new(paths, Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["get".into(), "pods".into()])
        .await
        .unwrap();
    assert_eq!(result.decision.result, DecisionResult::Deny);
    assert!(result.execution.is_none());
}

#[test]
fn target_requires_an_installed_lifecycle_provider() {
    let (_temp, paths, _registry) = targeted_fixture();
    set_target_provider(&paths, "missing");
    let error = ProviderRegistry::load(&paths).unwrap_err();
    assert!(error
        .to_string()
        .contains("install the provider before using this target"));
    // sanity: message is about the identity provider now
    assert!(error.to_string().contains("identity provider tool"));
}

#[test]
fn target_accepts_an_inherited_lifecycle_provider_without_validation() {
    let (_temp, paths, _registry) = targeted_fixture();
    add_inherited_auth_provider(&paths, None);
    set_target_provider(&paths, "auth");
    ProviderRegistry::load(&paths).unwrap();
}

#[test]
fn target_lifecycle_provider_cannot_require_another_target() {
    let (_temp, paths, _registry) = targeted_fixture();
    set_target_provider(&paths, "kubectl");
    let error = ProviderRegistry::load(&paths).unwrap_err();
    assert!(error
        .to_string()
        .contains("identity provider tool \"kubectl\" cannot require a target"));
}

#[tokio::test]
async fn target_runs_the_inherited_lifecycle_of_its_provider() {
    let (_temp, paths, _registry) = targeted_fixture();
    add_inherited_auth_provider(&paths, None);
    set_target_provider(&paths, "auth");
    let registry = ProviderRegistry::load(&paths).unwrap();

    let error = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["get".into(), "pods".into()])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("executable-that-must-not-run"));
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(audit.contains(" | auth | session-unchecked | "));
    assert!(audit.contains("preflight-ok"));
}

#[tokio::test]
async fn explicit_deny_does_not_read_preflight_provider_environment() {
    let (_temp, paths, _registry) = targeted_fixture();
    add_inherited_auth_provider(&paths, Some(("validator-that-must-not-run", &[])));
    fs::write(paths.provider("auth").env(), "deliberately invalid").unwrap();
    set_target_provider(&paths, "auth");
    let registry = ProviderRegistry::load(&paths).unwrap();

    let result = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["danger".into()])
        .await
        .unwrap();

    assert_eq!(result.decision.result, DecisionResult::Deny);
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(!audit.contains("preflight-provider"));
}

#[tokio::test]
async fn allowed_target_stops_when_preflight_provider_fails() {
    let (_temp, paths, _registry) = targeted_fixture();
    add_inherited_auth_provider(&paths, Some(("validator-that-must-not-run", &[])));
    fs::write(paths.provider("auth").env(), "deliberately invalid").unwrap();
    set_target_provider(&paths, "auth");
    let registry = ProviderRegistry::load(&paths).unwrap();

    let error = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["get".into(), "pods".into()])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("invalid env file"));
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(audit.contains("preflight-provider"));
    assert!(audit.contains("preflight-failed"));
    assert!(!audit.contains(" | ran | "));
}

#[tokio::test]
async fn successful_preflight_runs_before_the_target_provider() {
    let (_temp, paths, _registry) = targeted_fixture();
    add_inherited_auth_provider(&paths, Some(("rustc", &["--version"])));
    set_target_provider(&paths, "auth");
    let registry = ProviderRegistry::load(&paths).unwrap();

    let error = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["get".into(), "pods".into()])
        .await
        .unwrap_err();

    assert!(error.to_string().contains("executable-that-must-not-run"));
    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(audit.contains(" | auth | session-ok | "));
    assert!(audit.contains("preflight-ok"));
    assert!(!audit.contains(" | ran | "));
}

#[tokio::test]
async fn inherited_provider_without_validation_is_audited_as_unchecked() {
    let temp = TempDir::new().unwrap();
    let paths = ConfigPaths::new(temp.path().to_path_buf());
    let provider = paths.provider("unchecked");
    provider.ensure().unwrap();
    fs::write(
        provider.config(),
        r#"
version: "1"
name: unchecked
tool: unchecked
description: unchecked inherited provider
command: executable-that-must-not-run
policy: { minimum_accept_tokens: 1 }
auth: { strategy: inherited }
environment: { file: .env }
"#,
    )
    .unwrap();
    fs::write(
        provider.rules(),
        "version: '1.0'\ndeny: []\naccept: ['get']\n",
    )
    .unwrap();
    fs::write(provider.target_authorizations(), "deliberately invalid").unwrap();
    let registry = ProviderRegistry::load(&paths).unwrap();
    let _error = Invoker::new(paths.clone(), Settings::default(), registry)
        .invoke("unchecked", None, &["get".into()])
        .await
        .unwrap_err();

    let audit = fs::read_to_string(paths.log()).unwrap();
    assert!(audit.contains("session-unchecked"));
    assert!(!audit.contains("session-ok"));
}
