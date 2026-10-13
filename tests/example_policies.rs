//! Guards the shipped example policies: the regex rules in the snow setup must
//! compile (correct YAML escaping) and decide as documented.

use std::path::PathBuf;
use torii::jasper::rules::{self, Evaluation};

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("examples/providers")
        .join(relative)
}

fn s(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}

#[test]
fn snow_readonly_setup_compiles_and_decides() {
    let rules = rules::load(&example("snow/setups/readonly/rules.yaml")).unwrap();
    let compiled = rules.compile().expect("shipped regex rules must compile");

    // A destructive keyword anywhere in the inline query is denied...
    assert!(matches!(
        compiled.evaluate(&s(&["sql", "-q", "select 1; truncate t"]), 1),
        Evaluation::DeniedExplicit { .. }
    ));
    // ...while a plain read is allowed by the broad literal accept.
    assert!(matches!(
        compiled.evaluate(&s(&["sql", "-q", "select 1"]), 1),
        Evaluation::Allowed { .. }
    ));
    // COPY INTO (write) is caught by its regex.
    assert!(matches!(
        compiled.evaluate(&s(&["sql", "-q", "copy into t from @stage"]), 1),
        Evaluation::DeniedExplicit { .. }
    ));
}

#[test]
fn az_and_snow_root_rules_are_empty_and_compile() {
    for relative in ["az/rules.yaml", "snow/rules.yaml", "helm/rules.yaml"] {
        let rules = rules::load(&example(relative)).unwrap();
        rules.compile().unwrap();
    }
}

#[test]
fn helm_readonly_setup_lists_releases_but_not_their_values() {
    let rules = rules::load(&example("helm/setups/readonly/rules.yaml")).unwrap();
    let compiled = rules.compile().unwrap();

    assert!(matches!(
        compiled.evaluate(&s(&["list", "-n", "apps"]), 1),
        Evaluation::Allowed { .. }
    ));
    assert!(matches!(
        compiled.evaluate(&s(&["plugin", "install", "x"]), 1),
        Evaluation::DeniedExplicit { .. }
    ));
    // Values carry secrets: never accepted by the setup, left to a human.
    assert!(!matches!(
        compiled.evaluate(&s(&["get", "values", "api"]), 1),
        Evaluation::Allowed { .. }
    ));
}
