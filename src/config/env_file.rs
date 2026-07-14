use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::{Error, Result};

pub fn load(path: &Path) -> Result<Vec<(String, String)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&contents).map_err(|reason| Error::EnvParse {
        path: path.to_path_buf(),
        reason,
    })
}

pub fn parse(contents: &str) -> std::result::Result<Vec<(String, String)>, String> {
    let mut pairs = Vec::new();
    for (index, raw) in contents.lines().enumerate() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        for prefix in ["export ", "EXPORT ", "set ", "SET "] {
            if line.starts_with(prefix) {
                line = line[prefix.len()..].trim_start();
                break;
            }
        }
        if line
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("$env:"))
        {
            line = &line[5..];
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected KEY=VALUE", index + 1))?;
        let key = key.trim();
        if key.is_empty() {
            return Err(format!("line {}: empty key", index + 1));
        }
        if !key.bytes().all(|b| b == b'_' || b.is_ascii_alphanumeric()) {
            return Err(format!("line {}: invalid key {key:?}", index + 1));
        }
        pairs.push((key.to_string(), parse_value(value.trim())?));
    }
    Ok(pairs)
}

pub fn parse_allowed(
    contents: &str,
    allowed: &[String],
) -> std::result::Result<HashMap<String, String>, String> {
    let allowed: HashSet<&str> = allowed.iter().map(String::as_str).collect();
    Ok(parse(contents)?
        .into_iter()
        .filter(|(key, _)| allowed.contains(key.as_str()))
        .collect())
}

pub fn serialize(pairs: &[(String, String)]) -> String {
    let mut out = String::new();
    for (key, value) in pairs {
        out.push_str(key);
        out.push_str("=\"");
        out.push_str(&value.replace('\\', "\\\\").replace('"', "\\\""));
        out.push_str("\"\n");
    }
    out
}

fn parse_value(value: &str) -> std::result::Result<String, String> {
    let bytes = value.as_bytes();
    if bytes.len() < 2 || !matches!(bytes[0], b'\'' | b'"') || bytes[0] != bytes[bytes.len() - 1] {
        return Ok(value.to_string());
    }
    let inner = &value[1..value.len() - 1];
    if bytes[0] == b'\'' {
        return Ok(inner.to_string());
    }
    let mut output = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        match chars.next() {
            Some('\\') => output.push('\\'),
            Some('"') => output.push('"'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => return Err("quoted value ends with an incomplete escape".into()),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_assignment_formats() {
        let got = parse("export A=1\nSET B=\"two\"\n$Env:C='three'\nD=a=b\n").unwrap();
        assert_eq!(
            got,
            vec![
                ("A".into(), "1".into()),
                ("B".into(), "two".into()),
                ("C".into(), "three".into()),
                ("D".into(), "a=b".into())
            ]
        );
    }

    #[test]
    fn allowlist_drops_undeclared_fields() {
        let got = parse_allowed("A=1\nSECRET=2", &["A".into()]).unwrap();
        assert_eq!(got.get("A").map(String::as_str), Some("1"));
        assert!(!got.contains_key("SECRET"));
    }

    #[test]
    fn serialized_values_roundtrip_quotes_and_backslashes() {
        let values = vec![("A".into(), "a\\b\"c".into())];
        assert_eq!(parse(&serialize(&values)).unwrap(), values);
    }
}
