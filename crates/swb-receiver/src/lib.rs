use std::path::Path;

use thiserror::Error;
use swb_queue_sqlite::{QueueError, SqliteIngestQueue};
use swb_state::{SqliteStateStore, StateError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrainReport {
    pub processed: usize,
    pub skipped: usize,
}

#[derive(Debug)]
pub struct Receiver {
    queue: SqliteIngestQueue,
    state: SqliteStateStore,
}

#[derive(Debug, Error)]
pub enum ReceiverError {
    #[error("queue error: {0}")]
    Queue(#[from] QueueError),
    #[error("state error: {0}")]
    State(#[from] StateError),
}

impl Receiver {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, ReceiverError> {
        Ok(Self {
            queue: SqliteIngestQueue::open(&root)?,
            state: SqliteStateStore::open(root)?,
        })
    }

    pub fn drain_pending(&self) -> Result<DrainReport, ReceiverError> {
        let pending = self.queue.pending()?;
        let mut report = DrainReport {
            processed: 0,
            skipped: 0,
        };

        for entry in pending {
            let applied = self.state.apply_queue_entry(entry.id, &entry.envelope)?;
            self.queue.mark_processed(entry.id)?;
            if applied {
                report.processed += 1;
            } else {
                report.skipped += 1;
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use swb_core::{IngestEnvelope, RunRequest};
    use swb_queue_sqlite::SqliteIngestQueue;
    use swb_state::SqliteStateStore;

    use super::Receiver;

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-receiver-test-{serial}"))
    }

    #[test]
    fn receiver_drains_queue_into_state() {
        let root = unique_temp_root();
        let queue = SqliteIngestQueue::open(&root).unwrap();
        let request = RunRequest::new("TASK-1", "default", "codex", None);
        queue
            .enqueue(&IngestEnvelope::run_requested(&request))
            .unwrap();

        let receiver = Receiver::open(&root).unwrap();
        let report = receiver.drain_pending().unwrap();
        assert_eq!(report.processed, 1);

        let state = SqliteStateStore::open(&root).unwrap();
        assert!(state.get_run(&request.run_id).unwrap().is_some());

        fs::remove_dir_all(root).unwrap();
    }
}
