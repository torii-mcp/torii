use crate::error::{Error, Result};
use regex::{Regex, RegexBuilder};
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
    /// Compiles every rule up front so a malformed regex fails closed (an error,
    /// never a silent allow) before any policy decision is made.
    pub fn compile(&self) -> Result<CompiledRules> {
        Ok(CompiledRules {
            deny: self
                .deny
                .iter()
                .map(|rule| CompiledRule::parse(rule))
                .collect::<Result<_>>()?,
            accept: self
                .accept
                .iter()
                .map(|rule| CompiledRule::parse(rule))
                .collect::<Result<_>>()?,
        })
    }

    /// The ruleset that applies inside a target, composed from the shared
    /// provider policy and the target's own.
    ///
    /// The two vectors do not compose the same way, because they do not mean the
    /// same thing. `deny` accumulates: a shared deny is the provider-wide floor
    /// and creating a target policy must never be a way to step off it. `accept`
    /// is replaced: a target exists precisely to carry a different permission
    /// surface, so a shared allow must not leak into a target that declared its
    /// own. A target that wants a shared accept back relists it.
    pub fn layered(shared: &Rules, target: &Rules) -> Rules {
        let mut deny = shared.deny.clone();
        for rule in &target.deny {
            if !deny.contains(rule) {
                deny.push(rule.clone());
            }
        }
        Rules {
            version: target.version.clone(),
            deny,
            accept: target.accept.clone(),
        }
    }

    /// Accept rules that fall below the provider's minimum token width and are
    /// therefore ignored during evaluation. Regex rules carry their own
    /// specificity and are exempt from the token-width gate.
    pub fn invalid_accepts(&self, minimum_accept_tokens: usize) -> Vec<&str> {
        self.accept
            .iter()
            .filter(|rule| !is_regex_rule(rule) && tokens(rule).len() < minimum_accept_tokens)
            .map(String::as_str)
            .collect()
    }
}

/// A ruleset with every entry parsed into a matcher. Evaluation runs against the
/// normalized argv: `deny` wins, then `accept`, otherwise unresolved.
#[derive(Debug)]
pub struct CompiledRules {
    deny: Vec<CompiledRule>,
    accept: Vec<CompiledRule>,
}

impl CompiledRules {
    pub fn evaluate(&self, args: &[String], minimum_accept_tokens: usize) -> Evaluation {
        let joined = args.join(" ");
        for rule in &self.deny {
            if rule.is_match(args, &joined) {
                return Evaluation::DeniedExplicit {
                    rule: rule.raw.clone(),
                };
            }
        }
        for rule in &self.accept {
            if rule.token_len >= minimum_accept_tokens && rule.is_match(args, &joined) {
                return Evaluation::Allowed {
                    rule: rule.raw.clone(),
                };
            }
        }
        Evaluation::Unresolved
    }
}

#[derive(Debug)]
struct CompiledRule {
    raw: String,
    matcher: Matcher,
    /// Token width for the minimum-accept gate. Regex rules use `usize::MAX` so
    /// they are never filtered out by it.
    token_len: usize,
}

#[derive(Debug)]
enum Matcher {
    Literal(Vec<String>),
    Regex(Regex),
}

impl CompiledRule {
    fn parse(raw: &str) -> Result<Self> {
        if let Some((pattern, flags)) = regex_parts(raw) {
            let matcher = Matcher::Regex(build_regex(pattern, flags, raw)?);
            Ok(Self {
                raw: raw.to_string(),
                matcher,
                token_len: usize::MAX,
            })
        } else {
            let literal = tokens(raw);
            Ok(Self {
                raw: raw.to_string(),
                token_len: literal.len(),
                matcher: Matcher::Literal(literal.into_iter().map(str::to_string).collect()),
            })
        }
    }

    fn is_match(&self, args: &[String], joined: &str) -> bool {
        match &self.matcher {
            Matcher::Literal(rule) => literal_matches(args, rule),
            Matcher::Regex(regex) => regex.is_match(joined),
        }
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

/// The effective policy for an invocation scope. A target policy is layered over
/// the shared provider one; a target without a policy of its own uses the shared
/// policy whole. The shared policy is always required: it carries the deny floor,
/// so a missing file is an error, never an empty floor.
pub fn load_effective(shared: &Path, target: Option<&Path>) -> Result<Rules> {
    let base = load(shared)?;
    match target.filter(|path| path.exists()) {
        None => Ok(base),
        Some(path) => Ok(Rules::layered(&base, &load(path)?)),
    }
}

/// Literal token-prefix match: every token of the rule must equal the argv token
/// at the same position; extra argv tokens after the prefix are allowed.
fn literal_matches(args: &[String], rule: &[String]) -> bool {
    !rule.is_empty()
        && rule.len() <= args.len()
        && args.iter().zip(rule).all(|(arg, rule)| arg == rule)
}

fn tokens(value: &str) -> Vec<&str> {
    value.split_whitespace().collect()
}

/// A rule is a regex when it is delimited by `/…/flags`, e.g. `/\btruncate\b/i`.
/// The pattern is everything between the first and last slash; the trailing
/// segment holds ASCII flag letters.
fn regex_parts(raw: &str) -> Option<(&str, &str)> {
    let rest = raw.strip_prefix('/')?;
    let close = rest.rfind('/')?;
    let flags = &rest[close + 1..];
    if flags.chars().all(|c| c.is_ascii_alphabetic()) {
        Some((&rest[..close], flags))
    } else {
        None
    }
}

pub fn is_regex_rule(raw: &str) -> bool {
    regex_parts(raw).is_some()
}

fn build_regex(pattern: &str, flags: &str, raw: &str) -> Result<Regex> {
    let mut builder = RegexBuilder::new(pattern);
    for flag in flags.chars() {
        match flag {
            'i' => builder.case_insensitive(true),
            'm' => builder.multi_line(true),
            's' => builder.dot_matches_new_line(true),
            'x' => builder.ignore_whitespace(true),
            other => {
                return Err(Error::InvalidRule {
                    rule: raw.to_string(),
                    reason: format!("unsupported regex flag {other:?}"),
                })
            }
        };
    }
    builder.build().map_err(|source| Error::InvalidRule {
        rule: raw.to_string(),
        reason: source.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| (*v).into()).collect()
    }
    fn eval(deny: &[&str], accept: &[&str], args: &[&str], min: usize) -> Evaluation {
        Rules {
            version: "1.0".into(),
            deny: s(deny),
            accept: s(accept),
        }
        .compile()
        .unwrap()
        .evaluate(&s(args), min)
    }

    #[test]
    fn deny_has_priority() {
        assert!(matches!(
            eval(&["s3 rb"], &["s3 rb"], &["s3", "rb"], 2),
            Evaluation::DeniedExplicit { .. }
        ));
    }

    #[test]
    fn matching_respects_token_boundaries() {
        assert!(matches!(
            eval(&[], &["s3 ls"], &["s3api", "list-buckets"], 2),
            Evaluation::Unresolved
        ));
    }

    #[test]
    fn minimum_accept_tokens_is_provider_specific() {
        assert!(matches!(
            eval(&[], &["logs"], &["logs", "pod-x"], 1),
            Evaluation::Allowed { .. }
        ));
    }

    #[test]
    fn regex_deny_matches_anywhere_in_argv() {
        // A destructive keyword buried inside an inline query is denied.
        assert!(matches!(
            eval(
                &["/\\btruncate\\b/i"],
                &["sql -q"],
                &["sql", "-q", "SELECT 1; TRUNCATE t"],
                1,
            ),
            Evaluation::DeniedExplicit { .. }
        ));
    }

    #[test]
    fn regex_deny_wins_over_broad_literal_accept() {
        assert!(matches!(
            eval(
                &["/\\bdelete\\b/i"],
                &["sql -q"],
                &["sql", "-q", "delete from t"],
                1,
            ),
            Evaluation::DeniedExplicit { .. }
        ));
    }

    #[test]
    fn broad_literal_accept_allows_inline_read() {
        assert!(matches!(
            eval(
                &["/\\btruncate\\b/i"],
                &["sql -q"],
                &["sql", "-q", "select 1"],
                1
            ),
            Evaluation::Allowed { .. }
        ));
    }

    #[test]
    fn regex_accept_is_exempt_from_token_gate() {
        // A single-token regex accept still applies where a 2-token minimum runs.
        assert!(matches!(
            eval(&[], &["/^select/i"], &["select", "1"], 2),
            Evaluation::Allowed { .. }
        ));
        assert!(is_regex_rule("/^select/i"));
        assert!(!is_regex_rule("s3 ls"));
    }

    #[test]
    fn invalid_regex_fails_closed() {
        let rules = Rules {
            version: "1.0".into(),
            deny: s(&["/(unclosed/"]),
            accept: Vec::new(),
        };
        assert!(matches!(rules.compile(), Err(Error::InvalidRule { .. })));
    }

    #[test]
    fn unknown_flag_fails_closed() {
        let rules = Rules {
            version: "1.0".into(),
            deny: s(&["/truncate/z"]),
            accept: Vec::new(),
        };
        assert!(matches!(rules.compile(), Err(Error::InvalidRule { .. })));
    }

    fn rules(deny: &[&str], accept: &[&str]) -> Rules {
        Rules {
            version: "1.0".into(),
            deny: s(deny),
            accept: s(accept),
        }
    }

    #[test]
    fn a_target_policy_cannot_drop_a_shared_deny() {
        let shared = rules(&["iam"], &["s3 ls"]);
        let target = rules(&[], &["iam get-role"]);
        let effective = Rules::layered(&shared, &target);
        // The target relisted iam as an accept; the shared floor still wins.
        assert!(matches!(
            effective
                .compile()
                .unwrap()
                .evaluate(&s(&["iam", "get-role", "admin"]), 1),
            Evaluation::DeniedExplicit { rule } if rule == "iam"
        ));
    }

    #[test]
    fn a_target_policy_replaces_the_shared_accepts() {
        let shared = rules(&[], &["s3 ls", "ec2 describe-instances"]);
        let target = rules(&[], &["s3 ls"]);
        let effective = Rules::layered(&shared, &target);
        assert_eq!(effective.accept, s(&["s3 ls"]));
        // What the target did not relist is default-deny again, not inherited.
        assert!(matches!(
            effective
                .compile()
                .unwrap()
                .evaluate(&s(&["ec2", "describe-instances"]), 1),
            Evaluation::Unresolved
        ));
    }

    #[test]
    fn a_target_deny_narrows_only_its_own_target() {
        // "zero IAM in prd, some gets in dev": the deny lives in the target that
        // wants it, and the accept in the target that wants the exception.
        let shared = rules(&[], &["s3 ls"]);
        let prd = Rules::layered(&shared, &rules(&["iam"], &["s3 ls"]));
        let dev = Rules::layered(&shared, &rules(&[], &["s3 ls", "iam get-role"]));
        let args = s(&["iam", "get-role", "admin"]);
        assert!(matches!(
            prd.compile().unwrap().evaluate(&args, 1),
            Evaluation::DeniedExplicit { .. }
        ));
        assert!(matches!(
            dev.compile().unwrap().evaluate(&args, 1),
            Evaluation::Allowed { .. }
        ));
    }

    #[test]
    fn layering_does_not_duplicate_a_deny_the_target_repeats() {
        let effective = Rules::layered(&rules(&["s3 rb"], &[]), &rules(&["s3 rb", "iam"], &[]));
        assert_eq!(effective.deny, s(&["s3 rb", "iam"]));
    }

    #[test]
    fn invalid_accepts_skips_regex_rules() {
        let rules = Rules {
            version: "1.0".into(),
            deny: Vec::new(),
            accept: s(&["/^select/i", "s3"]),
        };
        // Only the short literal is flagged; the regex is exempt.
        assert_eq!(rules.invalid_accepts(2), vec!["s3"]);
    }
}
