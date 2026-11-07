//! SQLite History store. Every call is a short transaction; callers must never hold the
//! connection while waiting on a provider, the speech worker, or OS automation.

use crate::ipc_types::*;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

pub const DB_SCHEMA_VERSION: i64 = 1;

const MIGRATIONS: &[&str] = &[
    // v1 — minimum schema from the product specification.
    r#"
    CREATE TABLE dictations (
      id TEXT PRIMARY KEY,
      created_at_utc TEXT NOT NULL,
      updated_at_utc TEXT NOT NULL,
      source TEXT NOT NULL,
      audio_relative_path TEXT,
      audio_state TEXT NOT NULL,
      duration_ms INTEGER NOT NULL DEFAULT 0,
      original_text TEXT NOT NULL DEFAULT '',
      polished_text TEXT,
      final_text TEXT NOT NULL DEFAULT '',
      detected_language TEXT,
      requested_task TEXT NOT NULL,
      model_id TEXT NOT NULL,
      model_revision TEXT,
      destination_app_id TEXT,
      destination_app_name TEXT,
      processing_state TEXT NOT NULL,
      insertion_state TEXT NOT NULL,
      polish_state TEXT NOT NULL,
      error_code TEXT,
      error_message TEXT,
      segments_json TEXT,
      raw_result_json TEXT,
      settings_snapshot_json TEXT NOT NULL,
      schema_version INTEGER NOT NULL,
      import_name TEXT
    );
    CREATE INDEX dictations_created ON dictations(created_at_utc DESC);
    CREATE TABLE capture_journal (
      session_id TEXT PRIMARY KEY,
      dictation_id TEXT NOT NULL,
      capture_relative_path TEXT NOT NULL,
      format_json TEXT NOT NULL,
      frames_written INTEGER NOT NULL DEFAULT 0,
      started_at_utc TEXT NOT NULL,
      last_flush_at_utc TEXT NOT NULL,
      state TEXT NOT NULL,
      FOREIGN KEY(dictation_id) REFERENCES dictations(id) ON DELETE CASCADE
    );
    CREATE TABLE transcription_attempts (
      id TEXT PRIMARY KEY,
      dictation_id TEXT NOT NULL,
      started_at_utc TEXT NOT NULL,
      finished_at_utc TEXT,
      settings_snapshot_json TEXT NOT NULL,
      outcome TEXT NOT NULL,
      error_code TEXT,
      FOREIGN KEY(dictation_id) REFERENCES dictations(id) ON DELETE CASCADE
    );
    CREATE INDEX attempts_dictation ON transcription_attempts(dictation_id);
    CREATE TABLE pending_file_ops (
      id TEXT PRIMARY KEY,
      dictation_id TEXT,
      op TEXT NOT NULL,
      relative_path TEXT NOT NULL,
      created_at_utc TEXT NOT NULL,
      error TEXT
    );
    "#,
];

#[derive(Debug, Clone)]
pub struct DictationRow {
    pub id: String,
    pub created_at_utc: String,
    pub updated_at_utc: String,
    pub source: DictationSource,
    pub audio_relative_path: Option<String>,
    pub audio_state: AudioState,
    pub duration_ms: i64,
    pub original_text: String,
    pub polished_text: Option<String>,
    pub final_text: String,
    pub detected_language: Option<String>,
    pub requested_task: String,
    pub model_id: String,
    pub model_revision: Option<String>,
    pub destination_app_id: Option<String>,
    pub destination_app_name: Option<String>,
    pub processing_state: ProcessingState,
    pub insertion_state: InsertionState,
    pub polish_state: PolishState,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub segments_json: Option<String>,
    pub raw_result_json: Option<String>,
    pub settings_snapshot_json: String,
    pub import_name: Option<String>,
}

impl DictationRow {
    pub fn new(source: DictationSource, model_id: &str, task: &str, settings_snapshot_json: String) -> Self {
        let now = now_utc();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            created_at_utc: now.clone(),
            updated_at_utc: now,
            source,
            audio_relative_path: None,
            audio_state: AudioState::None,
            duration_ms: 0,
            original_text: String::new(),
            polished_text: None,
            final_text: String::new(),
            detected_language: None,
            requested_task: task.into(),
            model_id: model_id.into(),
            model_revision: None,
            destination_app_id: None,
            destination_app_name: None,
            processing_state: ProcessingState::Recording,
            insertion_state: InsertionState::NotRequested,
            polish_state: PolishState::Off,
            error_code: None,
            error_message: None,
            segments_json: None,
            raw_result_json: None,
            settings_snapshot_json,
            import_name: None,
        }
    }

    pub fn segments(&self) -> Vec<Segment> {
        self.segments_json.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
    }

    pub fn text_variant(&self, v: TextVariant) -> Option<&str> {
        match v {
            TextVariant::Final => Some(self.final_text.as_str()),
            TextVariant::Original => Some(self.original_text.as_str()),
            TextVariant::Polished => self.polished_text.as_deref(),
        }
    }

    pub fn to_entry(&self, audio_exists: bool) -> HistoryEntry {
        let has_text = !self.original_text.trim().is_empty();
        let in_flight = matches!(
            self.processing_state,
            ProcessingState::Recording | ProcessingState::Queued | ProcessingState::Transcribing | ProcessingState::Polishing
        );
        let (can_retry, reason) = if in_flight {
            (false, Some("This entry is still being processed.".to_string()))
        } else if self.audio_state == AudioState::RemovedByRetention {
            (false, Some("The recording was removed by your audio retention setting.".to_string()))
        } else if self.audio_relative_path.is_none() || !audio_exists {
            (false, Some("No saved recording is available for this entry.".to_string()))
        } else {
            (true, None)
        };
        HistoryEntry {
            id: self.id.clone(),
            created_at_utc: self.created_at_utc.clone(),
            source: self.source,
            audio_state: if self.audio_relative_path.is_some() && !audio_exists && self.audio_state == AudioState::Saved {
                AudioState::Missing
            } else {
                self.audio_state
            },
            duration_ms: self.duration_ms,
            original_text: self.original_text.clone(),
            polished_text: self.polished_text.clone(),
            final_text: self.final_text.clone(),
            detected_language: self.detected_language.clone(),
            model_id: self.model_id.clone(),
            destination_app_name: self.destination_app_name.clone(),
            processing_state: self.processing_state,
            polish_state: self.polish_state,
            insertion_state: self.insertion_state,
            error_code: self.error_code.clone(),
            error_message: self.error_message.clone(),
            has_segments: self.segments_json.as_deref().is_some_and(|s| s.len() > 2),
            can_retry,
            retry_unavailable_reason: reason,
            retry_kind: if !can_retry {
                None
            } else if has_text {
                Some(RetryKind::Retry)
            } else {
                Some(RetryKind::Recover)
            },
            import_name: self.import_name.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JournalRow {
    pub session_id: String,
    pub dictation_id: String,
    pub capture_relative_path: String,
    pub format_json: String,
    pub frames_written: i64,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct PendingFileOp {
    pub id: String,
    pub dictation_id: Option<String>,
    pub op: String,
    pub relative_path: String,
}

pub fn now_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub struct Db {
    conn: Mutex<Connection>,
}

pub type DbResult<T> = Result<T, String>;

fn e(err: rusqlite::Error) -> String {
    format!("History database error: {err}")
}

impl Db {
    /// Opens (never recreates) the database. A corrupt file is left untouched.
    pub fn open(path: &Path) -> DbResult<Self> {
        let conn = Connection::open(path).map_err(e)?;
        conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(e)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL;")
            .map_err(|err| format!("The History database could not be opened and was left untouched: {err}"))?;
        let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0)).map_err(e)?;
        if ok != "ok" {
            return Err(format!("The History database failed an integrity check and was left untouched ({ok})."));
        }
        crate::paths::restrict_to_user(path);
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    pub fn open_in_memory() -> DbResult<Self> {
        let conn = Connection::open_in_memory().map_err(e)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;").map_err(e)?;
        let db = Self { conn: Mutex::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> DbResult<()> {
        let mut conn = self.conn.lock();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(e)?;
        if version > MIGRATIONS.len() as i64 {
            return Err("History was created by a newer version of this app. Update the app to open it.".into());
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction().map_err(e)?;
            tx.execute_batch(sql).map_err(e)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64).map_err(e)?;
            tx.commit().map_err(e)?;
        }
        Ok(())
    }

    pub fn insert(&self, r: &DictationRow) -> DbResult<()> {
        self.conn
            .lock()
            .execute(
                "INSERT INTO dictations (id, created_at_utc, updated_at_utc, source, audio_relative_path, audio_state,
                 duration_ms, original_text, polished_text, final_text, detected_language, requested_task, model_id,
                 model_revision, destination_app_id, destination_app_name, processing_state, insertion_state, polish_state,
                 error_code, error_message, segments_json, raw_result_json, settings_snapshot_json, schema_version, import_name)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)",
                params![
                    r.id,
                    r.created_at_utc,
                    r.updated_at_utc,
                    enum_to_str(&r.source),
                    r.audio_relative_path,
                    enum_to_str(&r.audio_state),
                    r.duration_ms,
                    r.original_text,
                    r.polished_text,
                    r.final_text,
                    r.detected_language,
                    r.requested_task,
                    r.model_id,
                    r.model_revision,
                    r.destination_app_id,
                    r.destination_app_name,
                    enum_to_str(&r.processing_state),
                    enum_to_str(&r.insertion_state),
                    enum_to_str(&r.polish_state),
                    r.error_code,
                    r.error_message,
                    r.segments_json,
                    r.raw_result_json,
                    r.settings_snapshot_json,
                    DB_SCHEMA_VERSION,
                    r.import_name,
                ],
            )
            .map(|_| ())
            .map_err(e)
    }

    /// Persists every mutable column of a row. Callers load, modify, and save.
    pub fn save(&self, r: &DictationRow) -> DbResult<()> {
        let n = self
            .conn
            .lock()
            .execute(
                "UPDATE dictations SET updated_at_utc=?2, audio_relative_path=?3, audio_state=?4, duration_ms=?5,
                 original_text=?6, polished_text=?7, final_text=?8, detected_language=?9, model_id=?10, model_revision=?11,
                 destination_app_id=?12, destination_app_name=?13, processing_state=?14, insertion_state=?15,
                 polish_state=?16, error_code=?17, error_message=?18, segments_json=?19, raw_result_json=?20,
                 settings_snapshot_json=?21, requested_task=?22
                 WHERE id=?1",
                params![
                    r.id,
                    now_utc(),
                    r.audio_relative_path,
                    enum_to_str(&r.audio_state),
                    r.duration_ms,
                    r.original_text,
                    r.polished_text,
                    r.final_text,
                    r.detected_language,
                    r.model_id,
                    r.model_revision,
                    r.destination_app_id,
                    r.destination_app_name,
                    enum_to_str(&r.processing_state),
                    enum_to_str(&r.insertion_state),
                    enum_to_str(&r.polish_state),
                    r.error_code,
                    r.error_message,
                    r.segments_json,
                    r.raw_result_json,
                    r.settings_snapshot_json,
                    r.requested_task,
                ],
            )
            .map_err(e)?;
        if n == 0 {
            return Err("The History entry no longer exists.".into());
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> DbResult<Option<DictationRow>> {
        self.conn
            .lock()
            .query_row("SELECT * FROM dictations WHERE id = ?1", [id], row_to_dictation)
            .optional()
            .map_err(e)
    }

    /// Newest first. The search is case-insensitive over transcript text and app names.
    pub fn list(&self, query: Option<&str>) -> DbResult<Vec<DictationRow>> {
        let conn = self.conn.lock();
        let q = query.map(str::trim).filter(|q| !q.is_empty());
        let mut out = Vec::new();
        match q {
            None => {
                let mut stmt = conn.prepare("SELECT * FROM dictations ORDER BY created_at_utc DESC").map_err(e)?;
                for r in stmt.query_map([], row_to_dictation).map_err(e)? {
                    out.push(r.map_err(e)?);
                }
            }
            Some(q) => {
                // SQLite's LIKE folds ASCII case only, so filter in Rust with Unicode lowercasing.
                let needle = q.to_lowercase();
                let mut stmt = conn.prepare("SELECT * FROM dictations ORDER BY created_at_utc DESC").map_err(e)?;
                for r in stmt.query_map([], row_to_dictation).map_err(e)? {
                    let r = r.map_err(e)?;
                    let hay = [
                        Some(r.final_text.as_str()),
                        Some(r.original_text.as_str()),
                        r.polished_text.as_deref(),
                        r.destination_app_name.as_deref(),
                        r.import_name.as_deref(),
                    ];
                    if hay.iter().flatten().any(|h| h.to_lowercase().contains(&needle)) {
                        out.push(r);
                    }
                }
            }
        }
        Ok(out)
    }

    pub fn count(&self) -> DbResult<u64> {
        self.conn
            .lock()
            .query_row("SELECT COUNT(*) FROM dictations", [], |r| r.get::<_, i64>(0))
            .map(|n| n as u64)
            .map_err(e)
    }

    pub fn latest_with_text(&self) -> DbResult<Option<DictationRow>> {
        self.conn
            .lock()
            .query_row(
                "SELECT * FROM dictations WHERE trim(final_text) != '' ORDER BY created_at_utc DESC LIMIT 1",
                [],
                row_to_dictation,
            )
            .optional()
            .map_err(e)
    }

    pub fn delete(&self, id: &str) -> DbResult<()> {
        self.conn.lock().execute("DELETE FROM dictations WHERE id = ?1", [id]).map(|_| ()).map_err(e)
    }

    pub fn delete_all(&self) -> DbResult<()> {
        self.conn
            .lock()
            .execute_batch("DELETE FROM pending_file_ops; DELETE FROM dictations;")
            .map_err(e)
    }

    // ---- capture journal ----

    pub fn insert_journal(&self, j: &JournalRow) -> DbResult<()> {
        let now = now_utc();
        self.conn
            .lock()
            .execute(
                "INSERT INTO capture_journal (session_id, dictation_id, capture_relative_path, format_json, frames_written,
                 started_at_utc, last_flush_at_utc, state) VALUES (?1,?2,?3,?4,?5,?6,?6,?7)",
                params![j.session_id, j.dictation_id, j.capture_relative_path, j.format_json, j.frames_written, now, j.state],
            )
            .map(|_| ())
            .map_err(e)
    }

    pub fn update_journal(&self, session_id: &str, frames_written: i64, state: &str) -> DbResult<()> {
        self.conn
            .lock()
            .execute(
                "UPDATE capture_journal SET frames_written=?2, last_flush_at_utc=?3, state=?4 WHERE session_id=?1",
                params![session_id, frames_written, now_utc(), state],
            )
            .map(|_| ())
            .map_err(e)
    }

    pub fn update_journal_format(&self, session_id: &str, format_json: &str) -> DbResult<()> {
        self.conn
            .lock()
            .execute("UPDATE capture_journal SET format_json=?2 WHERE session_id=?1", params![session_id, format_json])
            .map(|_| ())
            .map_err(e)
    }

    pub fn delete_journal(&self, session_id: &str) -> DbResult<()> {
        self.conn
            .lock()
            .execute("DELETE FROM capture_journal WHERE session_id=?1", [session_id])
            .map(|_| ())
            .map_err(e)
    }

    pub fn open_journals(&self) -> DbResult<Vec<JournalRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT session_id, dictation_id, capture_relative_path, format_json, frames_written, state
                 FROM capture_journal",
            )
            .map_err(e)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(JournalRow {
                    session_id: r.get(0)?,
                    dictation_id: r.get(1)?,
                    capture_relative_path: r.get(2)?,
                    format_json: r.get(3)?,
                    frames_written: r.get(4)?,
                    state: r.get(5)?,
                })
            })
            .map_err(e)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e)
    }

    // ---- attempts ----

    pub fn start_attempt(&self, dictation_id: &str, settings_snapshot_json: &str) -> DbResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn
            .lock()
            .execute(
                "INSERT INTO transcription_attempts (id, dictation_id, started_at_utc, settings_snapshot_json, outcome)
                 VALUES (?1,?2,?3,?4,'running')",
                params![id, dictation_id, now_utc(), settings_snapshot_json],
            )
            .map_err(e)?;
        Ok(id)
    }

    pub fn finish_attempt(&self, attempt_id: &str, outcome: &str, error_code: Option<&str>) -> DbResult<()> {
        self.conn
            .lock()
            .execute(
                "UPDATE transcription_attempts SET finished_at_utc=?2, outcome=?3, error_code=?4 WHERE id=?1",
                params![attempt_id, now_utc(), outcome, error_code],
            )
            .map(|_| ())
            .map_err(e)
    }

    pub fn attempt_count(&self, dictation_id: &str) -> DbResult<i64> {
        self.conn
            .lock()
            .query_row("SELECT COUNT(*) FROM transcription_attempts WHERE dictation_id=?1", [dictation_id], |r| r.get(0))
            .map_err(e)
    }

    /// Startup reconciliation: attempts and rows that were mid-flight when the app stopped.
    pub fn mark_interrupted(&self) -> DbResult<Vec<String>> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction().map_err(e)?;
        tx.execute(
            "UPDATE transcription_attempts SET outcome='interrupted', finished_at_utc=?1 WHERE outcome='running'",
            [now_utc()],
        )
        .map_err(e)?;
        let ids: Vec<String> = {
            let mut stmt = tx
                .prepare(
                    "SELECT id FROM dictations WHERE processing_state IN ('recording','queued','transcribing','polishing')",
                )
                .map_err(e)?;
            let rows = stmt.query_map([], |r| r.get(0)).map_err(e)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(e)?
        };
        tx.execute(
            "UPDATE dictations SET processing_state='interrupted', updated_at_utc=?1,
             error_code=COALESCE(error_code,'interrupted'),
             error_message=COALESCE(error_message,'The app stopped before this entry finished. Recover it when you are ready.')
             WHERE processing_state IN ('recording','queued','transcribing','polishing')",
            [now_utc()],
        )
        .map_err(e)?;
        tx.commit().map_err(e)?;
        Ok(ids)
    }

    // ---- pending file operations (filesystem ⇄ database reconciliation) ----

    pub fn add_file_op(&self, dictation_id: Option<&str>, op: &str, relative_path: &str) -> DbResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn
            .lock()
            .execute(
                "INSERT INTO pending_file_ops (id, dictation_id, op, relative_path, created_at_utc) VALUES (?1,?2,?3,?4,?5)",
                params![id, dictation_id, op, relative_path, now_utc()],
            )
            .map_err(e)?;
        Ok(id)
    }

    pub fn complete_file_op(&self, id: &str) -> DbResult<()> {
        self.conn.lock().execute("DELETE FROM pending_file_ops WHERE id=?1", [id]).map(|_| ()).map_err(e)
    }

    pub fn fail_file_op(&self, id: &str, error: &str) -> DbResult<()> {
        self.conn
            .lock()
            .execute("UPDATE pending_file_ops SET error=?2 WHERE id=?1", params![id, error])
            .map(|_| ())
            .map_err(e)
    }

    pub fn pending_file_ops(&self) -> DbResult<Vec<PendingFileOp>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, dictation_id, op, relative_path FROM pending_file_ops").map_err(e)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(PendingFileOp { id: r.get(0)?, dictation_id: r.get(1)?, op: r.get(2)?, relative_path: r.get(3)? })
            })
            .map_err(e)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e)
    }

    /// Entries whose audio is due for removal under a retention policy.
    pub fn audio_older_than(&self, cutoff_utc: &str) -> DbResult<Vec<DictationRow>> {
        let conn = self.conn.lock();
        let mut stmt = conn
            .prepare(
                "SELECT * FROM dictations WHERE audio_relative_path IS NOT NULL AND audio_state='saved'
                 AND processing_state IN ('done','noSpeech') AND created_at_utc < ?1",
            )
            .map_err(e)?;
        let rows = stmt.query_map([cutoff_utc], row_to_dictation).map_err(e)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(e)
    }
}

fn parse_enum<T: for<'de> serde::Deserialize<'de>>(r: &Row, col: &str, fallback: T) -> rusqlite::Result<T> {
    let s: String = r.get(col)?;
    Ok(enum_from_str(&s).unwrap_or(fallback))
}

fn row_to_dictation(r: &Row) -> rusqlite::Result<DictationRow> {
    Ok(DictationRow {
        id: r.get("id")?,
        created_at_utc: r.get("created_at_utc")?,
        updated_at_utc: r.get("updated_at_utc")?,
        source: parse_enum(r, "source", DictationSource::Dictation)?,
        audio_relative_path: r.get("audio_relative_path")?,
        audio_state: parse_enum(r, "audio_state", AudioState::None)?,
        duration_ms: r.get("duration_ms")?,
        original_text: r.get("original_text")?,
        polished_text: r.get("polished_text")?,
        final_text: r.get("final_text")?,
        detected_language: r.get("detected_language")?,
        requested_task: r.get("requested_task")?,
        model_id: r.get("model_id")?,
        model_revision: r.get("model_revision")?,
        destination_app_id: r.get("destination_app_id")?,
        destination_app_name: r.get("destination_app_name")?,
        processing_state: parse_enum(r, "processing_state", ProcessingState::Failed)?,
        insertion_state: parse_enum(r, "insertion_state", InsertionState::NotRequested)?,
        polish_state: parse_enum(r, "polish_state", PolishState::Off)?,
        error_code: r.get("error_code")?,
        error_message: r.get("error_message")?,
        segments_json: r.get("segments_json")?,
        raw_result_json: r.get("raw_result_json")?,
        settings_snapshot_json: r.get("settings_snapshot_json")?,
        import_name: r.get("import_name")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(text: &str, app: Option<&str>) -> DictationRow {
        let mut r = DictationRow::new(DictationSource::Dictation, "base", "transcribe", "{}".into());
        r.original_text = text.into();
        r.final_text = text.into();
        r.destination_app_name = app.map(Into::into);
        r.processing_state = ProcessingState::Done;
        r
    }

    #[test]
    fn migrations_are_idempotent_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("h.sqlite3");
        Db::open(&f).unwrap().insert(&row("hello", None)).unwrap();
        let db = Db::open(&f).unwrap();
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn corrupt_database_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("h.sqlite3");
        std::fs::write(&f, b"this is not a database at all, definitely not").unwrap();
        assert!(Db::open(&f).is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"this is not a database at all, definitely not");
    }

    #[test]
    fn search_is_case_insensitive_over_text_and_app() {
        let db = Db::open_in_memory().unwrap();
        db.insert(&row("Ünïcode Grüße", Some("Notes"))).unwrap();
        db.insert(&row("other", Some("Visual Studio Code"))).unwrap();
        assert_eq!(db.list(Some("grüSSe")).unwrap().len(), 0); // ß folding is not claimed
        assert_eq!(db.list(Some("GRÜSSE")).unwrap().len(), 0);
        assert_eq!(db.list(Some("ünïCODE")).unwrap().len(), 1);
        assert_eq!(db.list(Some("studio")).unwrap().len(), 1);
        assert_eq!(db.list(Some("  ")).unwrap().len(), 2);
    }

    #[test]
    fn interrupted_processing_is_marked_once() {
        let db = Db::open_in_memory().unwrap();
        let mut r = row("", None);
        r.processing_state = ProcessingState::Transcribing;
        db.insert(&r).unwrap();
        db.start_attempt(&r.id, "{}").unwrap();
        assert_eq!(db.mark_interrupted().unwrap(), vec![r.id.clone()]);
        assert!(db.mark_interrupted().unwrap().is_empty(), "idempotent across launches");
        let got = db.get(&r.id).unwrap().unwrap();
        assert_eq!(got.processing_state, ProcessingState::Interrupted);
    }

    #[test]
    fn delete_cascades_attempts_and_journal() {
        let db = Db::open_in_memory().unwrap();
        let r = row("x", None);
        db.insert(&r).unwrap();
        db.start_attempt(&r.id, "{}").unwrap();
        db.insert_journal(&JournalRow {
            session_id: "s".into(),
            dictation_id: r.id.clone(),
            capture_relative_path: ".capture/s.wav".into(),
            format_json: "{}".into(),
            frames_written: 0,
            state: "capturing".into(),
        })
        .unwrap();
        db.delete(&r.id).unwrap();
        assert_eq!(db.attempt_count(&r.id).unwrap(), 0);
        assert!(db.open_journals().unwrap().is_empty());
    }

    #[test]
    fn retry_availability_is_derived() {
        let mut r = row("", None);
        r.audio_relative_path = Some("a.wav".into());
        r.audio_state = AudioState::Saved;
        r.processing_state = ProcessingState::Failed;
        let e = r.to_entry(true);
        assert!(e.can_retry);
        assert_eq!(e.retry_kind, Some(RetryKind::Recover));
        r.original_text = "hi".into();
        assert_eq!(r.to_entry(true).retry_kind, Some(RetryKind::Retry));
        r.audio_state = AudioState::RemovedByRetention;
        assert!(!r.to_entry(false).can_retry);
        assert!(r.to_entry(false).retry_unavailable_reason.unwrap().contains("retention"));
    }
}
