use super::rules;
use crate::error::{Error, Result};
use crate::providers::config::{GrantMode, GrantRule};
use std::io::Write;
use std::path::Path;

pub fn derive_rule(args: &[String], config: &GrantRule) -> String {
    match config.mode {
        GrantMode::FirstTokens => args
            .iter()
            .take(config.count.unwrap_or(2).max(1))
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" "),
        GrantMode::Exact => args.join(" "),
    }
}

pub fn parse(contents: &str) -> Vec<(u64, String)> {
    contents
        .lines()
        .filter_map(|raw| {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (expiry, rule) = line.split_once('\t')?;
            let expiry = expiry.trim().parse().ok()?;
            let rule = rule.trim();
            (!rule.is_empty()).then(|| (expiry, rule.to_string()))
        })
        .collect()
}

pub fn load_active(path: &Path, now: u64) -> Vec<(u64, String)> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| {
            parse(&s)
                .into_iter()
                .filter(|(expiry, _)| *expiry > now)
                .collect()
        })
        .unwrap_or_default()
}

pub fn matching_grant(entries: &[(u64, String)], args: &[String], now: u64) -> Option<String> {
    entries
        .iter()
        .find(|(expiry, rule)| *expiry > now && rules::matches(args, rule))
        .map(|(_, rule)| rule.clone())
}

pub fn add(path: &Path, rule: &str, expiry: u64, now: u64) -> Result<()> {
    let mut entries = load_active(path, now);
    entries.push((expiry, rule.to_string()));
    let parent = path.parent().ok_or_else(|| Error::Write {
        path: path.to_path_buf(),
        source: std::io::Error::other("missing parent directory"),
    })?;
    std::fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    for (expiry, rule) in entries {
        writeln!(temp, "{expiry}\t{rule}").map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    temp.flush().map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temp.persist(path).map_err(|error| Error::Write {
        path: path.to_path_buf(),
        source: error.error,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derives_first_tokens_or_exact() {
        let args = vec!["get".into(), "pods".into(), "-n".into(), "dev".into()];
        assert_eq!(
            derive_rule(
                &args,
                &GrantRule {
                    mode: GrantMode::FirstTokens,
                    count: Some(2)
                }
            ),
            "get pods"
        );
        assert_eq!(
            derive_rule(
                &args,
                &GrantRule {
                    mode: GrantMode::Exact,
                    count: None
                }
            ),
            "get pods -n dev"
        );
    }
}
