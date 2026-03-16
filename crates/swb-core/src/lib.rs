use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const SWB_CONFIG_FILE: &str = "swb.toml";
pub const SWB_V2_DIR: &str = ".swb/v2";
pub const SWB_V2_QUEUE_DB_FILE: &str = "ingest.sqlite3";
pub const SWB_V2_STATE_DB_FILE: &str = "state.sqlite3";

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwbPaths {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub queue_db_path: PathBuf,
    pub state_db_path: PathBuf,
}

impl SwbPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let data_dir = root.join(SWB_V2_DIR);
        Self {
            config_path: root.join(SWB_CONFIG_FILE),
            queue_db_path: data_dir.join(SWB_V2_QUEUE_DB_FILE),
            state_db_path: data_dir.join(SWB_V2_STATE_DB_FILE),
            data_dir,
            root,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Draft,
    Queued,
    Running,
    Evaluating,
    AwaitingReview,
    Approved,
    Rejected,
    Integrated,
    Archived,
    Failed,
    Cancelled,
}

impl Display for RunState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Draft => "draft",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Evaluating => "evaluating",
            Self::AwaitingReview => "awaiting_review",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Integrated => "integrated",
            Self::Archived => "archived",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Error)]
#[error("invalid run state: {value}")]
pub struct ParseRunStateError {
    value: String,
}

impl FromStr for RunState {
    type Err = ParseRunStateError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "evaluating" => Ok(Self::Evaluating),
            "awaiting_review" => Ok(Self::AwaitingReview),
            "approved" => Ok(Self::Approved),
            "rejected" => Ok(Self::Rejected),
            "integrated" => Ok(Self::Integrated),
            "archived" => Ok(Self::Archived),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(ParseRunStateError {
                value: other.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestKind {
    RunRequested,
    RunStarted,
    RunEvaluating,
    RunAwaitingReview,
    RunApproved,
    RunRejected,
    RunIntegrated,
    RunFailed,
    RunCancelled,
    AdapterEvent,
}

impl Display for IngestKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::RunRequested => "run_requested",
            Self::RunStarted => "run_started",
            Self::RunEvaluating => "run_evaluating",
            Self::RunAwaitingReview => "run_awaiting_review",
            Self::RunApproved => "run_approved",
            Self::RunRejected => "run_rejected",
            Self::RunIntegrated => "run_integrated",
            Self::RunFailed => "run_failed",
            Self::RunCancelled => "run_cancelled",
            Self::AdapterEvent => "adapter_event",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequest {
    pub run_id: String,
    pub task_id: String,
    pub workflow: String,
    pub adapter: String,
    pub prompt: Option<String>,
}

impl RunRequest {
    pub fn new(
        task_id: impl Into<String>,
        workflow: impl Into<String>,
        adapter: impl Into<String>,
        prompt: Option<String>,
    ) -> Self {
        let task_id = task_id.into();
        Self {
            run_id: generate_run_id(&task_id),
            task_id,
            workflow: workflow.into(),
            adapter: adapter.into(),
            prompt,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRequestedPayload {
    pub task_id: String,
    pub workflow: String,
    pub adapter: String,
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateChangePayload {
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterEventPayload {
    pub step_id: String,
    pub adapter: String,
    pub event_kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IngestEnvelope {
    pub run_id: String,
    pub ts: String,
    pub kind: IngestKind,
    pub payload: Value,
}

impl IngestEnvelope {
    pub fn run_requested(request: &RunRequest) -> Self {
        Self {
            run_id: request.run_id.clone(),
            ts: now_utc_rfc3339(),
            kind: IngestKind::RunRequested,
            payload: json!(RunRequestedPayload {
                task_id: request.task_id.clone(),
                workflow: request.workflow.clone(),
                adapter: request.adapter.clone(),
                prompt: request.prompt.clone(),
            }),
        }
    }

    pub fn state_change(
        run_id: impl Into<String>,
        kind: IngestKind,
        reason: Option<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            ts: now_utc_rfc3339(),
            kind,
            payload: json!(StateChangePayload { reason }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunRecord {
    pub run_id: String,
    pub task_id: String,
    pub workflow: String,
    pub adapter: String,
    pub state: RunState,
    pub created_at: String,
    pub updated_at: String,
    pub prompt: Option<String>,
    pub last_event_kind: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunLogRecord {
    pub entry_id: i64,
    pub applied_at: String,
    pub envelope: IngestEnvelope,
}

pub fn now_utc_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .expect("rfc3339 formatting should succeed")
}

pub fn generate_run_id(task_id: &str) -> String {
    let serial = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let slug = slugify(task_id);
    let timestamp = OffsetDateTime::now_utc().unix_timestamp();
    format!("run-{slug}-{timestamp}-{serial}")
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;

    for ch in value.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '-'
        };

        if normalized == '-' {
            if !last_dash && !slug.is_empty() {
                slug.push('-');
                last_dash = true;
            }
            continue;
        }

        slug.push(normalized);
        last_dash = false;
    }

    slug.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::{generate_run_id, IngestEnvelope, IngestKind, RunRequest, RunState, SwbPaths};

    #[test]
    fn swb_paths_use_v2_directory() {
        let paths = SwbPaths::new("/tmp/swb-root");
        assert!(paths.queue_db_path.ends_with(".swb/v2/ingest.sqlite3"));
        assert!(paths.state_db_path.ends_with(".swb/v2/state.sqlite3"));
        assert!(paths.config_path.ends_with("swb.toml"));
    }

    #[test]
    fn run_state_round_trip_strings() {
        assert_eq!("queued".parse::<RunState>().unwrap(), RunState::Queued);
        assert_eq!(RunState::AwaitingReview.to_string(), "awaiting_review");
    }

    #[test]
    fn generated_run_id_is_stable_and_slugged() {
        let run_id = generate_run_id("TASK Alpha/42");
        assert!(run_id.starts_with("run-task-alpha-42-"));
    }

    #[test]
    fn run_requested_envelope_uses_request_identity() {
        let request = RunRequest::new("TASK-1", "default", "codex", Some("hi".to_string()));
        let envelope = IngestEnvelope::run_requested(&request);
        assert_eq!(envelope.run_id, request.run_id);
        assert_eq!(envelope.kind, IngestKind::RunRequested);
    }
}
