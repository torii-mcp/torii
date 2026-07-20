use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use crate::error::{Error, Result};
use crate::providers::{Provider, TargetConfig, TargetMode};

const STATE_VERSION: &str = "1";
const LOCK_WAIT: Duration = Duration::from_secs(5);
const LOCK_RETRY: Duration = Duration::from_millis(10);

pub const MIN_TARGET_MINUTES: u32 = 1;
pub const MAX_TARGET_MINUTES: u32 = 24 * 60;

static REVISION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownTarget {
    pub target: String,
    pub binding: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetAuthorization {
    pub target: String,
    pub binding: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationSnapshot {
    pub revision: StateRevision,
    pub active: Vec<TargetAuthorization>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateRevision(Option<String>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationOutcome {
    Applied(Vec<TargetAuthorization>),
    StateChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationMode {
    Replace,
    Add,
}

#[derive(Debug, Clone, Copy)]
pub struct GuardedActivation<'a> {
    pub mode: ActivationMode,
    pub expected_revision: &'a StateRevision,
}

pub fn known_targets(provider: &Provider) -> Result<Vec<KnownTarget>> {
    provider
        .targets
        .values()
        .map(|target| {
            Ok(KnownTarget {
                target: target.config.name.clone(),
                binding: binding_fingerprint(&target.config)?,
            })
        })
        .collect()
}

pub fn human_binding(provider: &Provider, target_name: &str) -> Result<String> {
    let target = provider.target(target_name).ok_or_else(|| {
        Error::InvalidArguments(format!(
            "unknown target {target_name:?} for provider tool {:?}",
            provider.config.tool
        ))
    })?;
    let mode = provider
        .config
        .targeting
        .as_ref()
        .map(|targeting| targeting.mode)
        .ok_or_else(|| {
            Error::InvalidArguments(format!(
                "provider tool {:?} is not target-aware",
                provider.config.tool
            ))
        })?;
    let identity = &target.config.identity;
    let scope = target.config.credential_scope();
    Ok(match mode {
        TargetMode::KubectlContext => format!(
            "context={} · identity provider={} · scope={scope}",
            target.config.context.as_deref().expect("validated context"),
            identity.provider
        ),
        TargetMode::AwsProfile => {
            let region = target
                .config
                .region
                .as_deref()
                .unwrap_or("provider default");
            let account = identity.expect.as_deref().unwrap_or("unchecked");
            format!(
                "profile={} · account={account} · region={region} · scope={scope}",
                identity.profile.as_deref().expect("validated profile"),
            )
        }
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorizationFile {
    version: String,
    revision: String,
    #[serde(default)]
    authorizations: Vec<TargetAuthorization>,
}

pub fn load(path: &Path, known_targets: &[KnownTarget], now: u64) -> Result<AuthorizationSnapshot> {
    if !path.exists() {
        return Ok(AuthorizationSnapshot {
            revision: StateRevision(None),
            active: Vec::new(),
        });
    }
    let contents = std::fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let state: AuthorizationFile =
        serde_yaml::from_str(&contents).map_err(|source| Error::Yaml {
            path: path.to_path_buf(),
            source,
        })?;
    if state.version != STATE_VERSION {
        return Err(invalid_state(
            path,
            format!("unsupported version {:?}", state.version),
        ));
    }

    if state.revision.trim().is_empty() {
        return Err(invalid_state(path, "revision cannot be empty".into()));
    }
    let known = known_targets
        .iter()
        .map(|target| (target.target.as_str(), target.binding.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut active = Vec::new();
    for authorization in state.authorizations {
        if !seen.insert(authorization.target.clone()) {
            return Err(invalid_state(
                path,
                format!("duplicate target {:?}", authorization.target),
            ));
        }
        if authorization.expires_at > now
            && known
                .get(authorization.target.as_str())
                .is_some_and(|binding| *binding == authorization.binding)
        {
            active.push(authorization);
        }
    }
    active.sort_by(|left, right| left.target.cmp(&right.target));
    Ok(AuthorizationSnapshot {
        revision: StateRevision(Some(state.revision)),
        active,
    })
}

pub fn activate(
    path: &Path,
    lock_path: &Path,
    known_targets: &[KnownTarget],
    target: &str,
    now: u64,
    minutes: u32,
    mode: ActivationMode,
) -> Result<Vec<TargetAuthorization>> {
    match activate_inner(
        path,
        lock_path,
        known_targets,
        target,
        now,
        minutes,
        mode,
        None,
    )? {
        ActivationOutcome::Applied(active) => Ok(active),
        ActivationOutcome::StateChanged => unreachable!("unchecked activation cannot be stale"),
    }
}

pub fn activate_if_unchanged(
    path: &Path,
    lock_path: &Path,
    known_targets: &[KnownTarget],
    target: &str,
    now: u64,
    minutes: u32,
    guarded: GuardedActivation<'_>,
) -> Result<ActivationOutcome> {
    activate_inner(
        path,
        lock_path,
        known_targets,
        target,
        now,
        minutes,
        guarded.mode,
        Some(guarded.expected_revision),
    )
}

#[allow(clippy::too_many_arguments)]
fn activate_inner(
    path: &Path,
    lock_path: &Path,
    known_targets: &[KnownTarget],
    target: &str,
    now: u64,
    minutes: u32,
    mode: ActivationMode,
    expected_revision: Option<&StateRevision>,
) -> Result<ActivationOutcome> {
    validate_duration(minutes)?;
    let requested = known_targets
        .iter()
        .find(|known| known.target == target)
        .ok_or_else(|| Error::InvalidArguments(format!("unknown target {target:?}")))?;

    let _lock = StateLock::acquire(lock_path)?;
    let snapshot = load(path, known_targets, now)?;
    if expected_revision.is_some_and(|expected| expected != &snapshot.revision) {
        return Ok(ActivationOutcome::StateChanged);
    }
    let mut authorizations = match mode {
        ActivationMode::Replace => Vec::new(),
        ActivationMode::Add => snapshot.active,
    };
    let expires_at = now
        .checked_add(u64::from(minutes) * 60)
        .ok_or_else(|| Error::InvalidArguments("target authorization expiry overflow".into()))?;
    authorizations.retain(|authorization| authorization.target != target);
    authorizations.push(TargetAuthorization {
        target: target.into(),
        binding: requested.binding.clone(),
        expires_at,
    });
    authorizations.sort_by(|left, right| left.target.cmp(&right.target));
    write_state(path, &authorizations)?;
    Ok(ActivationOutcome::Applied(authorizations))
}

pub fn binding_fingerprint(config: &TargetConfig) -> Result<String> {
    let encoded = serde_json::to_vec(config).map_err(|error| {
        Error::InvalidArguments(format!("could not fingerprint target binding: {error}"))
    })?;
    let mut digest = Sha256::new();
    digest.update(b"torii-target-binding-v1\0");
    digest.update(encoded);
    Ok(format!("{:x}", digest.finalize()))
}

pub fn clear(path: &Path, lock_path: &Path) -> Result<()> {
    let _lock = StateLock::acquire(lock_path)?;
    write_state(path, &[])
}

pub fn revoke(
    path: &Path,
    lock_path: &Path,
    known_targets: &[KnownTarget],
    target: &str,
    now: u64,
) -> Result<()> {
    let _lock = StateLock::acquire(lock_path)?;
    let mut active = load(path, known_targets, now)?.active;
    active.retain(|authorization| authorization.target != target);
    // Write even when no matching lease exists. The new revision invalidates a
    // prompt that was opened before the target removal started.
    write_state(path, &active)
}

pub fn is_active(
    path: &Path,
    known_targets: &[KnownTarget],
    target: &str,
    now: u64,
) -> Result<bool> {
    Ok(load(path, known_targets, now)?
        .active
        .iter()
        .any(|authorization| authorization.target == target))
}

pub fn run_if_active<T>(
    path: &Path,
    lock_path: &Path,
    known_targets: &[KnownTarget],
    target: &str,
    action: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    let _lock = StateLock::acquire(lock_path)?;
    if !is_active(path, known_targets, target, epoch_seconds())? {
        return Ok(None);
    }
    action().map(Some)
}

pub fn validate_duration(minutes: u32) -> Result<()> {
    if !(MIN_TARGET_MINUTES..=MAX_TARGET_MINUTES).contains(&minutes) {
        return Err(Error::InvalidArguments(format!(
            "target authorization duration must be between {MIN_TARGET_MINUTES} and {MAX_TARGET_MINUTES} minutes"
        )));
    }
    Ok(())
}

fn write_state(path: &Path, authorizations: &[TargetAuthorization]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| {
        Error::InvalidArguments("target authorization path has no parent directory".into())
    })?;
    std::fs::create_dir_all(parent).map_err(|source| Error::Write {
        path: parent.to_path_buf(),
        source,
    })?;
    let state = AuthorizationFile {
        version: STATE_VERSION.into(),
        revision: fresh_revision(),
        authorizations: authorizations.to_vec(),
    };
    let yaml = serde_yaml::to_string(&state).map_err(|source| Error::Yaml {
        path: path.to_path_buf(),
        source,
    })?;
    let mut staged = tempfile::Builder::new()
        .prefix(".target-authorizations-")
        .tempfile_in(parent)
        .map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
    staged
        .write_all(yaml.as_bytes())
        .and_then(|_| staged.flush())
        .map_err(|source| Error::Write {
            path: path.to_path_buf(),
            source,
        })?;
    staged.persist(path).map_err(|error| Error::Write {
        path: path.to_path_buf(),
        source: error.error,
    })?;
    Ok(())
}

fn fresh_revision() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let counter = REVISION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{:x}-{counter:x}", std::process::id())
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn invalid_state(path: &Path, reason: String) -> Error {
    Error::InvalidTargetAuthorizations {
        path: path.to_path_buf(),
        reason,
    }
}

struct StateLock {
    _file: File,
}

impl StateLock {
    fn acquire(path: &Path) -> Result<Self> {
        let started = SystemTime::now();
        loop {
            match try_lock_file(path) {
                Ok(file) => return Ok(Self { _file: file }),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::PermissionDenied
                    ) =>
                {
                    if started.elapsed().unwrap_or(LOCK_WAIT) >= LOCK_WAIT {
                        return Err(Error::Write {
                            path: path.to_path_buf(),
                            source: std::io::Error::new(
                                std::io::ErrorKind::WouldBlock,
                                "timed out waiting for target authorization lock",
                            ),
                        });
                    }
                    std::thread::sleep(LOCK_RETRY);
                }
                Err(source) => {
                    return Err(Error::Write {
                        path: path.to_path_buf(),
                        source,
                    });
                }
            }
        }
    }
}

#[cfg(windows)]
fn try_lock_file(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(path)
}

#[cfg(unix)]
fn try_lock_file(path: &Path) -> std::io::Result<File> {
    use std::os::fd::AsRawFd;

    const LOCK_EXCLUSIVE: std::ffi::c_int = 2;
    const LOCK_NONBLOCKING: std::ffi::c_int = 4;
    extern "C" {
        fn flock(file_descriptor: std::ffi::c_int, operation: std::ffi::c_int) -> std::ffi::c_int;
    }

    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    // SAFETY: `file` owns a valid descriptor for the duration of this call and
    // `flock` neither retains the pointer nor accesses Rust-managed memory.
    let result = unsafe { flock(file.as_raw_fd(), LOCK_EXCLUSIVE | LOCK_NONBLOCKING) };
    if result == 0 {
        Ok(file)
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn paths(temp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        (
            temp.path().join("authorizations.yaml"),
            temp.path().join("authorizations.lock"),
        )
    }

    fn known() -> Vec<KnownTarget> {
        vec![
            KnownTarget {
                target: "dev".into(),
                binding: "dev-binding".into(),
            },
            KnownTarget {
                target: "prod".into(),
                binding: "prod-binding".into(),
            },
        ]
    }

    #[test]
    fn targets_start_inactive_and_replace_is_the_default_shape() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        assert!(load(&path, &known(), 100).unwrap().active.is_empty());

        activate(
            &path,
            &lock,
            &known(),
            "dev",
            100,
            30,
            ActivationMode::Replace,
        )
        .unwrap();
        let active = activate(
            &path,
            &lock,
            &known(),
            "prod",
            200,
            15,
            ActivationMode::Replace,
        )
        .unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].target, "prod");
        assert_eq!(active[0].binding, "prod-binding");
        assert_eq!(active[0].expires_at, 1_100);
    }

    #[test]
    fn add_keeps_other_active_targets_and_discards_expired_ones() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        activate(
            &path,
            &lock,
            &known(),
            "dev",
            100,
            1,
            ActivationMode::Replace,
        )
        .unwrap();
        let active =
            activate(&path, &lock, &known(), "prod", 200, 30, ActivationMode::Add).unwrap();
        assert_eq!(
            active
                .iter()
                .map(|item| item.target.as_str())
                .collect::<Vec<_>>(),
            ["prod"]
        );

        let active = activate(&path, &lock, &known(), "dev", 250, 30, ActivationMode::Add).unwrap();
        assert_eq!(
            active
                .iter()
                .map(|item| item.target.as_str())
                .collect::<Vec<_>>(),
            ["dev", "prod"]
        );
    }

    #[test]
    fn clear_recovers_even_when_the_previous_state_is_invalid() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        std::fs::write(&path, "not: [valid").unwrap();
        clear(&path, &lock).unwrap();
        assert!(load(&path, &known(), 100).unwrap().active.is_empty());
    }

    #[test]
    fn invalid_or_unknown_entries_never_authorize_a_target() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, _) = paths(&temp);
        std::fs::write(
            &path,
            "version: '1'\nrevision: a\nauthorizations:\n  - { target: removed, binding: old, expires_at: 500 }\n",
        )
        .unwrap();
        assert!(load(&path, &known(), 100).unwrap().active.is_empty());

        std::fs::write(
            &path,
            "version: '2'\nrevision: b\nauthorizations:\n  - { target: dev, binding: dev-binding, expires_at: 500 }\n",
        )
        .unwrap();
        assert!(matches!(
            load(&path, &known(), 100),
            Err(Error::InvalidTargetAuthorizations { .. })
        ));
    }

    #[test]
    fn changed_binding_invalidates_an_existing_authorization() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        activate(
            &path,
            &lock,
            &known(),
            "dev",
            100,
            30,
            ActivationMode::Replace,
        )
        .unwrap();
        let changed = vec![KnownTarget {
            target: "dev".into(),
            binding: "different-binding".into(),
        }];
        assert!(load(&path, &changed, 101).unwrap().active.is_empty());
    }

    #[test]
    fn stale_prompt_cannot_restore_authorization_after_clear() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        let snapshot = load(&path, &known(), 100).unwrap();
        clear(&path, &lock).unwrap();
        let outcome = activate_if_unchanged(
            &path,
            &lock,
            &known(),
            "prod",
            101,
            30,
            GuardedActivation {
                mode: ActivationMode::Replace,
                expected_revision: &snapshot.revision,
            },
        )
        .unwrap();
        assert_eq!(outcome, ActivationOutcome::StateChanged);
        assert!(load(&path, &known(), 101).unwrap().active.is_empty());
    }

    #[test]
    fn target_removal_revokes_even_an_identical_future_binding() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        activate(
            &path,
            &lock,
            &known(),
            "dev",
            100,
            30,
            ActivationMode::Replace,
        )
        .unwrap();
        revoke(&path, &lock, &known(), "dev", 101).unwrap();
        assert!(load(&path, &known(), 102).unwrap().active.is_empty());
    }

    #[test]
    fn duration_is_bounded() {
        assert!(validate_duration(0).is_err());
        assert!(validate_duration(1).is_ok());
        assert!(validate_duration(MAX_TARGET_MINUTES).is_ok());
        assert!(validate_duration(MAX_TARGET_MINUTES + 1).is_err());
    }

    #[test]
    fn operating_system_lock_is_exclusive_and_released_on_drop() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("authorizations.lock");
        let first = try_lock_file(&path).unwrap();
        assert!(try_lock_file(&path).is_err());
        drop(first);
        assert!(try_lock_file(&path).is_ok());
    }

    #[test]
    fn final_active_check_holds_the_lock_while_the_action_starts() {
        let temp = tempfile::TempDir::new().unwrap();
        let (path, lock) = paths(&temp);
        activate(
            &path,
            &lock,
            &known(),
            "dev",
            epoch_seconds(),
            30,
            ActivationMode::Replace,
        )
        .unwrap();
        let value = run_if_active(&path, &lock, &known(), "dev", || {
            assert!(try_lock_file(&lock).is_err());
            Ok(42)
        })
        .unwrap();
        assert_eq!(value, Some(42));
    }
}
