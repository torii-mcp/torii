//! The audit key, bound to this machine and this user.
//!
//! The key is generated once per configuration root and kept in the operating
//! system's credential store: Credential Manager on Windows, which protects it
//! with DPAPI under the user's login; Keychain on macOS; Secret Service on
//! Linux. Someone who takes the disk gets the encrypted database, not the key.
//!
//! Without a credential store (a Linux server with no Secret Service) the key
//! falls back to a file readable only by the user, with a loud warning: in
//! that case whoever has the disk has the key too. `TORII_AUDIT_KEYSTORE=file`
//! asks for the file on purpose; development and CI set it so tests never
//! write into the developer's credential store.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::config::ConfigPaths;

pub(super) const KEY_LEN: usize = 32;

/// 256 random bits: the database key, given to SQLite3 Multiple Ciphers as is.
#[derive(Clone)]
pub(super) struct Key([u8; KEY_LEN]);

impl Key {
    fn generate() -> Self {
        let mut bytes = [0; KEY_LEN];
        getrandom::getrandom(&mut bytes).expect("the operating system provides random bytes");
        Self(bytes)
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        bytes.try_into().ok().map(Self)
    }

    fn bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }

    /// The key as SQLite3 Multiple Ciphers' `PRAGMA hexkey` takes it.
    pub(super) fn hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}

/// A fixed key, for tests that open a database directly.
#[cfg(test)]
pub(super) fn test_key(seed: u8) -> Key {
    Key([seed; KEY_LEN])
}

const SERVICE: &str = "torii-audit";

/// Keys already loaded by this process, per configuration root.
static LOADED: Mutex<Option<HashMap<PathBuf, Key>>> = Mutex::new(None);

pub(super) fn load(paths: &ConfigPaths) -> Result<Key, String> {
    let root = paths.base().to_path_buf();
    if let Some(key) = cached(&root) {
        return Ok(key);
    }
    paths.ensure().map_err(|error| error.to_string())?;
    // Two Torii processes starting together must not both create a key.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.audit_lock())
        .map_err(|error| format!("cannot open {}: {error}", paths.audit_lock().display()))?;
    lock.lock()
        .map_err(|error| format!("cannot lock {}: {error}", paths.audit_lock().display()))?;
    let key = if std::env::var_os("TORII_AUDIT_KEYSTORE").is_some_and(|value| value == "file") {
        from_file(paths)?
    } else {
        from_store(paths)?
    };
    LOADED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(root, key.clone());
    Ok(key)
}

fn cached(root: &PathBuf) -> Option<Key> {
    LOADED
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .as_ref()?
        .get(root)
        .cloned()
}

/// The credential store, falling back to the file where there is none.
fn from_store(paths: &ConfigPaths) -> Result<Key, String> {
    // One entry per configuration root: two roots never share a key.
    let account = paths.base().display().to_string();
    match store_get_or_create(&account) {
        Ok(key) => Ok(key),
        Err(reason) if cfg!(windows) => Err(format!("Credential Manager failed: {reason}")),
        Err(reason) => {
            let key = from_file(paths)?;
            eprintln!(
                "torii: no credential store is available ({reason}); the audit key is in {}, readable only by you. Anyone with this disk can read the audit log.",
                paths.audit_key_file().display()
            );
            Ok(key)
        }
    }
}

fn store_get_or_create(account: &str) -> Result<Key, String> {
    // The store may block on a desktop prompt; keep it off the async runtime.
    let account = account.to_owned();
    std::thread::spawn(move || {
        let entry = keyring::Entry::new(SERVICE, &account).map_err(|error| error.to_string())?;
        match entry.get_secret() {
            Ok(bytes) => Key::from_bytes(&bytes)
                .ok_or_else(|| "the stored audit key has the wrong length".to_string()),
            Err(keyring::Error::NoEntry) => {
                let key = Key::generate();
                entry
                    .set_secret(key.bytes())
                    .map_err(|error| error.to_string())?;
                Ok(key)
            }
            Err(error) => Err(error.to_string()),
        }
    })
    .join()
    .map_err(|_| "the credential store thread panicked".to_string())?
}

fn from_file(paths: &ConfigPaths) -> Result<Key, String> {
    let path = paths.audit_key_file();
    match std::fs::read(&path) {
        Ok(bytes) => Key::from_bytes(&bytes)
            .ok_or_else(|| format!("{} must hold exactly {KEY_LEN} bytes", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let key = Key::generate();
            write_private(&path, key.bytes())
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            Ok(key)
        }
        Err(error) => Err(format!("cannot read {}: {error}", path.display())),
    }
}

/// Create the key file readable and writable only by its owner.
fn write_private(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_key_is_created_once_and_read_back() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        let first = from_file(&paths).unwrap();
        let second = from_file(&paths).unwrap();
        assert_eq!(first.bytes(), second.bytes());
        assert_eq!(
            std::fs::read(paths.audit_key_file()).unwrap().len(),
            KEY_LEN
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_key_is_private_to_its_owner() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        from_file(&paths).unwrap();
        let mode = std::fs::metadata(paths.audit_key_file())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_damaged_key_file_is_refused_rather_than_replaced() {
        let temp = tempfile::TempDir::new().unwrap();
        let paths = ConfigPaths::new(temp.path().to_path_buf());
        std::fs::write(paths.audit_key_file(), b"short").unwrap();
        assert!(from_file(&paths).is_err());
        assert_eq!(std::fs::read(paths.audit_key_file()).unwrap(), b"short");
    }
}
