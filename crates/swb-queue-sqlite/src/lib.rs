use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use swb_core::{now_utc_rfc3339, IngestEnvelope, SwbPaths};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct SqliteIngestQueue {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct QueuedEnvelope {
    pub id: i64,
    pub inserted_at: String,
    pub processed_at: Option<String>,
    pub envelope: IngestEnvelope,
}

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("failed to initialize queue directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlite error: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("failed to serialize envelope: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl SqliteIngestQueue {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, QueueError> {
        let path = SwbPaths::new(root).queue_db_path;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let queue = Self { path };
        queue.initialize()?;
        Ok(queue)
    }

    pub fn enqueue(&self, envelope: &IngestEnvelope) -> Result<QueuedEnvelope, QueueError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;
        let inserted_at = now_utc_rfc3339();
        let encoded = serde_json::to_string(envelope)?;
        tx.execute(
            "INSERT INTO ingest_queue (run_id, kind, envelope_json, inserted_at) VALUES (?1, ?2, ?3, ?4)",
            params![envelope.run_id, envelope.kind.to_string(), encoded, inserted_at],
        )?;
        let id = tx.last_insert_rowid();
        tx.commit()?;

        Ok(QueuedEnvelope {
            id,
            inserted_at,
            processed_at: None,
            envelope: envelope.clone(),
        })
    }

    pub fn pending(&self) -> Result<Vec<QueuedEnvelope>, QueueError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT id, inserted_at, processed_at, envelope_json
             FROM ingest_queue
             WHERE processed_at IS NULL
             ORDER BY id ASC",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();

        while let Some(row) = rows.next()? {
            let encoded: String = row.get(3)?;
            let envelope = serde_json::from_str::<IngestEnvelope>(&encoded)?;
            items.push(QueuedEnvelope {
                id: row.get(0)?,
                inserted_at: row.get(1)?,
                processed_at: row.get(2)?,
                envelope,
            });
        }

        Ok(items)
    }

    pub fn mark_processed(&self, entry_id: i64) -> Result<(), QueueError> {
        let conn = self.connect()?;
        conn.execute(
            "UPDATE ingest_queue SET processed_at = ?1 WHERE id = ?2",
            params![now_utc_rfc3339(), entry_id],
        )?;
        Ok(())
    }

    pub fn pending_count(&self) -> Result<u64, QueueError> {
        let conn = self.connect()?;
        let count = conn.query_row(
            "SELECT COUNT(*) FROM ingest_queue WHERE processed_at IS NULL",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        Ok(count)
    }

    fn initialize(&self) -> Result<(), QueueError> {
        let conn = self.connect()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ingest_queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                run_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                envelope_json TEXT NOT NULL,
                inserted_at TEXT NOT NULL,
                processed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS ingest_queue_pending_idx
            ON ingest_queue(processed_at, id);",
        )?;
        Ok(())
    }

    fn connect(&self) -> Result<Connection, QueueError> {
        Ok(Connection::open(&self.path)?)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use swb_core::{IngestEnvelope, RunRequest};

    use super::SqliteIngestQueue;

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-queue-test-{serial}"))
    }

    #[test]
    fn enqueue_and_read_pending_envelopes() {
        let root = unique_temp_root();
        let queue = SqliteIngestQueue::open(&root).unwrap();
        let request = RunRequest::new("TASK-1", "default", "codex", None);
        let envelope = IngestEnvelope::run_requested(&request);

        let queued = queue.enqueue(&envelope).unwrap();
        assert!(queued.id > 0);
        assert_eq!(queue.pending_count().unwrap(), 1);

        let pending = queue.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].envelope.run_id, request.run_id);

        fs::remove_dir_all(root).unwrap();
    }
}
