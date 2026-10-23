//! The audit log: every MCP call and every state-changing human command,
//! recorded so a human can follow, search and export what Torii did.
//!
//! - `record`: what one record holds;
//! - `transaction`: recording a call as it happens, and control actions;
//! - `ledger`: the database, SQLite3 Multiple Ciphers with the whole file
//!   encrypted;
//! - `query`: which records a search keeps;
//! - `key`: the machine-bound key, kept in the operating system's credential
//!   store.
//!
//! Credentials and clipboard contents never reach a record. The argv, the
//! agent's description and the output do, encrypted with a key that never
//! leaves this machine, because the human needs them to know what ran.

mod key;
mod ledger;
mod query;
pub mod record;
mod transaction;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub use ledger::{Entry, Ledger};
pub use query::Query;
pub use transaction::{record, Call, Closing, Execution, Trail, Transaction};

use crate::config::ConfigPaths;

/// Delete the records older than `days`. Returns how many went.
pub fn prune(paths: &ConfigPaths, days: u32) -> Result<usize, String> {
    let cutoff = now_ms() - i64::from(days) * 24 * 60 * 60 * 1000;
    Ledger::open(paths)?.prune(cutoff)
}

/// The detail of an event from its parts, leaving out the empty ones and the
/// `-` placeholder: `dev · 15min active=1`.
pub fn detail(subject: &str, extra: &str) -> String {
    [subject, extra]
        .into_iter()
        .filter(|part| !part.is_empty() && *part != "-")
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

static WARNED: AtomicBool = AtomicBool::new(false);

/// The audit log never stops a call, but its failure must not go unseen. Once
/// per process, on stderr: stdout belongs to the MCP transport.
fn warn_once(reason: &str) {
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("torii: the audit log is not recording: {reason}");
    }
}
