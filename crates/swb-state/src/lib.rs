use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;
use swb_core::{
    now_utc_rfc3339, IngestEnvelope, IngestKind, RunLogRecord, RunRecord, RunRequestedPayload,
    RunState, StateChangePayload, SwbPaths,
};

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
            "SELECT run_id, task_id, workflow, adapter, state, created_at, updated_at, prompt, last_event_kind, last_error
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
            "SELECT run_id, task_id, workflow, adapter, state, created_at, updated_at, prompt, last_event_kind, last_error
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

    fn apply_run_requested(
        &self,
        tx: &rusqlite::Transaction<'_>,
        envelope: &IngestEnvelope,
    ) -> Result<(), StateError> {
        let payload = serde_json::from_value::<RunRequestedPayload>(envelope.payload.clone())?;
        tx.execute(
            "INSERT INTO runs (
                run_id, task_id, workflow, adapter, state, created_at, updated_at, prompt, last_event_kind, last_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
            params![
                envelope.run_id,
                payload.task_id,
                payload.workflow,
                payload.adapter,
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

    fn get_run_from_tx(
        &self,
        tx: &rusqlite::Transaction<'_>,
        run_id: &str,
    ) -> Result<RunRecord, StateError> {
        tx.query_row(
            "SELECT run_id, task_id, workflow, adapter, state, created_at, updated_at, prompt, last_event_kind, last_error
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
            ON run_events(run_id, entry_id DESC);",
        )?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection, StateError> {
        Ok(Connection::open(&self.path)?)
    }
}

fn map_run_row(row: &rusqlite::Row<'_>) -> Result<RunRecord, rusqlite::Error> {
    let state_raw: String = row.get(4)?;
    let state = RunState::from_str(&state_raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(RunRecord {
        run_id: row.get(0)?,
        task_id: row.get(1)?,
        workflow: row.get(2)?,
        adapter: row.get(3)?,
        state,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        prompt: row.get(7)?,
        last_event_kind: row.get(8)?,
        last_error: row.get(9)?,
    })
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
        now_utc_rfc3339, AdapterEventPayload, IngestEnvelope, IngestKind, RunRequest, RunState,
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
}
