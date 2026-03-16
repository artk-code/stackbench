use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use swb_core::{
    now_utc_rfc3339, ExternalRefDraft, ExternalRefRecord, IngestEnvelope, IngestKind,
    OutboundUpdateDraft, OutboundUpdateRecord, RunLogRecord, RunRecord, RunRequestedPayload,
    RunState, StateChangePayload, SwbPaths,
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct SqliteStateStore {
    path: PathBuf,
}

#[derive(Debug, Error)]
pub enum StateError {
    #[error("failed to initialize state directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("payload decode failed: {0}")]
    Payload(#[from] serde_json::Error),
    #[error("invalid run state value in database: {0}")]
    InvalidStateValue(String),
    #[error("run not found: {0}")]
    MissingRun(String),
    #[error("invalid transition for run {run_id}: {from} -> {to}")]
    InvalidTransition {
        run_id: String,
        from: RunState,
        to: RunState,
    },
}

impl SqliteStateStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, StateError> {
        let path = SwbPaths::new(root).state_db_path;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let store = Self { path };
        store.initialize()?;
        Ok(store)
    }

    pub fn apply_queue_entry(
        &self,
        entry_id: i64,
        envelope: &IngestEnvelope,
    ) -> Result<bool, StateError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let already_applied = tx
            .query_row(
                "SELECT 1 FROM applied_queue_entries WHERE entry_id = ?1",
                params![entry_id],
                |_| Ok(()),
            )
            .optional()?;

        if already_applied.is_some() {
            return Ok(false);
        }

        match envelope.kind {
            IngestKind::RunRequested => self.apply_run_requested(&tx, envelope)?,
            IngestKind::RunStarted => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Running, envelope)?
            }
            IngestKind::RunEvaluating => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Evaluating, envelope)?
            }
            IngestKind::RunAwaitingReview => {
                self.apply_transition(&tx, &envelope.run_id, RunState::AwaitingReview, envelope)?
            }
            IngestKind::RunApproved => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Approved, envelope)?
            }
            IngestKind::RunRejected => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Rejected, envelope)?
            }
            IngestKind::RunIntegrated => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Integrated, envelope)?
            }
            IngestKind::RunFailed => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Failed, envelope)?
            }
            IngestKind::RunCancelled => {
                self.apply_transition(&tx, &envelope.run_id, RunState::Cancelled, envelope)?
            }
            IngestKind::AdapterEvent => {
                self.apply_adapter_event(&tx, &envelope.run_id, envelope)?
            }
        }
        self.record_run_event(&tx, entry_id, envelope)?;

        tx.execute(
            "INSERT INTO applied_queue_entries (entry_id, applied_at) VALUES (?1, ?2)",
            params![entry_id, now_utc_rfc3339()],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, StateError> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT run_id, task_id, workflow, adapter, profile_id, persona_id, gstack_id, gstack_fingerprint, state, created_at, updated_at, prompt, last_event_kind, last_error
             FROM runs WHERE run_id = ?1",
            params![run_id],
            map_run_row,
        )
        .optional()
        .map_err(StateError::from)
    }

    pub fn list_runs(&self) -> Result<Vec<RunRecord>, StateError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT run_id, task_id, workflow, adapter, profile_id, persona_id, gstack_id, gstack_fingerprint, state, created_at, updated_at, prompt, last_event_kind, last_error
             FROM runs
             ORDER BY created_at DESC, run_id DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_run_row(row)?);
        }
        Ok(items)
    }

    pub fn list_run_logs(
        &self,
        run_id: &str,
        limit: usize,
    ) -> Result<Vec<RunLogRecord>, StateError> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT entry_id, applied_at, envelope_json
             FROM run_events
             WHERE run_id = ?1
             ORDER BY entry_id DESC
             LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![run_id, limit as i64])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let envelope_json: String = row.get(2)?;
            items.push(RunLogRecord {
                entry_id: row.get(0)?,
                applied_at: row.get(1)?,
                envelope: serde_json::from_str(&envelope_json)?,
            });
        }
        items.reverse();
        Ok(items)
    }

    pub fn upsert_external_ref(
        &self,
        draft: &ExternalRefDraft,
    ) -> Result<ExternalRefRecord, StateError> {
        let conn = self.connect()?;
        let now = now_utc_rfc3339();
        conn.execute(
            "INSERT INTO external_refs (
                system, entity_kind, external_id, task_id, run_id, persona_id, title, url, metadata_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
            ON CONFLICT(system, entity_kind, external_id) DO UPDATE SET
                task_id = excluded.task_id,
                run_id = excluded.run_id,
                persona_id = excluded.persona_id,
                title = excluded.title,
                url = excluded.url,
                metadata_json = excluded.metadata_json,
                updated_at = excluded.updated_at",
            params![
                draft.system,
                draft.entity_kind,
                draft.external_id,
                draft.task_id,
                draft.run_id,
                draft.persona_id,
                draft.title,
                draft.url,
                serde_json::to_string(&draft.metadata)?,
                now,
            ],
        )?;
        self.get_external_ref(&draft.system, &draft.entity_kind, &draft.external_id)?
            .ok_or_else(|| StateError::Sql(rusqlite::Error::QueryReturnedNoRows))
    }

    pub fn get_external_ref(
        &self,
        system: &str,
        entity_kind: &str,
        external_id: &str,
    ) -> Result<Option<ExternalRefRecord>, StateError> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT system, entity_kind, external_id, task_id, run_id, persona_id, title, url, metadata_json, created_at, updated_at
             FROM external_refs
             WHERE system = ?1 AND entity_kind = ?2 AND external_id = ?3",
            params![system, entity_kind, external_id],
            map_external_ref_row,
        )
        .optional()
        .map_err(StateError::from)
    }

    pub fn list_external_refs_for_run(
        &self,
        run_id: &str,
    ) -> Result<Vec<ExternalRefRecord>, StateError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT system, entity_kind, external_id, task_id, run_id, persona_id, title, url, metadata_json, created_at, updated_at
             FROM external_refs
             WHERE run_id = ?1
             ORDER BY system ASC, entity_kind ASC, external_id ASC",
        )?;
        let mut rows = stmt.query(params![run_id])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_external_ref_row(row)?);
        }
        Ok(items)
    }

    pub fn queue_outbound_update(
        &self,
        draft: &OutboundUpdateDraft,
    ) -> Result<OutboundUpdateRecord, StateError> {
        let conn = self.connect()?;
        let now = now_utc_rfc3339();
        conn.execute(
            "INSERT INTO outbound_updates (
                system, target_kind, target_id, task_id, run_id, status, body, metadata_json, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?8)",
            params![
                draft.system,
                draft.target_kind,
                draft.target_id,
                draft.task_id,
                draft.run_id,
                draft.body,
                serde_json::to_string(&draft.metadata)?,
                now,
            ],
        )?;
        self.get_outbound_update(conn.last_insert_rowid())
    }

    pub fn get_outbound_update(&self, id: i64) -> Result<OutboundUpdateRecord, StateError> {
        let conn = self.connect()?;
        conn.query_row(
            "SELECT id, system, target_kind, target_id, task_id, run_id, status, body, metadata_json, created_at, updated_at
             FROM outbound_updates WHERE id = ?1",
            params![id],
            map_outbound_update_row,
        )
        .map_err(StateError::from)
    }

    pub fn list_outbound_updates(
        &self,
        system: Option<&str>,
        status: Option<&str>,
        limit: usize,
    ) -> Result<Vec<OutboundUpdateRecord>, StateError> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, system, target_kind, target_id, task_id, run_id, status, body, metadata_json, created_at, updated_at
             FROM outbound_updates
             WHERE (?1 IS NULL OR system = ?1)
               AND (?2 IS NULL OR status = ?2)
             ORDER BY id DESC
             LIMIT ?3",
        )?;
        let mut rows = stmt.query(params![system, status, limit as i64])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            items.push(map_outbound_update_row(row)?);
        }
        items.reverse();
        Ok(items)
    }

    pub fn mark_outbound_update_status(
        &self,
        id: i64,
        status: &str,
    ) -> Result<OutboundUpdateRecord, StateError> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE outbound_updates SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, now_utc_rfc3339(), id],
        )?;
        self.get_outbound_update(id)
    }

    fn apply_run_requested(
        &self,
        tx: &rusqlite::Transaction<'_>,
        envelope: &IngestEnvelope,
    ) -> Result<(), StateError> {
        let payload = serde_json::from_value::<RunRequestedPayload>(envelope.payload.clone())?;
        tx.execute(
            "INSERT INTO runs (
                run_id, task_id, workflow, adapter, profile_id, persona_id, gstack_id, gstack_fingerprint, state, created_at, updated_at, prompt, last_event_kind, last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, NULL)",
            params![
                envelope.run_id,
                payload.task_id,
                payload.workflow,
                payload.adapter,
                payload.profile_id,
                payload.persona_id,
                payload.gstack_id,
                payload.gstack_fingerprint,
                RunState::Queued.to_string(),
                envelope.ts,
                envelope.ts,
                payload.prompt,
                envelope.kind.to_string(),
            ],
        )?;
        Ok(())
    }

    fn apply_transition(
        &self,
        tx: &rusqlite::Transaction<'_>,
        run_id: &str,
        next_state: RunState,
        envelope: &IngestEnvelope,
    ) -> Result<(), StateError> {
        let current = self.get_run_from_tx(tx, run_id)?;
        if !transition_allowed(current.state, next_state) {
            return Err(StateError::InvalidTransition {
                run_id: run_id.to_string(),
                from: current.state,
                to: next_state,
            });
        }

        let payload = serde_json::from_value::<StateChangePayload>(envelope.payload.clone())?;
        tx.execute(
            "UPDATE runs
             SET state = ?1, updated_at = ?2, last_event_kind = ?3, last_error = ?4
             WHERE run_id = ?5",
            params![
                next_state.to_string(),
                envelope.ts,
                envelope.kind.to_string(),
                payload.reason,
                run_id,
            ],
        )?;
        self.queue_transition_outbound_updates(
            tx,
            &current,
            next_state,
            payload.reason.as_deref(),
        )?;
        Ok(())
    }

    fn apply_adapter_event(
        &self,
        tx: &rusqlite::Transaction<'_>,
        run_id: &str,
        envelope: &IngestEnvelope,
    ) -> Result<(), StateError> {
        let _ = self.get_run_from_tx(tx, run_id)?;
        tx.execute(
            "UPDATE runs SET updated_at = ?1, last_event_kind = ?2 WHERE run_id = ?3",
            params![envelope.ts, envelope.kind.to_string(), run_id],
        )?;
        Ok(())
    }

    fn record_run_event(
        &self,
        tx: &rusqlite::Transaction<'_>,
        entry_id: i64,
        envelope: &IngestEnvelope,
    ) -> Result<(), StateError> {
        tx.execute(
            "INSERT INTO run_events (
                entry_id, run_id, ts, kind, envelope_json, applied_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry_id,
                envelope.run_id,
                envelope.ts,
                envelope.kind.to_string(),
                serde_json::to_string(envelope)?,
                now_utc_rfc3339(),
            ],
        )?;
        Ok(())
    }

    fn queue_transition_outbound_updates(
        &self,
        tx: &rusqlite::Transaction<'_>,
        run: &RunRecord,
        next_state: RunState,
        reason: Option<&str>,
    ) -> Result<(), StateError> {
        let body = match next_state {
            RunState::AwaitingReview => {
                format!("Run {} is awaiting review.", run.run_id)
            }
            RunState::Approved => format!("Run {} was approved.", run.run_id),
            RunState::Rejected => match reason.filter(|value| !value.trim().is_empty()) {
                Some(reason) => format!("Run {} was rejected: {}.", run.run_id, reason.trim()),
                None => format!("Run {} was rejected.", run.run_id),
            },
            RunState::Integrated => format!("Run {} was integrated.", run.run_id),
            RunState::Failed => match reason.filter(|value| !value.trim().is_empty()) {
                Some(reason) => format!("Run {} failed: {}.", run.run_id, reason.trim()),
                None => format!("Run {} failed.", run.run_id),
            },
            _ => return Ok(()),
        };

        let mut stmt = tx.prepare(
            "SELECT system, entity_kind, external_id, task_id, run_id
             FROM external_refs
             WHERE run_id = ?1
             ORDER BY system ASC, entity_kind ASC, external_id ASC",
        )?;
        let mut rows = stmt.query(params![&run.run_id])?;
        while let Some(row) = rows.next()? {
            let system: String = row.get(0)?;
            let target_kind: String = row.get(1)?;
            let target_id: String = row.get(2)?;
            let task_id: Option<String> = row.get(3)?;
            let run_id: Option<String> = row.get(4)?;
            tx.execute(
                "INSERT INTO outbound_updates (
                    system, target_kind, target_id, task_id, run_id, status, body, metadata_json, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', ?6, ?7, ?8, ?8)",
                params![
                    system,
                    target_kind,
                    target_id,
                    task_id,
                    run_id,
                    body,
                    serde_json::to_string(&serde_json::json!({
                        "source": "run_state",
                        "state": next_state.to_string(),
                        "run_id": run.run_id,
                    }))?,
                    now_utc_rfc3339(),
                ],
            )?;
        }
        Ok(())
    }

    fn get_run_from_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        run_id: &str,
    ) -> Result<RunRecord, StateError> {
        tx.query_row(
            "SELECT run_id, task_id, workflow, adapter, profile_id, persona_id, gstack_id, gstack_fingerprint, state, created_at, updated_at, prompt, last_event_kind, last_error
             FROM runs WHERE run_id = ?1",
            params![run_id],
            map_run_row,
        )
        .optional()?
        .ok_or_else(|| StateError::MissingRun(run_id.to_string()))
    }

    fn initialize(&self) -> Result<(), StateError> {
        let conn = self.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                run_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                workflow TEXT NOT NULL,
                adapter TEXT NOT NULL,
                profile_id TEXT,
                persona_id TEXT,
                gstack_id TEXT,
                gstack_fingerprint TEXT,
                state TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                prompt TEXT,
                last_event_kind TEXT NOT NULL,
                last_error TEXT
            );
            CREATE TABLE IF NOT EXISTS applied_queue_entries (
                entry_id INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS run_events (
                entry_id INTEGER PRIMARY KEY,
                run_id TEXT NOT NULL,
                ts TEXT NOT NULL,
                kind TEXT NOT NULL,
                envelope_json TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS run_events_run_id_entry_idx
            ON run_events(run_id, entry_id DESC);
            CREATE TABLE IF NOT EXISTS external_refs (
                system TEXT NOT NULL,
                entity_kind TEXT NOT NULL,
                external_id TEXT NOT NULL,
                task_id TEXT,
                run_id TEXT,
                persona_id TEXT,
                title TEXT,
                url TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY(system, entity_kind, external_id)
            );
            CREATE INDEX IF NOT EXISTS external_refs_run_id_idx
            ON external_refs(run_id, system, entity_kind);
            CREATE TABLE IF NOT EXISTS outbound_updates (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                system TEXT NOT NULL,
                target_kind TEXT NOT NULL,
                target_id TEXT NOT NULL,
                task_id TEXT,
                run_id TEXT,
                status TEXT NOT NULL,
                body TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS outbound_updates_status_idx
            ON outbound_updates(status, id DESC);",
        )?;
        ensure_runs_column(&conn, "profile_id", "TEXT")?;
        ensure_runs_column(&conn, "persona_id", "TEXT")?;
        ensure_runs_column(&conn, "gstack_id", "TEXT")?;
        ensure_runs_column(&conn, "gstack_fingerprint", "TEXT")?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection, StateError> {
        Ok(Connection::open(&self.path)?)
    }
}

fn map_run_row(row: &rusqlite::Row<'_>) -> Result<RunRecord, rusqlite::Error> {
    let state_raw: String = row.get(8)?;
    let state = RunState::from_str(&state_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(RunRecord {
        run_id: row.get(0)?,
        task_id: row.get(1)?,
        workflow: row.get(2)?,
        adapter: row.get(3)?,
        profile_id: row.get(4)?,
        persona_id: row.get(5)?,
        gstack_id: row.get(6)?,
        gstack_fingerprint: row.get(7)?,
        state,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        prompt: row.get(11)?,
        last_event_kind: row.get(12)?,
        last_error: row.get(13)?,
    })
}

fn map_external_ref_row(row: &rusqlite::Row<'_>) -> Result<ExternalRefRecord, rusqlite::Error> {
    Ok(ExternalRefRecord {
        system: row.get(0)?,
        entity_kind: row.get(1)?,
        external_id: row.get(2)?,
        task_id: row.get(3)?,
        run_id: row.get(4)?,
        persona_id: row.get(5)?,
        title: row.get(6)?,
        url: row.get(7)?,
        metadata: parse_json_column(row.get(8)?)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn map_outbound_update_row(
    row: &rusqlite::Row<'_>,
) -> Result<OutboundUpdateRecord, rusqlite::Error> {
    Ok(OutboundUpdateRecord {
        id: row.get(0)?,
        system: row.get(1)?,
        target_kind: row.get(2)?,
        target_id: row.get(3)?,
        task_id: row.get(4)?,
        run_id: row.get(5)?,
        status: row.get(6)?,
        body: row.get(7)?,
        metadata: parse_json_column(row.get(8)?)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn parse_json_column(raw: String) -> Result<Value, rusqlite::Error> {
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
    })
}

fn ensure_runs_column(
    conn: &Connection,
    column_name: &str,
    column_sql: &str,
) -> Result<(), StateError> {
    let mut stmt = conn.prepare("PRAGMA table_info(runs)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let current_name: String = row.get(1)?;
        if current_name == column_name {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE runs ADD COLUMN {column_name} {column_sql}"),
        [],
    )?;
    Ok(())
}

fn transition_allowed(current: RunState, next: RunState) -> bool {
    matches!(
        (current, next),
        (RunState::Queued, RunState::Running)
            | (RunState::Queued, RunState::AwaitingReview)
            | (RunState::Queued, RunState::Failed)
            | (RunState::Queued, RunState::Cancelled)
            | (RunState::Running, RunState::Evaluating)
            | (RunState::Running, RunState::AwaitingReview)
            | (RunState::Running, RunState::Failed)
            | (RunState::Running, RunState::Cancelled)
            | (RunState::Evaluating, RunState::AwaitingReview)
            | (RunState::Evaluating, RunState::Failed)
            | (RunState::AwaitingReview, RunState::Approved)
            | (RunState::AwaitingReview, RunState::Rejected)
            | (RunState::Approved, RunState::Integrated)
            | (RunState::Approved, RunState::Archived)
    )
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::json;
    use swb_core::{
        now_utc_rfc3339, AdapterEventPayload, ExternalRefDraft, IngestEnvelope, IngestKind,
        OutboundUpdateDraft, RunRequest, RunState,
    };

    use super::SqliteStateStore;

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-state-test-{serial}"))
    }

    #[test]
    fn projected_run_request_creates_queued_run() {
        let root = unique_temp_root();
        let store = SqliteStateStore::open(&root).unwrap();
        let request = RunRequest::new("TASK-1", "default", "codex", None);
        let envelope = IngestEnvelope::run_requested(&request);

        let applied = store.apply_queue_entry(1, &envelope).unwrap();
        assert!(applied);

        let run = store.get_run(&request.run_id).unwrap().unwrap();
        assert_eq!(run.state, RunState::Queued);
        assert!(run.profile_id.is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approval_flow_requires_awaiting_review() {
        let root = unique_temp_root();
        let store = SqliteStateStore::open(&root).unwrap();
        let request = RunRequest::new("TASK-1", "default", "codex", None);
        store
            .apply_queue_entry(1, &IngestEnvelope::run_requested(&request))
            .unwrap();
        store
            .apply_queue_entry(
                2,
                &IngestEnvelope::state_change(
                    request.run_id.clone(),
                    IngestKind::RunAwaitingReview,
                    None,
                ),
            )
            .unwrap();
        store
            .apply_queue_entry(
                3,
                &IngestEnvelope::state_change(
                    request.run_id.clone(),
                    IngestKind::RunApproved,
                    None,
                ),
            )
            .unwrap();

        let run = store.get_run(&request.run_id).unwrap().unwrap();
        assert_eq!(run.state, RunState::Approved);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn projected_run_request_stores_profile_and_gstack_metadata() {
        let root = unique_temp_root();
        let store = SqliteStateStore::open(&root).unwrap();
        let request = RunRequest::with_gstack(
            "TASK-PROFILE",
            "default",
            "codex",
            Some("review the changes".to_string()),
            Some("eng-review".to_string()),
            Some("slack-review".to_string()),
            Some("profile.eng-review".to_string()),
            Some("sha256:test".to_string()),
        );
        store
            .apply_queue_entry(1, &IngestEnvelope::run_requested(&request))
            .unwrap();

        let run = store.get_run(&request.run_id).unwrap().unwrap();
        assert_eq!(run.profile_id.as_deref(), Some("eng-review"));
        assert_eq!(run.persona_id.as_deref(), Some("slack-review"));
        assert_eq!(run.gstack_id.as_deref(), Some("profile.eng-review"));
        assert_eq!(run.gstack_fingerprint.as_deref(), Some("sha256:test"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn applied_events_are_queryable_as_run_logs() {
        let root = unique_temp_root();
        let store = SqliteStateStore::open(&root).unwrap();
        let request = RunRequest::new("TASK-2", "default", "shell", Some("hello".to_string()));
        store
            .apply_queue_entry(1, &IngestEnvelope::run_requested(&request))
            .unwrap();
        store
            .apply_queue_entry(
                2,
                &IngestEnvelope::state_change(request.run_id.clone(), IngestKind::RunStarted, None),
            )
            .unwrap();
        store
            .apply_queue_entry(
                3,
                &IngestEnvelope {
                    run_id: request.run_id.clone(),
                    ts: now_utc_rfc3339(),
                    kind: IngestKind::AdapterEvent,
                    payload: json!(AdapterEventPayload {
                        step_id: "primary".to_string(),
                        adapter: "shell".to_string(),
                        event_kind: "command_completed".to_string(),
                        payload: json!({
                            "success": true,
                            "exit_code": 0,
                        }),
                    }),
                },
            )
            .unwrap();

        let logs = store.list_run_logs(&request.run_id, 10).unwrap();
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0].entry_id, 1);
        assert_eq!(logs[0].envelope.kind, IngestKind::RunRequested);
        assert_eq!(logs[1].entry_id, 2);
        assert_eq!(logs[1].envelope.kind, IngestKind::RunStarted);
        assert_eq!(logs[2].entry_id, 3);
        assert_eq!(logs[2].envelope.kind, IngestKind::AdapterEvent);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_refs_and_outbound_updates_round_trip() {
        let root = unique_temp_root();
        let store = SqliteStateStore::open(&root).unwrap();

        let external = store
            .upsert_external_ref(&ExternalRefDraft {
                system: "slack".to_string(),
                entity_kind: "channel".to_string(),
                external_id: "T1/C1".to_string(),
                task_id: Some("TASK-1".to_string()),
                run_id: Some("run-1".to_string()),
                persona_id: Some("slack-review".to_string()),
                title: Some("Bug intake".to_string()),
                url: Some("https://example.test/slack/T1/C1".to_string()),
                metadata: json!({ "thread_ts": "12345.6789" }),
            })
            .unwrap();
        assert_eq!(external.persona_id.as_deref(), Some("slack-review"));

        let queued = store
            .queue_outbound_update(&OutboundUpdateDraft {
                system: "slack".to_string(),
                target_kind: "channel".to_string(),
                target_id: "T1/C1".to_string(),
                task_id: Some("TASK-1".to_string()),
                run_id: Some("run-1".to_string()),
                body: "Queued run run-1".to_string(),
                metadata: json!({ "kind": "queued" }),
            })
            .unwrap();
        assert_eq!(queued.status, "pending");

        let listed = store
            .list_outbound_updates(Some("slack"), Some("pending"), 10)
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].body, "Queued run run-1");

        let updated = store
            .mark_outbound_update_status(queued.id, "sent")
            .unwrap();
        assert_eq!(updated.status, "sent");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn state_transitions_queue_simple_outbound_updates_for_linked_external_refs() {
        let root = unique_temp_root();
        let store = SqliteStateStore::open(&root).unwrap();
        let request = RunRequest::new("TASK-3", "default", "codex", None);
        store
            .apply_queue_entry(1, &IngestEnvelope::run_requested(&request))
            .unwrap();
        store
            .upsert_external_ref(&ExternalRefDraft {
                system: "linear".to_string(),
                entity_kind: "issue".to_string(),
                external_id: "ABC-123".to_string(),
                task_id: Some("TASK-3".to_string()),
                run_id: Some(request.run_id.clone()),
                persona_id: None,
                title: Some("ABC-123".to_string()),
                url: None,
                metadata: json!({}),
            })
            .unwrap();
        store
            .apply_queue_entry(
                2,
                &IngestEnvelope::state_change(
                    request.run_id.clone(),
                    IngestKind::RunAwaitingReview,
                    None,
                ),
            )
            .unwrap();

        let updates = store
            .list_outbound_updates(Some("linear"), Some("pending"), 10)
            .unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].body.contains("awaiting review"));

        fs::remove_dir_all(root).unwrap();
    }
}
