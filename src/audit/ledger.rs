//! The audit database: SQLite3 Multiple Ciphers, the whole file encrypted.
//!
//! The file is encrypted page by page with ChaCha20-Poly1305 under the
//! machine's audit key, so a copied disk reads nothing of it, not even dates.
//! Inside, records are ordinary rows with indexes: searching is SQL.
//!
//! Three tables: `calls` (a call or control action), `events` (its steps) and
//! `outputs` (what a call returned, deflated). SQLite in WAL mode lets the
//! several Torii servers of one machine write at the same time.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};

use super::key::{self, Key};
use super::query::Query;
use super::record::{CallRecord, EventRecord, Kind, Outcome, Output};
use crate::config::ConfigPaths;

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS calls (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        kind TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        finished_at INTEGER,
        tool TEXT NOT NULL,
        target TEXT,
        scope TEXT NOT NULL,
        args TEXT NOT NULL,
        description TEXT,
        outcome TEXT NOT NULL,
        source TEXT,
        rule TEXT,
        error TEXT,
        exit_code INTEGER,
        stdout_bytes INTEGER,
        stderr_bytes INTEGER,
        truncated INTEGER
    );
    CREATE INDEX IF NOT EXISTS calls_by_time ON calls(started_at);
    CREATE INDEX IF NOT EXISTS calls_by_scope ON calls(tool, target, started_at);
    CREATE INDEX IF NOT EXISTS calls_by_outcome ON calls(outcome, started_at);
    CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        call_id INTEGER NOT NULL REFERENCES calls(id) ON DELETE CASCADE,
        at INTEGER NOT NULL,
        scope TEXT NOT NULL,
        name TEXT NOT NULL,
        detail TEXT NOT NULL
    );
    CREATE INDEX IF NOT EXISTS events_by_call ON events(call_id);
    CREATE TABLE IF NOT EXISTS outputs (
        call_id INTEGER PRIMARY KEY REFERENCES calls(id) ON DELETE CASCADE,
        deflated BLOB NOT NULL
    );
";

const CALL_COLUMNS: &str = "id, kind, started_at, finished_at, tool, target, scope, args, \
     description, outcome, source, rule, error, exit_code, stdout_bytes, stderr_bytes, truncated";

/// A row of `calls`.
pub struct Entry {
    pub id: i64,
    pub record: CallRecord,
}

pub struct Ledger {
    connection: Connection,
}

impl Ledger {
    /// The audit database of this configuration root, with this machine's key.
    pub fn open(paths: &ConfigPaths) -> Result<Self, String> {
        let key = key::load(paths)?;
        Self::open_at(&paths.audit_db(), &key)
    }

    fn open_at(path: &Path, key: &Key) -> Result<Self, String> {
        match Self::connect(path, key) {
            Ok(ledger) => Ok(ledger),
            Err(Opening::WrongKey) => {
                // Encrypted with another key, or not encrypted at all: the key
                // was lost or this file came from elsewhere. It is kept aside,
                // never overwritten, and a fresh database starts.
                let aside = set_aside(path)?;
                eprintln!(
                    "torii: the audit log does not open with this machine's key; it was moved to {} and a new one started.",
                    aside.display()
                );
                Self::connect(path, key).map_err(Opening::into_message)
            }
            Err(failure) => Err(failure.into_message()),
        }
    }

    fn connect(path: &Path, key: &Key) -> Result<Self, Opening> {
        let failed = |error: rusqlite::Error| {
            Opening::Failed(format!("cannot open {}: {error}", path.display()))
        };
        let connection = Connection::open(path).map_err(failed)?;
        // The cipher is chosen before the key and the key before anything is
        // read. The key is 256 random bits already, so a single key
        // derivation round adds nothing to it.
        connection
            .execute_batch(&format!(
                "PRAGMA cipher = 'chacha20'; PRAGMA kdf_iter = 1; PRAGMA hexkey = '{}';",
                key.hex()
            ))
            .map_err(failed)?;
        // The first read is where a wrong key shows.
        if connection
            .query_row("SELECT count(*) FROM sqlite_master", [], |row| {
                row.get::<_, i64>(0)
            })
            .is_err()
        {
            return Err(Opening::WrongKey);
        }
        connection
            .busy_timeout(std::time::Duration::from_secs(10))
            .and_then(|()| {
                connection.execute_batch(
                    "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
                )
            })
            .and_then(|()| connection.execute_batch(SCHEMA))
            .map_err(failed)?;
        Ok(Self { connection })
    }

    pub fn insert_call(&self, record: &CallRecord) -> Result<i64, String> {
        self.connection
            .execute(
                "INSERT INTO calls (kind, started_at, finished_at, tool, target, scope, args,
                     description, outcome, source, rule, error, exit_code, stdout_bytes,
                     stderr_bytes, truncated)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    record.kind.name(),
                    record.started_at,
                    record.finished_at,
                    record.tool,
                    record.target,
                    record.scope,
                    args_json(&record.args),
                    record.description,
                    record.outcome.name(),
                    record.source,
                    record.rule,
                    record.error,
                    record.exit_code,
                    record.stdout_bytes.map(to_i64),
                    record.stderr_bytes.map(to_i64),
                    record.truncated,
                ],
            )
            .map_err(sql)?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn update_call(&self, id: i64, record: &CallRecord) -> Result<(), String> {
        self.connection
            .execute(
                "UPDATE calls SET finished_at = ?2, outcome = ?3, source = ?4, rule = ?5,
                     error = ?6, exit_code = ?7, stdout_bytes = ?8, stderr_bytes = ?9,
                     truncated = ?10
                 WHERE id = ?1",
                params![
                    id,
                    record.finished_at,
                    record.outcome.name(),
                    record.source,
                    record.rule,
                    record.error,
                    record.exit_code,
                    record.stdout_bytes.map(to_i64),
                    record.stderr_bytes.map(to_i64),
                    record.truncated,
                ],
            )
            .map(|_| ())
            .map_err(sql)
    }

    pub fn add_event(&self, call_id: i64, event: &EventRecord) -> Result<(), String> {
        self.connection
            .execute(
                "INSERT INTO events (call_id, at, scope, name, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![call_id, event.at, event.scope, event.name, event.detail],
            )
            .map(|_| ())
            .map_err(sql)
    }

    pub fn put_output(&self, call_id: i64, output: &Output) -> Result<(), String> {
        let json = serde_json::to_vec(output).map_err(|error| error.to_string())?;
        let mut encoder =
            flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
        encoder
            .write_all(&json)
            .map_err(|error| error.to_string())?;
        let deflated = encoder.finish().map_err(|error| error.to_string())?;
        self.connection
            .execute(
                "INSERT OR REPLACE INTO outputs (call_id, deflated) VALUES (?1, ?2)",
                params![call_id, deflated],
            )
            .map(|_| ())
            .map_err(sql)
    }

    pub fn call(&self, id: i64) -> Result<Option<Entry>, String> {
        self.connection
            .query_row(
                &format!("SELECT {CALL_COLUMNS} FROM calls WHERE id = ?1"),
                params![id],
                entry,
            )
            .optional()
            .map_err(sql)
    }

    /// The newest `limit` records, newest first.
    pub fn newest(&self, limit: usize) -> Result<Vec<Entry>, String> {
        self.search(&Query {
            limit: Some(limit),
            ..Query::default()
        })
    }

    /// Every record, newest first.
    pub fn all(&self) -> Result<Vec<Entry>, String> {
        self.search(&Query::default())
    }

    /// Records after `id`, oldest first: what `tail --follow` has not shown.
    pub fn after(&self, id: i64) -> Result<Vec<Entry>, String> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT {CALL_COLUMNS} FROM calls WHERE id > ?1 ORDER BY id ASC"
            ))
            .map_err(sql)?;
        let rows = statement.query_map(params![id], entry).map_err(sql)?;
        rows.map(|row| row.map_err(sql)).collect()
    }

    /// The lowest and highest record numbers kept, if there is any record.
    pub fn id_range(&self) -> Result<Option<(i64, i64)>, String> {
        self.connection
            .query_row("SELECT MIN(id), MAX(id) FROM calls", [], |row| {
                Ok(row
                    .get::<_, Option<i64>>(0)?
                    .zip(row.get::<_, Option<i64>>(1)?))
            })
            .map_err(sql)
    }

    /// Record numbers whose decimal digits end with `digits`, newest first,
    /// at most `limit` of them: what `torii log show 312` may mean.
    pub fn ending_with(&self, digits: &str, limit: usize) -> Result<Vec<i64>, String> {
        debug_assert!(digits.bytes().all(|b| b.is_ascii_digit()));
        let mut statement = self
            .connection
            .prepare(
                "SELECT id FROM calls WHERE CAST(id AS TEXT) LIKE ?1 ORDER BY id DESC LIMIT ?2",
            )
            .map_err(sql)?;
        let rows = statement
            .query_map(params![format!("%{digits}"), limit as i64], |row| {
                row.get(0)
            })
            .map_err(sql)?;
        rows.map(|row| row.map_err(sql)).collect()
    }

    /// The newest records the query keeps, newest first.
    pub fn search(&self, query: &Query) -> Result<Vec<Entry>, String> {
        let (clause, values) = query.sql();
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT {CALL_COLUMNS} FROM calls {clause} ORDER BY id DESC"
            ))
            .map_err(sql)?;
        let rows = statement
            .query_map(params_from_iter(values), entry)
            .map_err(sql)?;
        let mut found = Vec::new();
        for row in rows {
            let entry = row.map_err(sql)?;
            if query.matches_text(&entry.record) {
                found.push(entry);
                if query.limit.is_some_and(|limit| found.len() >= limit) {
                    break;
                }
            }
        }
        Ok(found)
    }

    pub fn events(&self, call_id: i64) -> Result<Vec<EventRecord>, String> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT at, scope, name, detail FROM events WHERE call_id = ?1 ORDER BY id ASC",
            )
            .map_err(sql)?;
        let rows = statement
            .query_map(params![call_id], |row| {
                Ok(EventRecord {
                    at: row.get(0)?,
                    scope: row.get(1)?,
                    name: row.get(2)?,
                    detail: row.get(3)?,
                })
            })
            .map_err(sql)?;
        rows.map(|row| row.map_err(sql)).collect()
    }

    /// `Ok(None)` when the call has no stored output.
    pub fn output(&self, call_id: i64) -> Result<Option<Output>, String> {
        let deflated: Option<Vec<u8>> = self
            .connection
            .query_row(
                "SELECT deflated FROM outputs WHERE call_id = ?1",
                params![call_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(sql)?;
        let Some(deflated) = deflated else {
            return Ok(None);
        };
        let mut json = Vec::new();
        flate2::read::DeflateDecoder::new(deflated.as_slice())
            .read_to_end(&mut json)
            .map_err(|error| error.to_string())?;
        serde_json::from_slice(&json)
            .map(Some)
            .map_err(|error| error.to_string())
    }

    /// Delete the records that started before `cutoff_ms`, with their events
    /// and outputs. Returns how many records went.
    pub fn prune(&self, cutoff_ms: i64) -> Result<usize, String> {
        self.connection
            .execute(
                "DELETE FROM calls WHERE started_at < ?1",
                params![cutoff_ms],
            )
            .map_err(sql)
    }
}

/// Why a database did not open.
enum Opening {
    /// It is encrypted with another key, or not encrypted.
    WrongKey,
    Failed(String),
}

impl Opening {
    fn into_message(self) -> String {
        match self {
            Opening::WrongKey => "the audit log does not open with this machine's key".into(),
            Opening::Failed(message) => message,
        }
    }
}

fn entry(row: &Row<'_>) -> rusqlite::Result<Entry> {
    let text_error = |index: usize, value: String| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            format!("unknown value {value:?}").into(),
        )
    };
    let kind: String = row.get(1)?;
    let outcome: String = row.get(9)?;
    let args: String = row.get(7)?;
    Ok(Entry {
        id: row.get(0)?,
        record: CallRecord {
            kind: Kind::parse(&kind).ok_or_else(|| text_error(1, kind.clone()))?,
            started_at: row.get(2)?,
            finished_at: row.get(3)?,
            tool: row.get(4)?,
            target: row.get(5)?,
            scope: row.get(6)?,
            args: serde_json::from_str(&args).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    error.into(),
                )
            })?,
            description: row.get(8)?,
            outcome: Outcome::parse(&outcome).ok_or_else(|| text_error(9, outcome.clone()))?,
            source: row.get(10)?,
            rule: row.get(11)?,
            error: row.get(12)?,
            exit_code: row.get(13)?,
            stdout_bytes: row.get::<_, Option<i64>>(14)?.map(to_u64),
            stderr_bytes: row.get::<_, Option<i64>>(15)?.map(to_u64),
            truncated: row.get(16)?,
        },
    })
}

/// The argv keeps its exact tokens as a JSON array.
fn args_json(args: &[String]) -> String {
    serde_json::to_string(args).expect("a list of strings always serializes")
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn to_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}

fn sql(error: rusqlite::Error) -> String {
    error.to_string()
}

/// Move a database that does not open beside itself, with its WAL files.
fn set_aside(path: &Path) -> Result<PathBuf, String> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S%.3f");
    let aside = path.with_file_name(format!("audit-{stamp}.unreadable.db"));
    std::fs::rename(path, &aside)
        .map_err(|error| format!("cannot move {} aside: {error}", path.display()))?;
    for suffix in ["-wal", "-shm"] {
        let mut companion = path.as_os_str().to_owned();
        companion.push(suffix);
        let mut moved = aside.as_os_str().to_owned();
        moved.push(suffix);
        let _ = std::fs::rename(companion, moved);
    }
    Ok(aside)
}

#[cfg(test)]
pub(super) fn open_for_test(path: &Path, key: &Key) -> Ledger {
    Ledger::open_at(path, key).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::key::test_key;

    fn record(started_at: i64, args: &[&str]) -> CallRecord {
        CallRecord {
            kind: Kind::Call,
            started_at,
            finished_at: None,
            tool: "aws".into(),
            target: Some("prd".into()),
            scope: "aws/prd".into(),
            args: args.iter().map(|arg| arg.to_string()).collect(),
            description: Some("conferir logs de produção".into()),
            outcome: Outcome::Running,
            source: None,
            rule: None,
            error: None,
            exit_code: None,
            stdout_bytes: None,
            stderr_bytes: None,
            truncated: None,
        }
    }

    fn event(name: &str) -> EventRecord {
        EventRecord {
            at: 2,
            scope: "aws".into(),
            name: name.into(),
            detail: format!("{name} detail"),
        }
    }

    fn ledger(temp: &tempfile::TempDir) -> Ledger {
        open_for_test(&temp.path().join("audit.db"), &test_key(1))
    }

    fn file_text(temp: &tempfile::TempDir) -> String {
        let mut bytes = Vec::new();
        for name in ["audit.db", "audit.db-wal"] {
            bytes.extend(std::fs::read(temp.path().join(name)).unwrap_or_default());
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[test]
    fn nothing_in_the_file_reads_without_the_key() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let id = ledger
            .insert_call(&record(
                1,
                &[
                    "secretsmanager",
                    "get-secret-value",
                    "--secret-id",
                    "prd/db",
                ],
            ))
            .unwrap();
        ledger.add_event(id, &event("allowed-by-rules")).unwrap();
        ledger
            .put_output(
                id,
                &Output {
                    stdout: "SuperSecretPassword".into(),
                    stderr: String::new(),
                },
            )
            .unwrap();
        // Read while the connection is open: the WAL holds the latest pages.
        let text = file_text(&temp);
        drop(ledger);
        let text = text + &file_text(&temp);
        for readable in [
            "get-secret-value",
            "prd/db",
            "aws/prd",
            "allowed-by-rules",
            "produção",
            "SuperSecret",
            "CREATE TABLE",
            "calls",
            "SQLite format",
        ] {
            assert!(
                !text.contains(readable),
                "{readable:?} is readable in the file"
            );
        }
    }

    #[test]
    fn a_call_round_trips_every_field() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let mut call = record(10, &["s3", "ls", "s3://bucket with space", "", "ção"]);
        let id = ledger.insert_call(&call).unwrap();
        call.finished_at = Some(25);
        call.outcome = Outcome::Allowed;
        call.source = Some("grant".into());
        call.rule = Some("prefix s3 ls".into());
        call.exit_code = Some(-1);
        call.stdout_bytes = Some(u64::from(u32::MAX) + 7);
        call.stderr_bytes = Some(0);
        call.truncated = Some(true);
        ledger.update_call(id, &call).unwrap();
        let read = ledger.call(id).unwrap().unwrap().record;
        assert_eq!(format!("{read:?}"), format!("{call:?}"));
    }

    #[test]
    fn the_output_comes_back_byte_for_byte() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let id = ledger.insert_call(&record(1, &["get"])).unwrap();
        let output = Output {
            stdout: "linha ção 🦀\n".repeat(20_000),
            stderr: "aviso\r\n\0fim".into(),
        };
        ledger.put_output(id, &output).unwrap();
        let read = ledger.output(id).unwrap().unwrap();
        assert_eq!(read.stdout, output.stdout);
        assert_eq!(read.stderr, output.stderr);
        assert!(ledger.output(id + 1).unwrap().is_none());
    }

    #[test]
    fn records_survive_closing_and_reopening() {
        let temp = tempfile::TempDir::new().unwrap();
        let id = {
            let ledger = ledger(&temp);
            let id = ledger.insert_call(&record(5, &["get", "pods"])).unwrap();
            ledger.add_event(id, &event("ran")).unwrap();
            id
        };
        let ledger = ledger(&temp);
        assert_eq!(
            ledger.call(id).unwrap().unwrap().record.args,
            ["get", "pods"]
        );
        assert_eq!(ledger.events(id).unwrap()[0].name, "ran");
    }

    #[test]
    fn listing_is_newest_first_and_follow_is_oldest_first() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let ids = (0..5)
            .map(|index| ledger.insert_call(&record(index, &["get"])).unwrap())
            .collect::<Vec<_>>();
        let ids_of = |entries: Vec<Entry>| entries.iter().map(|entry| entry.id).collect::<Vec<_>>();
        assert_eq!(ids_of(ledger.newest(2).unwrap()), [ids[4], ids[3]]);
        assert_eq!(ids_of(ledger.after(ids[2]).unwrap()), [ids[3], ids[4]]);
        assert_eq!(ids_of(ledger.all().unwrap()).len(), 5);
    }

    #[test]
    fn events_keep_their_order_and_their_call() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let first = ledger.insert_call(&record(1, &["get"])).unwrap();
        let second = ledger.insert_call(&record(2, &["get"])).unwrap();
        for name in ["session-ok", "allowed-by-rules", "ran"] {
            ledger.add_event(first, &event(name)).unwrap();
            ledger
                .add_event(second, &event(&format!("other-{name}")))
                .unwrap();
        }
        let names = ledger
            .events(first)
            .unwrap()
            .into_iter()
            .map(|event| event.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["session-ok", "allowed-by-rules", "ran"]);
    }

    #[test]
    fn search_filters_by_index_and_by_text() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let mut kubectl = record(100, &["delete", "pod", "API-7f9c"]);
        kubectl.tool = "kubectl".into();
        kubectl.target = Some("dev".into());
        kubectl.scope = "kubectl/dev".into();
        kubectl.outcome = Outcome::Denied;
        kubectl.description = Some("Remover o pod em PRODUÇÃO".into());
        ledger.insert_call(&kubectl).unwrap();
        let mut aws = record(200, &["s3", "ls"]);
        aws.outcome = Outcome::Allowed;
        ledger.insert_call(&aws).unwrap();
        let mut human = record(300, &[]);
        human.kind = Kind::Control;
        human.outcome = Outcome::Done;
        ledger.insert_call(&human).unwrap();

        let count = |query: Query| ledger.search(&query).unwrap().len();
        assert_eq!(count(Query::default()), 3);
        assert_eq!(
            count(Query {
                tool: Some("kubectl".into()),
                ..Query::default()
            }),
            1
        );
        assert_eq!(
            count(Query {
                target: Some("prd".into()),
                ..Query::default()
            }),
            2
        );
        assert_eq!(
            count(Query {
                outcomes: vec![Outcome::Denied, Outcome::Allowed],
                ..Query::default()
            }),
            2
        );
        assert_eq!(
            count(Query {
                kind: Some(Kind::Control),
                ..Query::default()
            }),
            1
        );
        assert_eq!(
            count(Query {
                since: Some(150),
                until: Some(250),
                ..Query::default()
            }),
            1
        );
        // Case folds beyond ASCII, over the call and the reason.
        assert_eq!(
            count(Query {
                text: Some("api-7F9C".into()),
                ..Query::default()
            }),
            1
        );
        // Every reason says "produção"; one of them as "PRODUÇÃO".
        assert_eq!(
            count(Query {
                text: Some("produção".into()),
                ..Query::default()
            }),
            3
        );
        assert_eq!(
            count(Query {
                text: Some("PRODUÇÃO".into()),
                tool: Some("kubectl".into()),
                ..Query::default()
            }),
            1
        );
        assert_eq!(
            count(Query {
                text: Some("nada".into()),
                ..Query::default()
            }),
            0
        );
        assert_eq!(
            count(Query {
                limit: Some(2),
                ..Query::default()
            }),
            2
        );
    }

    #[test]
    fn prune_drops_old_records_with_their_events_and_outputs() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let old = ledger.insert_call(&record(100, &["get"])).unwrap();
        let fresh = ledger.insert_call(&record(900, &["get"])).unwrap();
        for call_id in [old, fresh] {
            ledger.add_event(call_id, &event("ran")).unwrap();
            ledger.put_output(call_id, &Output::default()).unwrap();
        }
        assert_eq!(ledger.prune(500).unwrap(), 1);
        assert!(ledger.call(old).unwrap().is_none());
        assert!(ledger.events(old).unwrap().is_empty());
        assert!(ledger.output(old).unwrap().is_none());
        assert_eq!(ledger.events(fresh).unwrap().len(), 1);
        assert!(ledger.output(fresh).unwrap().is_some());
    }

    /// A pruned number is never handed out again, so a `#16` written in a
    /// ticket keeps meaning the same call.
    #[test]
    fn numbers_are_not_reused_after_a_prune_and_the_range_follows() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        assert_eq!(ledger.id_range().unwrap(), None);
        let first = ledger.insert_call(&record(100, &["get"])).unwrap();
        let second = ledger.insert_call(&record(900, &["get"])).unwrap();
        ledger.prune(500).unwrap();
        let third = ledger.insert_call(&record(950, &["get"])).unwrap();
        assert!(third > second && second > first);
        assert_eq!(ledger.id_range().unwrap(), Some((second, third)));
    }

    #[test]
    fn ending_with_matches_the_last_digits_newest_first() {
        let temp = tempfile::TempDir::new().unwrap();
        let ledger = ledger(&temp);
        let ids = (0..25)
            .map(|index| ledger.insert_call(&record(index, &["get"])).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids[11], 12);
        assert_eq!(ledger.ending_with("12", 10).unwrap(), [12]);
        assert_eq!(ledger.ending_with("2", 10).unwrap(), [22, 12, 2]);
        assert_eq!(ledger.ending_with("2", 2).unwrap(), [22, 12]);
        assert!(ledger.ending_with("99", 10).unwrap().is_empty());
    }

    #[test]
    fn a_database_with_another_key_is_set_aside_not_overwritten() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("audit.db");
        let first = open_for_test(&path, &test_key(1));
        first.insert_call(&record(1, &["get"])).unwrap();
        drop(first);
        let second = open_for_test(&path, &test_key(2));
        assert!(second.all().unwrap().is_empty());
        drop(second);
        let aside = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".unreadable.db"))
            .collect::<Vec<_>>();
        assert_eq!(aside.len(), 1, "{aside:?}");
        // The first key still opens what was set aside.
        let kept = open_for_test(&temp.path().join(&aside[0]), &test_key(1));
        assert_eq!(kept.all().unwrap().len(), 1);
    }

    #[test]
    fn a_plain_sqlite_file_is_set_aside_too() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("audit.db");
        std::fs::write(&path, b"SQLite format 3\0not encrypted").unwrap();
        let ledger = open_for_test(&path, &test_key(1));
        assert!(ledger.all().unwrap().is_empty());
    }

    /// Several Torii servers write into one database at once. Every record
    /// lands, each with exactly its own events, and none is lost or mixed.
    #[test]
    fn concurrent_writers_lose_nothing_and_mix_nothing() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("audit.db");
        drop(open_for_test(&path, &test_key(1)));
        let writers = (0..6)
            .map(|writer| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let ledger = open_for_test(&path, &test_key(1));
                    for call in 0..40 {
                        let tag = format!("writer-{writer}-call-{call}");
                        let id = ledger.insert_call(&record(call, &["get", &tag])).unwrap();
                        for step in 0..3 {
                            ledger
                                .add_event(id, &event(&format!("{tag}-step-{step}")))
                                .unwrap();
                        }
                        ledger
                            .put_output(
                                id,
                                &Output {
                                    stdout: tag.repeat(50),
                                    stderr: String::new(),
                                },
                            )
                            .unwrap();
                    }
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap();
        }
        let ledger = open_for_test(&path, &test_key(1));
        let entries = ledger.all().unwrap();
        assert_eq!(entries.len(), 6 * 40);
        for entry in entries {
            let tag = &entry.record.args[1];
            let events = ledger.events(entry.id).unwrap();
            assert_eq!(events.len(), 3, "{tag}");
            for (step, event) in events.iter().enumerate() {
                assert_eq!(event.name, format!("{tag}-step-{step}"));
            }
            assert_eq!(
                ledger.output(entry.id).unwrap().unwrap().stdout,
                tag.repeat(50)
            );
        }
    }
}
