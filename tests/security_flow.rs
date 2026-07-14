use std::fs;
use tempfile::TempDir;
use torii::config::{ConfigPaths, Settings};
use torii::core::Invoker;
use torii::jasper::DecisionResult;
use torii::providers::ProviderRegistry;

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
  grant_rule: { mode: first_tokens, count: 1 }
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
    fs::write(provider.credentials(), "also invalid").unwrap();
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
    let target = provider.target("mpce_dev");
    target.ensure().unwrap();
    fs::write(
        target.config(),
        "version: '1'\nname: mpce_dev\ncontext: eks-mpce-dev\n",
    )
    .unwrap();
    let registry = ProviderRegistry::load(&paths).unwrap();
    (temp, paths, registry)
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
async fn explicit_deny_reports_the_selected_target() {
    let (_temp, paths, registry) = targeted_fixture();
    let result = Invoker::new(paths, Settings::default(), registry)
        .invoke("kubectl", Some("mpce_dev"), &["danger".into()])
        .await
        .unwrap();
    assert_eq!(result.target.as_deref(), Some("mpce_dev"));
    assert_eq!(result.decision.result, DecisionResult::Deny);
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
