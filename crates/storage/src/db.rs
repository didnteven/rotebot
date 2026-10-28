//! SQLCipher-backed product database (plan §3.3, ship blockers P3/P4).
//!
//! One `Db` owns one connection; the app core is its only writer. Rows hold the
//! fields the core filters or sorts on as columns, and everything else as JSON
//! the core defines, so DTO changes do not need migrations. Money is integer
//! microdollars. Secrets never belong here: keys and tokens live in the vault.

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde_json::Value;
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database key rejected or file is not a Macroscope database")]
    KeyRejected,
    #[error("database error: {0}")]
    Sql(String),
    #[error("stored JSON is invalid: {0}")]
    Json(String),
    #[error("database schema is newer than this app supports")]
    FutureSchema,
    #[error("database lock poisoned")]
    Poisoned,
}

impl From<rusqlite::Error> for DbError {
    fn from(e: rusqlite::Error) -> Self {
        DbError::Sql(e.to_string())
    }
}

impl From<serde_json::Error> for DbError {
    fn from(e: serde_json::Error) -> Self {
        DbError::Json(e.to_string())
    }
}

/// Ordered migrations; index + 1 is the schema version. Never edit a shipped entry.
const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE tools (
  id TEXT PRIMARY KEY, name TEXT NOT NULL, active_version TEXT, state TEXT NOT NULL,
  sample INTEGER NOT NULL DEFAULT 0, created_ms INTEGER NOT NULL, last_run_ms INTEGER,
  run_count INTEGER NOT NULL DEFAULT 0, detail_json TEXT NOT NULL);
CREATE TABLE versions (
  tool_id TEXT NOT NULL REFERENCES tools(id) ON DELETE CASCADE, hash TEXT NOT NULL,
  manifest_json TEXT NOT NULL, mapping_json TEXT NOT NULL, created_ms INTEGER NOT NULL,
  PRIMARY KEY (tool_id, hash));
CREATE TABLE grants (
  id TEXT PRIMARY KEY, tool_id TEXT NOT NULL REFERENCES tools(id) ON DELETE CASCADE,
  version_hash TEXT NOT NULL, revision INTEGER NOT NULL, scope_json TEXT NOT NULL,
  revoked_ms INTEGER);
CREATE TABLE runs (
  id TEXT PRIMARY KEY, tool_id TEXT NOT NULL, version_hash TEXT NOT NULL, state TEXT NOT NULL,
  plan_hash TEXT, receipt_json TEXT, failure_json TEXT, created_ms INTEGER NOT NULL);
CREATE INDEX runs_by_tool ON runs(tool_id, created_ms);
CREATE TABLE drafts (
  id TEXT PRIMARY KEY, revision INTEGER NOT NULL, draft_json TEXT NOT NULL,
  updated_ms INTEGER NOT NULL);
CREATE TABLE builds (
  id TEXT PRIMARY KEY, draft_id TEXT NOT NULL, route TEXT NOT NULL, phase TEXT NOT NULL,
  reserved_micros INTEGER NOT NULL, spent_micros INTEGER, state_json TEXT NOT NULL,
  created_ms INTEGER NOT NULL, updated_ms INTEGER NOT NULL);
CREATE TABLE model_usage (
  leg_id TEXT PRIMARY KEY, parent_id TEXT, model TEXT, usage_json TEXT,
  price_version TEXT NOT NULL, estimated_micros INTEGER, reconciliation TEXT NOT NULL);
CREATE TABLE outbound (
  id TEXT PRIMARY KEY, at_ms INTEGER NOT NULL, purpose TEXT NOT NULL, destination TEXT NOT NULL,
  route TEXT NOT NULL, synthetic_only INTEGER NOT NULL, cost_micros INTEGER,
  payload_text TEXT, payload_expires_ms INTEGER, metadata_expires_ms INTEGER NOT NULL);
CREATE TABLE activity (
  id TEXT PRIMARY KEY, at_ms INTEGER NOT NULL, kind TEXT NOT NULL, title TEXT NOT NULL,
  detail TEXT);
CREATE INDEX activity_by_time ON activity(at_ms);
CREATE TABLE settings_kv (key TEXT PRIMARY KEY, value_json TEXT NOT NULL);
CREATE TABLE connector_state (connector_id TEXT PRIMARY KEY, state_json TEXT NOT NULL);
CREATE TABLE detections (
  connector_id TEXT NOT NULL, signal_kind TEXT NOT NULL, count INTEGER NOT NULL,
  first_ms INTEGER NOT NULL, last_ms INTEGER NOT NULL, PRIMARY KEY (connector_id, signal_kind));
CREATE TABLE suggestion_dismissals (connector_id TEXT PRIMARY KEY, until_ms INTEGER);
"#,
    r#"
ALTER TABLE runs ADD COLUMN run_dir TEXT;
"#,
    r#"
CREATE TABLE deletion_journal (id INTEGER PRIMARY KEY CHECK (id = 1), include_tools INTEGER NOT NULL);
"#,
    r#"
CREATE TABLE schedules (id TEXT PRIMARY KEY, tool_id TEXT NOT NULL UNIQUE REFERENCES tools(id) ON DELETE CASCADE, data_json TEXT NOT NULL);
CREATE TABLE today (id TEXT PRIMARY KEY, data_json TEXT NOT NULL);
"#,
];

pub const SCHEMA_VERSION: usize = MIGRATIONS.len();

/// Outbound payload bodies expire after seven days, metadata after thirty (plan §8.3).
pub const OUTBOUND_PAYLOAD_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
pub const OUTBOUND_METADATA_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolRow {
    pub id: String,
    pub name: String,
    pub active_version: Option<String>,
    pub state: String,
    pub sample: bool,
    pub created_ms: i64,
    pub last_run_ms: Option<i64>,
    pub run_count: i64,
    pub detail: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VersionRow {
    pub tool_id: String,
    pub hash: String,
    pub manifest: Value,
    pub mapping: Value,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunRow {
    pub id: String,
    pub tool_id: String,
    pub version_hash: String,
    pub state: String,
    pub plan_hash: Option<String>,
    pub receipt: Option<Value>,
    pub failure: Option<Value>,
    pub created_ms: i64,
    /// Private staging directory holding the run's journal, for crash reconciliation.
    pub run_dir: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BuildRow {
    pub id: String,
    pub draft_id: String,
    pub route: String,
    pub phase: String,
    pub reserved_micros: i64,
    pub spent_micros: Option<i64>,
    pub state: Value,
    pub created_ms: i64,
    pub updated_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageRow {
    pub leg_id: String,
    pub parent_id: Option<String>,
    pub model: String,
    pub usage: Value,
    pub price_version: String,
    pub estimated_micros: Option<i64>,
    /// "estimated", "reconciled", or "unknown" (keeps its reservation).
    pub reconciliation: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OutboundRow {
    pub id: String,
    pub at_ms: i64,
    pub purpose: String,
    pub destination: String,
    pub route: String,
    pub synthetic_only: bool,
    pub cost_micros: Option<i64>,
    pub payload_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityRow {
    pub id: String,
    pub at_ms: i64,
    pub kind: String,
    pub title: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectionRow {
    pub connector_id: String,
    pub signal_kind: String,
    pub count: i64,
    pub first_ms: i64,
    pub last_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableStats {
    pub table: &'static str,
    pub rows: i64,
}

fn json(text: String) -> Result<Value, DbError> {
    Ok(serde_json::from_str(&text)?)
}

fn opt_json(text: Option<String>) -> Result<Option<Value>, DbError> {
    text.map(json).transpose()
}

impl Db {
    /// Opens (creating if needed) an encrypted database with a raw 256-bit key and
    /// applies pending migrations. A wrong key fails with `KeyRejected`.
    pub fn open(path: &Path, key: &[u8; 32]) -> Result<Db, DbError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        Self::init(conn, key)
    }

    /// An encrypted in-memory database, for tests.
    pub fn open_in_memory(key: &[u8; 32]) -> Result<Db, DbError> {
        Self::init(Connection::open_in_memory()?, key)
    }

    fn init(conn: Connection, key: &[u8; 32]) -> Result<Db, DbError> {
        // Raw key form skips SQLCipher's passphrase KDF; the key is already random.
        conn.execute_batch(&format!("PRAGMA key = \"x'{}'\";", hex::encode(key)))?;
        // The first read is where SQLCipher discovers a wrong key.
        if conn
            .query_row("SELECT count(*) FROM sqlite_master", [], |r| {
                r.get::<_, i64>(0)
            })
            .is_err()
        {
            return Err(DbError::KeyRejected);
        }
        conn.execute_batch(
            "PRAGMA cipher_memory_security = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA secure_delete = ON;",
        )?;
        let db = Db {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T, DbError>) -> Result<T, DbError> {
        let conn = self.conn.lock().map_err(|_| DbError::Poisoned)?;
        f(&conn)
    }

    fn migrate(&self) -> Result<(), DbError> {
        self.with(|c| {
            let current: usize =
                c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize;
            if current > SCHEMA_VERSION {
                return Err(DbError::FutureSchema);
            }
            for (index, sql) in MIGRATIONS.iter().enumerate().skip(current) {
                c.execute_batch(&format!(
                    "BEGIN; {sql} PRAGMA user_version = {}; COMMIT;",
                    index + 1
                ))?;
            }
            Ok(())
        })
    }

    pub fn schema_version(&self) -> Result<usize, DbError> {
        self.with(|c| Ok(c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? as usize))
    }

    // ---------- tools, versions, grants ----------

    pub fn put_tool(&self, t: &ToolRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO tools (id, name, active_version, state, sample, created_ms,
                   last_run_ms, run_count, detail_json) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
                 ON CONFLICT(id) DO UPDATE SET name=excluded.name,
                   active_version=excluded.active_version, state=excluded.state,
                   sample=excluded.sample, last_run_ms=excluded.last_run_ms,
                   run_count=excluded.run_count, detail_json=excluded.detail_json",
                params![
                    t.id,
                    t.name,
                    t.active_version,
                    t.state,
                    t.sample,
                    t.created_ms,
                    t.last_run_ms,
                    t.run_count,
                    t.detail.to_string()
                ],
            )?;
            Ok(())
        })
    }

    fn tool_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<(ToolRow, String)> {
        Ok((
            ToolRow {
                id: r.get(0)?,
                name: r.get(1)?,
                active_version: r.get(2)?,
                state: r.get(3)?,
                sample: r.get(4)?,
                created_ms: r.get(5)?,
                last_run_ms: r.get(6)?,
                run_count: r.get(7)?,
                detail: Value::Null,
            },
            r.get(8)?,
        ))
    }

    const TOOL_COLUMNS: &'static str = "id, name, active_version, state, sample, created_ms, \
        last_run_ms, run_count, detail_json";

    pub fn tool(&self, id: &str) -> Result<Option<ToolRow>, DbError> {
        self.with(|c| {
            let found = c
                .query_row(
                    &format!("SELECT {} FROM tools WHERE id = ?1", Self::TOOL_COLUMNS),
                    [id],
                    Self::tool_from_row,
                )
                .optional()?;
            found
                .map(|(mut t, d)| {
                    t.detail = json(d)?;
                    Ok(t)
                })
                .transpose()
        })
    }

    pub fn list_tools(&self) -> Result<Vec<ToolRow>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {} FROM tools ORDER BY created_ms DESC, id",
                Self::TOOL_COLUMNS
            ))?;
            let rows = stmt.query_map([], Self::tool_from_row)?;
            rows.map(|row| {
                let (mut t, d) = row?;
                t.detail = json(d)?;
                Ok(t)
            })
            .collect()
        })
    }

    /// Deletes a tool with its versions and grants (cascade) and its runs.
    pub fn delete_tool(&self, id: &str) -> Result<(), DbError> {
        self.with(|c| {
            c.execute("DELETE FROM runs WHERE tool_id = ?1", [id])?;
            c.execute("DELETE FROM tools WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// Versions are immutable: inserting an existing (tool, hash) pair is an error.
    pub fn insert_version(&self, v: &VersionRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO versions (tool_id, hash, manifest_json, mapping_json, created_ms)
                 VALUES (?1,?2,?3,?4,?5)",
                params![
                    v.tool_id,
                    v.hash,
                    v.manifest.to_string(),
                    v.mapping.to_string(),
                    v.created_ms
                ],
            )?;
            Ok(())
        })
    }

    pub fn version(&self, tool_id: &str, hash: &str) -> Result<Option<VersionRow>, DbError> {
        self.with(|c| {
            let found = c
                .query_row(
                    "SELECT manifest_json, mapping_json, created_ms FROM versions
                     WHERE tool_id = ?1 AND hash = ?2",
                    [tool_id, hash],
                    |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get(2)?)),
                )
                .optional()?;
            found
                .map(|(m, p, created_ms)| {
                    Ok(VersionRow {
                        tool_id: tool_id.into(),
                        hash: hash.into(),
                        manifest: json(m)?,
                        mapping: json(p)?,
                        created_ms,
                    })
                })
                .transpose()
        })
    }

    pub fn put_grant(
        &self,
        id: &str,
        tool_id: &str,
        version_hash: &str,
        revision: i64,
        scope: &Value,
    ) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO grants (id, tool_id, version_hash, revision, scope_json,
                   revoked_ms) VALUES (?1,?2,?3,?4,?5,NULL)",
                params![id, tool_id, version_hash, revision, scope.to_string()],
            )?;
            Ok(())
        })
    }

    /// The current (unrevoked, highest revision) grant for a tool version.
    pub fn active_grant(
        &self,
        tool_id: &str,
        version_hash: &str,
    ) -> Result<Option<(String, i64, Value)>, DbError> {
        self.with(|c| {
            let found = c
                .query_row(
                    "SELECT id, revision, scope_json FROM grants
                     WHERE tool_id = ?1 AND version_hash = ?2 AND revoked_ms IS NULL
                     ORDER BY revision DESC LIMIT 1",
                    [tool_id, version_hash],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, i64>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    },
                )
                .optional()?;
            found
                .map(|(id, rev, s)| Ok((id, rev, json(s)?)))
                .transpose()
        })
    }

    pub fn revoke_grants(&self, tool_id: &str, at_ms: i64) -> Result<usize, DbError> {
        self.with(|c| {
            Ok(c.execute(
                "UPDATE grants SET revoked_ms = ?2 WHERE tool_id = ?1 AND revoked_ms IS NULL",
                params![tool_id, at_ms],
            )?)
        })
    }

    /// Revisions never repeat, including after all previous grants were revoked.
    pub fn next_grant_revision(&self, tool_id: &str) -> Result<i64, DbError> {
        self.with(|c| {
            Ok(c.query_row(
                "SELECT coalesce(max(revision), 0) + 1 FROM grants WHERE tool_id = ?1",
                [tool_id],
                |r| r.get(0),
            )?)
        })
    }

    pub fn begin_deletion(&self, include_tools: bool) -> Result<(), DbError> {
        self.with(|c| {
            // Never narrow an unfinished wipe on a retry.
            c.execute("INSERT INTO deletion_journal VALUES (1, ?1)
                ON CONFLICT(id) DO UPDATE SET include_tools = max(include_tools, excluded.include_tools)", [include_tools])?;
            Ok(())
        })
    }

    pub fn pending_deletion(&self) -> Result<Option<bool>, DbError> {
        self.with(|c| {
            Ok(c.query_row(
                "SELECT include_tools FROM deletion_journal WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .optional()?)
        })
    }

    /// Keep the tombstone until the core has also removed files. Idempotent after a crash.
    pub fn delete_local_rows(&self, include_tools: bool) -> Result<(), DbError> {
        self.with(|c| {
            let tx = c.unchecked_transaction()?;
            tx.execute_batch(
                "DELETE FROM model_usage; DELETE FROM builds; DELETE FROM drafts;
                DELETE FROM outbound; DELETE FROM detections; DELETE FROM suggestion_dismissals;
                DELETE FROM activity; DELETE FROM today; DELETE FROM schedules;",
            )?;
            if include_tools {
                tx.execute_batch("DELETE FROM runs; DELETE FROM tools;")?;
            } else {
                tx.execute_batch(
                    "UPDATE runs SET run_dir = NULL,
                    state = CASE WHEN state = 'staged' THEN 'discarded' ELSE state END;",
                )?;
            }
            tx.commit()?;
            Ok(())
        })
    }

    pub fn finish_deletion(&self) -> Result<(), DbError> {
        self.with(|c| {
            c.execute("DELETE FROM deletion_journal", [])?;
            Ok(())
        })
    }

    pub fn put_schedule(&self, id: &str, tool_id: &str, data: &Value) -> Result<(), DbError> {
        self.with(|c| { c.execute("INSERT INTO schedules VALUES (?1,?2,?3) ON CONFLICT(tool_id) DO UPDATE SET data_json=excluded.data_json", params![id,tool_id,data.to_string()])?; Ok(()) })
    }
    pub fn schedule_rows(&self) -> Result<Vec<Value>, DbError> {
        self.with(|c| {
            let mut s = c.prepare("SELECT data_json FROM schedules ORDER BY id")?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|r| json(r?)).collect()
        })
    }
    pub fn remove_schedule(&self, tool_id: &str) -> Result<(), DbError> {
        self.with(|c| {
            c.execute("DELETE FROM schedules WHERE tool_id=?1", [tool_id])?;
            Ok(())
        })
    }
    pub fn put_today(&self, id: &str, data: &Value) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO today VALUES (?1,?2)",
                params![id, data.to_string()],
            )?;
            Ok(())
        })
    }
    pub fn today_rows(&self) -> Result<Vec<Value>, DbError> {
        self.with(|c| {
            let mut s = c.prepare("SELECT data_json FROM today")?;
            let rows = s.query_map([], |r| r.get::<_, String>(0))?;
            rows.map(|r| json(r?)).collect()
        })
    }
    pub fn dismiss_today(&self, id: &str) -> Result<(), DbError> {
        self.with(|c| {
            c.execute("DELETE FROM today WHERE id=?1", [id])?;
            Ok(())
        })
    }
    // ---------- runs ----------

    pub fn put_run(&self, r: &RunRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO runs (id, tool_id, version_hash, state, plan_hash,
                   receipt_json, failure_json, created_ms, run_dir)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    r.id,
                    r.tool_id,
                    r.version_hash,
                    r.state,
                    r.plan_hash,
                    r.receipt.as_ref().map(Value::to_string),
                    r.failure.as_ref().map(Value::to_string),
                    r.created_ms,
                    r.run_dir
                ],
            )?;
            Ok(())
        })
    }

    fn runs_where(&self, clause: &str, arg: Option<&str>) -> Result<Vec<RunRow>, DbError> {
        self.with(|c| {
            let sql = format!(
                "SELECT id, tool_id, version_hash, state, plan_hash, receipt_json, failure_json,
                   created_ms, run_dir FROM runs {clause} ORDER BY created_ms DESC, id"
            );
            let mut stmt = c.prepare(&sql)?;
            let map = |r: &rusqlite::Row<'_>| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, i64>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            };
            let rows: Vec<_> = match arg {
                Some(a) => stmt.query_map([a], map)?.collect::<Result<_, _>>()?,
                None => stmt.query_map([], map)?.collect::<Result<_, _>>()?,
            };
            rows.into_iter()
                .map(
                    |(
                        id,
                        tool_id,
                        version_hash,
                        state,
                        plan_hash,
                        receipt,
                        failure,
                        created_ms,
                        run_dir,
                    )| {
                        Ok(RunRow {
                            id,
                            tool_id,
                            version_hash,
                            state,
                            plan_hash,
                            receipt: opt_json(receipt)?,
                            failure: opt_json(failure)?,
                            created_ms,
                            run_dir,
                        })
                    },
                )
                .collect()
        })
    }

    pub fn run(&self, id: &str) -> Result<Option<RunRow>, DbError> {
        Ok(self
            .runs_where("WHERE id = ?1", Some(id))?
            .into_iter()
            .next())
    }

    pub fn list_runs(&self) -> Result<Vec<RunRow>, DbError> {
        self.runs_where("", None)
    }

    /// Runs that recorded a staging directory but no receipt: candidates for
    /// crash reconciliation at startup.
    pub fn unreceipted_runs(&self) -> Result<Vec<RunRow>, DbError> {
        Ok(self
            .runs_where("WHERE receipt_json IS NULL AND run_dir IS NOT NULL", None)?
            .into_iter()
            .collect())
    }

    pub fn runs_for_tool(&self, tool_id: &str) -> Result<Vec<RunRow>, DbError> {
        self.runs_where("WHERE tool_id = ?1", Some(tool_id))
    }

    // ---------- drafts and builds ----------

    pub fn put_draft(
        &self,
        id: &str,
        revision: i64,
        draft: &Value,
        now_ms: i64,
    ) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO drafts (id, revision, draft_json, updated_ms)
                 VALUES (?1,?2,?3,?4)",
                params![id, revision, draft.to_string(), now_ms],
            )?;
            Ok(())
        })
    }

    pub fn draft(&self, id: &str) -> Result<Option<(i64, Value)>, DbError> {
        self.with(|c| {
            let found = c
                .query_row(
                    "SELECT revision, draft_json FROM drafts WHERE id = ?1",
                    [id],
                    |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
                )
                .optional()?;
            found.map(|(rev, d)| Ok((rev, json(d)?))).transpose()
        })
    }

    pub fn put_build(&self, b: &BuildRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO builds (id, draft_id, route, phase, reserved_micros,
                   spent_micros, state_json, created_ms, updated_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    b.id,
                    b.draft_id,
                    b.route,
                    b.phase,
                    b.reserved_micros,
                    b.spent_micros,
                    b.state.to_string(),
                    b.created_ms,
                    b.updated_ms
                ],
            )?;
            Ok(())
        })
    }

    pub fn build(&self, id: &str) -> Result<Option<BuildRow>, DbError> {
        self.with(|c| {
            let found = c
                .query_row(
                    "SELECT draft_id, route, phase, reserved_micros, spent_micros, state_json,
                       created_ms, updated_ms FROM builds WHERE id = ?1",
                    [id],
                    |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                            r.get::<_, i64>(3)?,
                            r.get::<_, Option<i64>>(4)?,
                            r.get::<_, String>(5)?,
                            r.get::<_, i64>(6)?,
                            r.get::<_, i64>(7)?,
                        ))
                    },
                )
                .optional()?;
            found
                .map(
                    |(
                        draft_id,
                        route,
                        phase,
                        reserved_micros,
                        spent_micros,
                        s,
                        created_ms,
                        updated_ms,
                    )| {
                        Ok(BuildRow {
                            id: id.into(),
                            draft_id,
                            route,
                            phase,
                            reserved_micros,
                            spent_micros,
                            state: json(s)?,
                            created_ms,
                            updated_ms,
                        })
                    },
                )
                .transpose()
        })
    }

    /// Builds that were active when the app stopped; the core marks them interrupted.
    pub fn unfinished_builds(&self) -> Result<Vec<String>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id FROM builds WHERE phase NOT IN ('ready','failed','stopped') ORDER BY id",
            )?;
            let ids = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(ids)
        })
    }

    /// Sum of reservations for builds not yet reconciled (plan §9.1 admission limits).
    pub fn reserved_micros_since(&self, since_ms: i64) -> Result<i64, DbError> {
        self.with(|c| {
            Ok(c.query_row(
                "SELECT COALESCE(SUM(COALESCE(spent_micros, reserved_micros)), 0) FROM builds
                 WHERE created_ms >= ?1",
                [since_ms],
                |r| r.get(0),
            )?)
        })
    }

    /// One model-usage ledger leg (plan §9.1): deduplicated by `leg_id`.
    pub fn record_usage(&self, u: &UsageRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO model_usage (leg_id, parent_id, model, usage_json,
                   price_version, estimated_micros, reconciliation) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    u.leg_id,
                    u.parent_id,
                    u.model,
                    u.usage.to_string(),
                    u.price_version,
                    u.estimated_micros,
                    u.reconciliation
                ],
            )?;
            Ok(())
        })
    }

    /// Estimated spend for one parent job, counting each leg once. `None` if any leg
    /// has unknown usage, so callers keep the reservation (plan §9.1).
    pub fn usage_estimated_total(&self, parent_id: &str) -> Result<Option<i64>, DbError> {
        self.with(|c| {
            let (total, unknown, count): (i64, i64, i64) = c.query_row(
                "SELECT COALESCE(SUM(estimated_micros), 0),
                        COALESCE(SUM(estimated_micros IS NULL OR reconciliation = 'unknown'), 0), COUNT(*)
                 FROM model_usage WHERE parent_id = ?1",
                [parent_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            Ok((count > 0 && unknown == 0).then_some(total))
        })
    }

    // ---------- outbound, activity ----------

    pub fn insert_outbound(&self, o: &OutboundRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO outbound (id, at_ms, purpose, destination, route, synthetic_only,
                   cost_micros, payload_text, payload_expires_ms, metadata_expires_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    o.id,
                    o.at_ms,
                    o.purpose,
                    o.destination,
                    o.route,
                    o.synthetic_only,
                    o.cost_micros,
                    o.payload_text,
                    o.at_ms + OUTBOUND_PAYLOAD_TTL_MS,
                    o.at_ms + OUTBOUND_METADATA_TTL_MS
                ],
            )?;
            Ok(())
        })
    }

    /// Custom routes retain counts and usage only, never request or response text.
    pub fn set_outbound_metadata(&self, id: &str, metadata: &str) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "UPDATE outbound SET payload_text = ?2 WHERE id = ?1",
                params![id, metadata],
            )?;
            Ok(())
        })
    }

    pub fn set_outbound_cost(&self, id: &str, cost_micros: Option<i64>) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "UPDATE outbound SET cost_micros = ?2 WHERE id = ?1",
                params![id, cost_micros],
            )?;
            Ok(())
        })
    }

    pub fn list_outbound(&self) -> Result<Vec<OutboundRow>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, at_ms, purpose, destination, route, synthetic_only, cost_micros,
                   payload_text FROM outbound ORDER BY at_ms DESC, id",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(OutboundRow {
                        id: r.get(0)?,
                        at_ms: r.get(1)?,
                        purpose: r.get(2)?,
                        destination: r.get(3)?,
                        route: r.get(4)?,
                        synthetic_only: r.get(5)?,
                        cost_micros: r.get(6)?,
                        payload_text: r.get(7)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
    }

    /// Drops expired payload bodies and expired metadata rows. Returns (bodies, rows).
    pub fn purge_expired_outbound(&self, now_ms: i64) -> Result<(usize, usize), DbError> {
        self.with(|c| {
            let bodies = c.execute(
                "UPDATE outbound SET payload_text = NULL
                 WHERE payload_text IS NOT NULL AND payload_expires_ms <= ?1",
                [now_ms],
            )?;
            let rows = c.execute(
                "DELETE FROM outbound WHERE metadata_expires_ms <= ?1",
                [now_ms],
            )?;
            Ok((bodies, rows))
        })
    }

    pub fn delete_outbound_history(&self) -> Result<usize, DbError> {
        self.with(|c| Ok(c.execute("DELETE FROM outbound", [])?))
    }

    pub fn add_activity(&self, a: &ActivityRow) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO activity (id, at_ms, kind, title, detail)
                 VALUES (?1,?2,?3,?4,?5)",
                params![a.id, a.at_ms, a.kind, a.title, a.detail],
            )?;
            Ok(())
        })
    }

    pub fn list_activity(&self, limit: i64) -> Result<Vec<ActivityRow>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, at_ms, kind, title, detail FROM activity
                 ORDER BY at_ms DESC, id LIMIT ?1",
            )?;
            let rows = stmt
                .query_map([limit], |r| {
                    Ok(ActivityRow {
                        id: r.get(0)?,
                        at_ms: r.get(1)?,
                        kind: r.get(2)?,
                        title: r.get(3)?,
                        detail: r.get(4)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
    }

    // ---------- settings and connections ----------

    pub fn set_setting(&self, key: &str, value: &Value) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO settings_kv (key, value_json) VALUES (?1, ?2)",
                params![key, value.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn setting(&self, key: &str) -> Result<Option<Value>, DbError> {
        self.with(|c| {
            let found = c
                .query_row(
                    "SELECT value_json FROM settings_kv WHERE key = ?1",
                    [key],
                    |r| r.get::<_, String>(0),
                )
                .optional()?;
            opt_json(found)
        })
    }

    pub fn set_connector_state(&self, connector_id: &str, state: &Value) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO connector_state (connector_id, state_json) VALUES (?1, ?2)",
                params![connector_id, state.to_string()],
            )?;
            Ok(())
        })
    }

    pub fn connector_states(&self) -> Result<Vec<(String, Value)>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT connector_id, state_json FROM connector_state ORDER BY connector_id",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<Result<_, _>>()?;
            rows.into_iter().map(|(id, s)| Ok((id, json(s)?))).collect()
        })
    }

    /// Adds `count` sightings of a connector by one signal kind. Only ids, kinds,
    /// counts and times are stored — never the title or text that matched.
    pub fn record_detection(
        &self,
        connector_id: &str,
        signal_kind: &str,
        count: i64,
        at_ms: i64,
    ) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT INTO detections (connector_id, signal_kind, count, first_ms, last_ms)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(connector_id, signal_kind) DO UPDATE SET
                   count = count + excluded.count, last_ms = MAX(last_ms, excluded.last_ms)",
                params![connector_id, signal_kind, count, at_ms],
            )?;
            Ok(())
        })
    }

    pub fn detections(&self) -> Result<Vec<DetectionRow>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT connector_id, signal_kind, count, first_ms, last_ms FROM detections
                 ORDER BY count DESC, connector_id, signal_kind",
            )?;
            let rows = stmt
                .query_map([], |r| {
                    Ok(DetectionRow {
                        connector_id: r.get(0)?,
                        signal_kind: r.get(1)?,
                        count: r.get(2)?,
                        first_ms: r.get(3)?,
                        last_ms: r.get(4)?,
                    })
                })?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        })
    }

    /// `until_ms = None` dismisses permanently; `Some(t)` snoozes until `t`.
    pub fn dismiss_suggestion(
        &self,
        connector_id: &str,
        until_ms: Option<i64>,
    ) -> Result<(), DbError> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO suggestion_dismissals (connector_id, until_ms) VALUES (?1, ?2)",
                params![connector_id, until_ms],
            )?;
            Ok(())
        })
    }

    /// Connector ids whose suggestion is hidden at `now_ms`.
    pub fn hidden_suggestions(&self, now_ms: i64) -> Result<Vec<String>, DbError> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT connector_id FROM suggestion_dismissals
                 WHERE until_ms IS NULL OR until_ms > ?1 ORDER BY connector_id",
            )?;
            let ids = stmt
                .query_map([now_ms], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            Ok(ids)
        })
    }

    /// Row counts for the Privacy page.
    pub fn stats(&self) -> Result<Vec<TableStats>, DbError> {
        const TABLES: [&str; 13] = [
            "tools",
            "versions",
            "runs",
            "builds",
            "outbound",
            "activity",
            "detections",
            "drafts",
            "model_usage",
            "suggestion_dismissals",
            "grants",
            "today",
            "schedules",
        ];
        self.with(|c| {
            TABLES
                .iter()
                .map(|t| {
                    let rows =
                        c.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))?;
                    Ok(TableStats { table: t, rows })
                })
                .collect()
        })
    }
}
