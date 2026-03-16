use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub const SWB_CONFIG_FILE: &str = "swb.toml";
pub const SWB_DATA_DIR: &str = ".swb";
pub const SWB_QUEUE_DB_FILE: &str = "ingest.sqlite3";
pub const SWB_STATE_DB_FILE: &str = "state.sqlite3";
pub const SWB_ASSETS_DIR: &str = "swb";
pub const SWB_PROFILES_DIR: &str = "profiles";
pub const SWB_PERSONAS_DIR: &str = "personas";
pub const SWB_PROMPTS_DIR: &str = "prompts";
pub const SWB_RUNTIME_PROMPTS_DIR: &str = "runtime";

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwbPaths {
    pub root: PathBuf,
    pub config_path: PathBuf,
    pub data_dir: PathBuf,
    pub queue_db_path: PathBuf,
    pub state_db_path: PathBuf,
    pub assets_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub personas_dir: PathBuf,
    pub prompts_dir: PathBuf,
    pub runtime_prompts_dir: PathBuf,
}

impl SwbPaths {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let data_dir = root.join(SWB_DATA_DIR);
        let assets_dir = root.join(SWB_ASSETS_DIR);
        let prompts_dir = assets_dir.join(SWB_PROMPTS_DIR);
        Self {
            config_path: root.join(SWB_CONFIG_FILE),
            queue_db_path: data_dir.join(SWB_QUEUE_DB_FILE),
            state_db_path: data_dir.join(SWB_STATE_DB_FILE),
            assets_dir: assets_dir.clone(),
            profiles_dir: assets_dir.join(SWB_PROFILES_DIR),
            personas_dir: assets_dir.join(SWB_PERSONAS_DIR),
            prompts_dir: prompts_dir.clone(),
            runtime_prompts_dir: prompts_dir.join(SWB_RUNTIME_PROMPTS_DIR),
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
    pub profile_id: Option<String>,
    pub persona_id: Option<String>,
    pub gstack_id: Option<String>,
    pub gstack_fingerprint: Option<String>,
}

impl RunRequest {
    pub fn new(
        task_id: impl Into<String>,
        workflow: impl Into<String>,
        adapter: impl Into<String>,
        prompt: Option<String>,
    ) -> Self {
        Self::with_gstack(task_id, workflow, adapter, prompt, None, None, None, None)
    }

    pub fn with_gstack(
        task_id: impl Into<String>,
        workflow: impl Into<String>,
        adapter: impl Into<String>,
        prompt: Option<String>,
        profile_id: Option<String>,
        persona_id: Option<String>,
        gstack_id: Option<String>,
        gstack_fingerprint: Option<String>,
    ) -> Self {
        let task_id = task_id.into();
        Self {
            run_id: generate_run_id(&task_id),
            task_id,
            workflow: workflow.into(),
            adapter: adapter.into(),
            prompt,
            profile_id,
            persona_id,
            gstack_id,
            gstack_fingerprint,
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
    pub profile_id: Option<String>,
    pub persona_id: Option<String>,
    pub gstack_id: Option<String>,
    pub gstack_fingerprint: Option<String>,
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
                profile_id: request.profile_id.clone(),
                persona_id: request.persona_id.clone(),
                gstack_id: request.gstack_id.clone(),
                gstack_fingerprint: request.gstack_fingerprint.clone(),
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
    pub profile_id: Option<String>,
    pub persona_id: Option<String>,
    pub gstack_id: Option<String>,
    pub gstack_fingerprint: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRefDraft {
    pub system: String,
    pub entity_kind: String,
    pub external_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub persona_id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRefRecord {
    pub system: String,
    pub entity_kind: String,
    pub external_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub persona_id: Option<String>,
    pub title: Option<String>,
    pub url: Option<String>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundUpdateDraft {
    pub system: String,
    pub target_kind: String,
    pub target_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub body: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutboundUpdateRecord {
    pub id: i64,
    pub system: String,
    pub target_kind: String,
    pub target_id: String,
    pub task_id: Option<String>,
    pub run_id: Option<String>,
    pub status: String,
    pub body: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
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
    fn swb_paths_use_data_directory() {
        let paths = SwbPaths::new("/tmp/swb-root");
        assert!(paths.queue_db_path.ends_with(".swb/ingest.sqlite3"));
        assert!(paths.state_db_path.ends_with(".swb/state.sqlite3"));
        assert!(paths.config_path.ends_with("swb.toml"));
        assert!(paths.profiles_dir.ends_with("swb/profiles"));
        assert!(paths.personas_dir.ends_with("swb/personas"));
        assert!(paths.runtime_prompts_dir.ends_with("swb/prompts/runtime"));
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
        let request = RunRequest::with_gstack(
            "TASK-1",
            "default",
            "codex",
            Some("hi".to_string()),
            Some("eng-review".to_string()),
            Some("slack-review".to_string()),
            Some("profile.eng-review".to_string()),
            Some("sha256:test".to_string()),
        );
        let envelope = IngestEnvelope::run_requested(&request);
        assert_eq!(envelope.run_id, request.run_id);
        assert_eq!(envelope.kind, IngestKind::RunRequested);
        assert_eq!(envelope.payload["profile_id"], "eng-review");
        assert_eq!(envelope.payload["persona_id"], "slack-review");
    }
}
