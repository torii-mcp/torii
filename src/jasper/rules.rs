use crate::error::{Error, Result};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct Rules {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub deny: Vec<String>,
    #[serde(default)]
    pub accept: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evaluation {
    Allowed { rule: String },
    DeniedExplicit { rule: String },
    Unresolved,
}

impl Rules {
    pub fn evaluate(&self, args: &[String], minimum_accept_tokens: usize) -> Evaluation {
        for rule in &self.deny {
            if matches(args, rule) {
                return Evaluation::DeniedExplicit { rule: rule.clone() };
            }
        }
        for rule in &self.accept {
            if tokens(rule).len() >= minimum_accept_tokens && matches(args, rule) {
                return Evaluation::Allowed { rule: rule.clone() };
            }
        }
        Evaluation::Unresolved
    }

    pub fn invalid_accepts(&self, minimum_accept_tokens: usize) -> Vec<&str> {
        self.accept
            .iter()
            .filter(|rule| tokens(rule).len() < minimum_accept_tokens)
            .map(String::as_str)
            .collect()
    }
}

pub fn load(path: &Path) -> Result<Rules> {
    if !path.exists() {
        return Err(Error::RulesNotFound(path.to_path_buf()));
    }
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

pub fn matches(args: &[String], rule: &str) -> bool {
    let rule = tokens(rule);
    !rule.is_empty()
        && rule.len() <= args.len()
        && args
            .iter()
            .map(String::as_str)
            .zip(rule)
            .all(|(arg, rule)| arg == rule)
}

fn tokens(value: &str) -> Vec<&str> {
    value.split_whitespace().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }
    fn rules(deny: &[&str], accept: &[&str]) -> Rules {
        Rules {
            version: "1.0".into(),
            deny: s(deny),
            accept: s(accept),
        }
    }

    #[test]
    fn deny_has_priority() {
        assert!(matches!(
            rules(&["s3 rb"], &["s3 rb"]).evaluate(&s(&["s3", "rb"]), 2),
            Evaluation::DeniedExplicit { .. }
        ));
    }
    #[test]
    fn matching_respects_token_boundaries() {
        assert!(matches!(
            rules(&[], &["s3 ls"]).evaluate(&s(&["s3api", "list-buckets"]), 2),
            Evaluation::Unresolved
        ));
    }
    #[test]
    fn minimum_accept_tokens_is_provider_specific() {
        assert!(matches!(
            rules(&[], &["logs"]).evaluate(&s(&["logs", "pod-x"]), 1),
            Evaluation::Allowed { .. }
        ));
    }
}
