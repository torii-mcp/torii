use crate::control::GrantSelection;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;

const GRANT_FILE_VERSION: &str = "2";
const DIGEST_HEX_LENGTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantMatcher {
    Exact(Vec<String>),
    Prefix(Vec<String>),
}

impl GrantMatcher {
    pub fn from_selection(args: &[String], selection: GrantSelection) -> Option<Self> {
        match selection {
            GrantSelection::Exact => Some(Self::Exact(args.to_vec())),
            GrantSelection::Prefix { token_count }
                if token_count > 0 && token_count <= args.len() =>
            {
                Some(Self::Prefix(args[..token_count].to_vec()))
            }
            GrantSelection::Prefix { .. } => None,
        }
    }

    fn stored(&self) -> StoredMatcher {
        match self {
            Self::Exact(args) => StoredMatcher::Exact {
                token_count: args.len(),
                sha256: fingerprint(StoredMode::Exact, args),
            },
            Self::Prefix(args) => StoredMatcher::Prefix {
                token_count: args.len(),
                sha256: fingerprint(StoredMode::Prefix, args),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantFileV2 {
    version: String,
    entries: Vec<StoredGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredGrant {
    expires_at: u64,
    matcher: StoredMatcher,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
enum StoredMatcher {
    Exact { token_count: usize, sha256: String },
    Prefix { token_count: usize, sha256: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoredMode {
    Exact,
    Prefix,
}

impl StoredMatcher {
    fn mode(&self) -> StoredMode {
        match self {
            Self::Exact { .. } => StoredMode::Exact,
            Self::Prefix { .. } => StoredMode::Prefix,
        }
    }

    fn token_count(&self) -> usize {
        match self {
            Self::Exact { token_count, .. } | Self::Prefix { token_count, .. } => *token_count,
        }
    }

    fn digest(&self) -> &str {
        match self {
            Self::Exact { sha256, .. } | Self::Prefix { sha256, .. } => sha256,
        }
    }

    fn valid(&self) -> bool {
        self.token_count() > 0
            && self.digest().len() == DIGEST_HEX_LENGTH
            && self.digest().bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    fn matches(&self, args: &[String]) -> bool {
        if !self.valid() {
            return false;
        }
        let token_count = self.token_count();
        match self.mode() {
            StoredMode::Exact if args.len() != token_count => false,
            StoredMode::Prefix if args.len() < token_count => false,
            _ => fingerprint(self.mode(), &args[..token_count]) == self.digest(),
        }
    }

    fn evidence(&self) -> GrantEvidence {
        GrantEvidence {
            mode: match self.mode() {
                StoredMode::Exact => "exact",
                StoredMode::Prefix => "prefix",
            },
            token_count: self.token_count(),
            digest_prefix: self.digest()[..8].to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ActiveGrant {
    expires_at: u64,
    matcher: StoredMatcher,
}

#[derive(Debug, Default)]
pub struct GrantLoad {
    pub active: Vec<ActiveGrant>,
    pub legacy_ignored: bool,
    pub invalid_ignored: bool,
}

#[derive(Debug, Clone)]
pub struct GrantEvidence {
    mode: &'static str,
    token_count: usize,
    digest_prefix: String,
}

impl GrantEvidence {
    pub fn reference(&self) -> String {
        format!(
            "grant:{}:{}:{}",
            self.mode, self.token_count, self.digest_prefix
        )
    }
}

pub fn load_active(path: &Path, now: u64) -> GrantLoad {
    let Ok(contents) = std::fs::read_to_string(path) else {
        return GrantLoad::default();
    };
    if contents.trim().is_empty() {
        return GrantLoad::default();
    }
    let Ok(file) = serde_yaml::from_str::<GrantFileV2>(&contents) else {
        return GrantLoad {
            legacy_ignored: contents.lines().any(|line| line.contains('\t')),
            invalid_ignored: !contents.lines().any(|line| line.contains('\t')),
            ..GrantLoad::default()
        };
    };
    if file.version != GRANT_FILE_VERSION || file.entries.iter().any(|entry| !entry.matcher.valid())
    {
        return GrantLoad {
            invalid_ignored: true,
            ..GrantLoad::default()
        };
    }
    GrantLoad {
        active: file
            .entries
            .into_iter()
            .filter(|entry| entry.expires_at > now)
            .map(|entry| ActiveGrant {
                expires_at: entry.expires_at,
                matcher: entry.matcher,
            })
            .collect(),
        ..GrantLoad::default()
    }
}

pub fn matching_grant(entries: &[ActiveGrant], args: &[String], now: u64) -> Option<GrantEvidence> {
    entries
        .iter()
        .find(|entry| entry.expires_at > now && entry.matcher.matches(args))
        .map(|entry| entry.matcher.evidence())
}

pub fn add(path: &Path, matcher: &GrantMatcher, expiry: u64, now: u64) -> Result<GrantEvidence> {
    let mut entries = load_active(path, now).active;
    let stored = matcher.stored();
    let evidence = stored.evidence();
    entries.push(ActiveGrant {
        expires_at: expiry,
        matcher: stored,
    });
    let parent = path.parent().ok_or_else(|| Error::Write {
        path: path.to_path_buf(),
        source: std::io::Error::other("missing parent directory"),
    })?;
    std::fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    let contents = serde_yaml::to_string(&GrantFileV2 {
        version: GRANT_FILE_VERSION.into(),
        entries: entries
            .into_iter()
            .map(|entry| StoredGrant {
                expires_at: entry.expires_at,
                matcher: entry.matcher,
            })
            .collect(),
    })
    .map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temp.write_all(contents.as_bytes())
        .map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
    temp.flush().map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })?;
    temp.persist(path).map_err(|error| Error::Write {
        path: path.to_path_buf(),
        source: error.error,
    })?;
    Ok(evidence)
}

fn fingerprint(mode: StoredMode, tokens: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"torii-grant-v2\0");
    hasher.update(match mode {
        StoredMode::Exact => b"exact".as_slice(),
        StoredMode::Prefix => b"prefix".as_slice(),
    });
    hasher.update((tokens.len() as u64).to_be_bytes());
    for token in tokens {
        hasher.update((token.len() as u64).to_be_bytes());
        hasher.update(token.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).into()).collect()
    }

    #[test]
    fn exact_requires_the_entire_vector() {
        let matcher = GrantMatcher::Exact(args(&["get", "pods"])).stored();
        assert!(matcher.matches(&args(&["get", "pods"])));
        assert!(!matcher.matches(&args(&["get", "pods", "--all-namespaces"])));
    }

    #[test]
    fn prefix_deliberately_allows_a_suffix() {
        let matcher = GrantMatcher::Prefix(args(&["get", "pods"])).stored();
        assert!(matcher.matches(&args(&["get", "pods"])));
        assert!(matcher.matches(&args(&["get", "pods", "-n", "financeiro"])));
        assert!(!matcher.matches(&args(&["get", "secrets"])));
    }

    #[test]
    fn fingerprint_preserves_token_boundaries() {
        let joined = GrantMatcher::Exact(args(&["get", "pods -A"])).stored();
        assert!(!joined.matches(&args(&["get", "pods", "-A"])));
        let empty = GrantMatcher::Exact(args(&["get", ""])).stored();
        assert!(empty.matches(&args(&["get", ""])));
        assert!(!empty.matches(&args(&["get"])));
    }

    #[test]
    fn selection_rejects_an_invalid_prefix_boundary() {
        let args = args(&["get", "pods"]);
        assert!(
            GrantMatcher::from_selection(&args, GrantSelection::Prefix { token_count: 0 })
                .is_none()
        );
        assert!(
            GrantMatcher::from_selection(&args, GrantSelection::Prefix { token_count: 3 })
                .is_none()
        );
    }

    #[test]
    fn legacy_grants_are_ignored_and_next_write_replaces_them() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("grants");
        std::fs::write(&path, "9999999999\tget pods\n").unwrap();

        let load = load_active(&path, 1);
        assert!(load.active.is_empty());
        assert!(load.legacy_ignored);

        add(&path, &GrantMatcher::Prefix(args(&["get", "pods"])), 100, 1).unwrap();
        let contents = std::fs::read_to_string(path).unwrap();
        assert!(contents.contains("version: '2'"));
        assert!(!contents.contains("\tget pods"));
    }

    #[test]
    fn invalid_grant_file_fails_closed() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("grants");
        std::fs::write(&path, "version: '2'\nentries: not-a-list\n").unwrap();
        let load = load_active(&path, 1);
        assert!(load.active.is_empty());
        assert!(load.invalid_ignored);
    }
}
