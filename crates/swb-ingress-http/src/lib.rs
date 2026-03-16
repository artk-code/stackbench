use std::collections::VecDeque;
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::body::Bytes;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tracing::info;

use swb_config::{get_persona_from_root, list_personas_from_root, SwbConfig};
use swb_core::{ExternalRefDraft, OutboundUpdateDraft, RunRecord};
use swb_queue_sqlite::SqliteIngestQueue;
use swb_receiver::Receiver;
use swb_state::SqliteStateStore;

type HmacSha256 = Hmac<Sha256>;

const SLACK_SIGNATURE_HEADER: &str = "x-slack-signature";
const SLACK_TIMESTAMP_HEADER: &str = "x-slack-request-timestamp";
const LINEAR_SIGNATURE_HEADER: &str = "linear-signature";
const SLACK_SIGNING_SECRET_ENV: &str = "SWB_SLACK_SIGNING_SECRET";
const LINEAR_WEBHOOK_SECRET_ENV: &str = "SWB_LINEAR_WEBHOOK_SECRET";
const SLACK_ALLOWED_SKEW_SECS: i64 = 60 * 5;

#[derive(Debug, Clone)]
pub struct IngressAppState {
    pub root: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub listen_addr: String,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8787".to_string(),
        }
    }
}

#[derive(Debug, Error)]
pub enum IngressError {
    #[error("config error: {0}")]
    Config(#[from] swb_config::ConfigError),
    #[error("queue error: {0}")]
    Queue(#[from] swb_queue_sqlite::QueueError),
    #[error("receiver error: {0}")]
    Receiver(#[from] swb_receiver::ReceiverError),
    #[error("state error: {0}")]
    State(#[from] swb_state::StateError),
    #[error("server error: {0}")]
    Server(#[from] std::io::Error),
    #[error("address parse error: {0}")]
    AddressParse(#[from] std::net::AddrParseError),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct IngressRunSpec {
    task_id: Option<String>,
    persona_id: Option<String>,
    profile_id: Option<String>,
    workflow: Option<String>,
    adapter: Option<String>,
    prompt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct IngressDispatchResult {
    pub run: RunRecord,
    pub queued_update_id: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SlackCommandForm {
    team_id: String,
    team_domain: Option<String>,
    channel_id: String,
    channel_name: Option<String>,
    user_id: String,
    user_name: Option<String>,
    command: Option<String>,
    text: Option<String>,
    response_url: Option<String>,
    trigger_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackPayloadEnvelope {
    payload: String,
}

#[derive(Debug, Deserialize)]
struct SlackActionPayload {
    #[serde(rename = "type")]
    payload_type: String,
    team: Option<SlackTeam>,
    user: Option<SlackUser>,
    channel: Option<SlackChannel>,
    message: Option<SlackMessage>,
    response_url: Option<String>,
    trigger_id: Option<String>,
    actions: Vec<SlackAction>,
}

#[derive(Debug, Deserialize)]
struct SlackTeam {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SlackUser {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SlackChannel {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SlackMessage {
    ts: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlackAction {
    action_id: String,
    value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LinearWebhookEnvelope {
    action: Option<String>,
    #[serde(rename = "type")]
    entity_type: Option<String>,
    data: Value,
    url: Option<String>,
    #[serde(rename = "webhookTimestamp")]
    webhook_timestamp: Option<i64>,
}

pub fn app(root: impl AsRef<Path>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ingress/slack/command", post(slack_command))
        .route("/ingress/slack/action", post(slack_action))
        .route("/ingress/linear/webhook", post(linear_webhook))
        .with_state(IngressAppState {
            root: root.as_ref().to_path_buf(),
        })
}

pub async fn serve(root: impl AsRef<Path>, options: ServerOptions) -> Result<(), IngressError> {
    let address: SocketAddr = options.listen_addr.parse()?;
    let listener = TcpListener::bind(address).await?;
    info!("stackbench ingress listening on {}", listener.local_addr()?);
    axum::serve(listener, app(root)).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn slack_command(
    State(state): State<IngressAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, IngressHttpError> {
    verify_slack_request(&headers, &body)?;
    let form = parse_form::<SlackCommandForm>(&body)?;
    let spec = parse_ingress_text(form.text.as_deref().unwrap_or_default())?;
    let task_id = spec
        .task_id
        .unwrap_or_else(|| generate_slack_task_id(&form.team_id, &form.channel_id));
    let persona_id = select_persona(&state.root, "slack", spec.persona_id.as_deref())?;
    let dispatch = enqueue_run(
        &state.root,
        &task_id,
        persona_id.as_deref(),
        spec.profile_id.as_deref(),
        spec.workflow.as_deref(),
        spec.adapter.as_deref(),
        spec.prompt,
        Some(ExternalRefDraft {
            system: "slack".to_string(),
            entity_kind: "command".to_string(),
            external_id: slack_command_external_id(&form),
            task_id: Some(task_id.clone()),
            run_id: None,
            persona_id: persona_id.clone(),
            title: form.command.clone(),
            url: None,
            metadata: json!({
                "team_id": form.team_id,
                "team_domain": form.team_domain,
                "channel_id": form.channel_id,
                "channel_name": form.channel_name,
                "user_id": form.user_id,
                "user_name": form.user_name,
                "response_url": form.response_url,
                "trigger_id": form.trigger_id,
            }),
        }),
        form.response_url
            .as_ref()
            .map(|response_url| OutboundUpdateDraft {
                system: "slack".to_string(),
                target_kind: "response_url".to_string(),
                target_id: response_url.clone(),
                task_id: Some(task_id.clone()),
                run_id: None,
                body: String::new(),
                metadata: json!({}),
            }),
    )?;

    Ok(slack_json_response(json!({
        "response_type": "ephemeral",
        "text": format!(
            "Queued run {} for {} using {}.",
            dispatch.run.run_id,
            dispatch.run.task_id,
            dispatch
                .run
                .persona_id
                .as_deref()
                .or(dispatch.run.profile_id.as_deref())
                .unwrap_or("default")
        ),
    })))
}

async fn slack_action(
    State(state): State<IngressAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, IngressHttpError> {
    verify_slack_request(&headers, &body)?;
    let form = parse_form::<SlackPayloadEnvelope>(&body)?;
    let payload = serde_json::from_str::<SlackActionPayload>(&form.payload)?;
    let response_url = payload.response_url.clone();
    let action = payload.actions.first().ok_or_else(|| {
        IngressHttpError::BadRequest("slack action payload is missing actions".to_string())
    })?;

    match action.action_id.as_str() {
        "dispatch_run" => {
            let spec = parse_action_value(action.value.as_deref().unwrap_or_default())?;
            let channel_id = payload
                .channel
                .as_ref()
                .map(|channel| channel.id.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let team_id = payload
                .team
                .as_ref()
                .map(|team| team.id.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let task_id = spec
                .task_id
                .unwrap_or_else(|| generate_slack_task_id(&team_id, &channel_id));
            let persona_id = select_persona(&state.root, "slack", spec.persona_id.as_deref())?;
            let message_ts = payload
                .message
                .as_ref()
                .and_then(|message| message.ts.clone())
                .unwrap_or_else(|| "message".to_string());
            let dispatch = enqueue_run(
                &state.root,
                &task_id,
                persona_id.as_deref(),
                spec.profile_id.as_deref(),
                spec.workflow.as_deref(),
                spec.adapter.as_deref(),
                spec.prompt,
                Some(ExternalRefDraft {
                    system: "slack".to_string(),
                    entity_kind: "message".to_string(),
                    external_id: format!("{team_id}/{channel_id}/{message_ts}"),
                    task_id: Some(task_id.clone()),
                    run_id: None,
                    persona_id: persona_id.clone(),
                    title: Some(payload.payload_type.clone()),
                    url: None,
                    metadata: json!({
                        "team_id": team_id,
                        "channel_id": channel_id,
                        "response_url": response_url,
                        "trigger_id": payload.trigger_id,
                        "user_id": payload.user.as_ref().map(|user| user.id.clone()),
                    }),
                }),
                response_url.as_ref().map(|value| OutboundUpdateDraft {
                    system: "slack".to_string(),
                    target_kind: "response_url".to_string(),
                    target_id: value.clone(),
                    task_id: Some(task_id.clone()),
                    run_id: None,
                    body: String::new(),
                    metadata: json!({}),
                }),
            )?;

            Ok(slack_json_response(json!({
                "response_type": "ephemeral",
                "text": format!("Queued run {} for {}.", dispatch.run.run_id, dispatch.run.task_id),
            })))
        }
        "run_status" => {
            let run_id = action
                .value
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    IngressHttpError::BadRequest(
                        "run_status action requires a run_id value".to_string(),
                    )
                })?;
            let state_store = SqliteStateStore::open(&state.root)?;
            let run = state_store
                .get_run(run_id)?
                .ok_or_else(|| IngressHttpError::NotFound(format!("run not found: {run_id}")))?;
            Ok(slack_json_response(json!({
                "response_type": "ephemeral",
                "text": format!(
                    "Run {} is {} on {}.",
                    run.run_id,
                    run.state,
                    run.adapter
                ),
            })))
        }
        other => Err(IngressHttpError::BadRequest(format!(
            "unsupported slack action: {other}"
        ))),
    }
}

async fn linear_webhook(
    State(state): State<IngressAppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, IngressHttpError> {
    let payload = serde_json::from_slice::<LinearWebhookEnvelope>(&body)?;
    verify_linear_request(&headers, &body, payload.webhook_timestamp)?;
    let spec = linear_spec_from_payload(&payload)?;
    let task_id = spec
        .task_id
        .clone()
        .unwrap_or_else(|| generate_linear_task_id(&payload.data));
    let persona_id = select_persona(&state.root, "linear", spec.persona_id.as_deref())?;
    let dispatch = enqueue_run(
        &state.root,
        &task_id,
        persona_id.as_deref(),
        spec.profile_id.as_deref(),
        spec.workflow.as_deref(),
        spec.adapter.as_deref(),
        spec.prompt.clone(),
        Some(ExternalRefDraft {
            system: "linear".to_string(),
            entity_kind: payload
                .entity_type
                .clone()
                .unwrap_or_else(|| "entity".to_string())
                .to_ascii_lowercase(),
            external_id: linear_external_id(&payload.data),
            task_id: Some(task_id.clone()),
            run_id: None,
            persona_id: persona_id.clone(),
            title: payload
                .data
                .get("title")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    payload
                        .data
                        .get("identifier")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                }),
            url: payload.url.clone(),
            metadata: json!({
                "action": payload.action,
                "type": payload.entity_type,
                "url": payload.url,
            }),
        }),
        Some(OutboundUpdateDraft {
            system: "linear".to_string(),
            target_kind: payload
                .entity_type
                .clone()
                .unwrap_or_else(|| "entity".to_string())
                .to_ascii_lowercase(),
            target_id: linear_external_id(&payload.data),
            task_id: Some(task_id.clone()),
            run_id: None,
            body: String::new(),
            metadata: json!({}),
        }),
    )?;

    Ok(json_response(
        StatusCode::ACCEPTED,
        json!({
            "status": "queued",
            "run_id": dispatch.run.run_id,
            "task_id": dispatch.run.task_id,
        }),
    ))
}

fn enqueue_run(
    root: &Path,
    task_id: &str,
    persona_id: Option<&str>,
    profile_id: Option<&str>,
    workflow: Option<&str>,
    adapter: Option<&str>,
    prompt: Option<String>,
    external_ref: Option<ExternalRefDraft>,
    outbound_update: Option<OutboundUpdateDraft>,
) -> Result<IngressDispatchResult, IngressHttpError> {
    let config = SwbConfig::load_from_root(root)?;
    let request = config.build_run_request(
        root, task_id, workflow, adapter, persona_id, profile_id, prompt,
    )?;
    let queue = SqliteIngestQueue::open(root)?;
    queue.enqueue(&swb_core::IngestEnvelope::run_requested(&request))?;
    let receiver = Receiver::open(root)?;
    let _ = receiver.drain_pending()?;
    let state = SqliteStateStore::open(root)?;
    let linked_ref = external_ref.map(|mut draft| {
        draft.run_id = Some(request.run_id.clone());
        if draft.task_id.is_none() {
            draft.task_id = Some(task_id.to_string());
        }
        if draft.persona_id.is_none() {
            draft.persona_id = request.persona_id.clone();
        }
        draft
    });
    if let Some(draft) = linked_ref {
        state.upsert_external_ref(&draft)?;
    }
    let queued_update_id = if let Some(mut draft) = outbound_update {
        draft.run_id = Some(request.run_id.clone());
        if draft.task_id.is_none() {
            draft.task_id = Some(task_id.to_string());
        }
        if draft.body.trim().is_empty() {
            draft.body = format!(
                "Queued run {} for {} using {}.",
                request.run_id,
                task_id,
                request
                    .persona_id
                    .as_deref()
                    .or(request.profile_id.as_deref())
                    .unwrap_or("default")
            );
        }
        Some(state.queue_outbound_update(&draft)?.id)
    } else {
        None
    };
    let run = state.get_run(&request.run_id)?.ok_or_else(|| {
        IngressHttpError::State(swb_state::StateError::MissingRun(request.run_id.clone()))
    })?;
    Ok(IngressDispatchResult {
        run,
        queued_update_id,
    })
}

fn select_persona(
    root: &Path,
    ingress: &str,
    requested: Option<&str>,
) -> Result<Option<String>, IngressHttpError> {
    if let Some(requested) = requested {
        let persona = get_persona_from_root(root, requested, Some(ingress))
            .or_else(|_| get_persona_from_root(root, requested, None))?;
        return Ok(Some(persona.id));
    }

    let personas = list_personas_from_root(root, Some(ingress))?;
    Ok(personas.first().map(|persona| persona.id.clone()))
}

fn parse_form<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, IngressHttpError> {
    serde_urlencoded::from_bytes(body).map_err(IngressHttpError::Form)
}

fn parse_ingress_text(text: &str) -> Result<IngressRunSpec, IngressHttpError> {
    let mut tokens = VecDeque::from(
        text.split_whitespace()
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>(),
    );
    let mut spec = IngressRunSpec::default();
    let mut prompt_parts = Vec::new();

    while let Some(token) = tokens.pop_front() {
        match token.as_str() {
            "--task" => spec.task_id = Some(require_value(&mut tokens, "--task")?),
            "--persona" => spec.persona_id = Some(require_value(&mut tokens, "--persona")?),
            "--profile" => spec.profile_id = Some(require_value(&mut tokens, "--profile")?),
            "--workflow" => spec.workflow = Some(require_value(&mut tokens, "--workflow")?),
            "--adapter" => spec.adapter = Some(require_value(&mut tokens, "--adapter")?),
            "--" => {
                prompt_parts.extend(tokens.into_iter());
                break;
            }
            other if other.starts_with("--") => {
                return Err(IngressHttpError::BadRequest(format!(
                    "unknown ingress flag: {other}"
                )));
            }
            other => prompt_parts.push(other.to_string()),
        }
    }

    let prompt = prompt_parts.join(" ").trim().to_string();
    if !prompt.is_empty() {
        spec.prompt = Some(prompt);
    }
    Ok(spec)
}

fn parse_action_value(value: &str) -> Result<IngressRunSpec, IngressHttpError> {
    if value.trim().is_empty() {
        return Ok(IngressRunSpec::default());
    }
    if let Ok(json_value) = serde_json::from_str::<Value>(value) {
        return Ok(IngressRunSpec {
            task_id: json_value
                .get("task_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            persona_id: json_value
                .get("persona_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            profile_id: json_value
                .get("profile_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            workflow: json_value
                .get("workflow")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            adapter: json_value
                .get("adapter")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            prompt: json_value
                .get("prompt")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
        });
    }
    parse_ingress_text(value)
}

fn linear_spec_from_payload(
    payload: &LinearWebhookEnvelope,
) -> Result<IngressRunSpec, IngressHttpError> {
    let entity_type = payload
        .entity_type
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match entity_type.as_str() {
        "issue" => {
            if !has_stackbench_label(&payload.data) {
                return Err(IngressHttpError::Ignored(
                    "linear issue ignored because it has no stackbench label".to_string(),
                ));
            }
            Ok(IngressRunSpec {
                task_id: payload
                    .data
                    .get("identifier")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        payload
                            .data
                            .get("id")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    }),
                persona_id: None,
                profile_id: None,
                workflow: None,
                adapter: None,
                prompt: Some(build_linear_issue_prompt(&payload.data)),
            })
        }
        "comment" => {
            let body = payload
                .data
                .get("body")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    IngressHttpError::BadRequest(
                        "linear comment payload is missing data.body".to_string(),
                    )
                })?;
            let Some(stripped) = strip_linear_command(body) else {
                return Err(IngressHttpError::Ignored(
                    "linear comment ignored because it does not start with /swb or /stackbench"
                        .to_string(),
                ));
            };
            let mut spec = parse_ingress_text(stripped)?;
            if spec.task_id.is_none() {
                spec.task_id = payload
                    .data
                    .get("issue")
                    .and_then(|issue| issue.get("identifier"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| {
                        payload
                            .data
                            .get("issueId")
                            .and_then(Value::as_str)
                            .map(ToOwned::to_owned)
                    });
            }
            if spec.prompt.is_none() {
                spec.prompt = Some(build_linear_comment_prompt(&payload.data));
            }
            Ok(spec)
        }
        other => Err(IngressHttpError::Ignored(format!(
            "linear event ignored: unsupported type {other}"
        ))),
    }
}

fn build_linear_issue_prompt(data: &Value) -> String {
    let identifier = data
        .get("identifier")
        .and_then(Value::as_str)
        .unwrap_or("Linear issue");
    let title = data
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Untitled");
    let description = data
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| data.get("body").and_then(Value::as_str))
        .unwrap_or("");
    let mut prompt = format!("Investigate Linear issue {identifier}: {title}");
    if !description.trim().is_empty() {
        prompt.push_str("\n\nIssue details:\n");
        prompt.push_str(description.trim());
    }
    prompt
}

fn build_linear_comment_prompt(data: &Value) -> String {
    let issue_identifier = data
        .get("issue")
        .and_then(|issue| issue.get("identifier"))
        .and_then(Value::as_str)
        .unwrap_or("Linear issue");
    let body = data.get("body").and_then(Value::as_str).unwrap_or("");
    let stripped = strip_linear_command(body).unwrap_or(body).trim();
    if stripped.is_empty() {
        format!("Respond to Linear comment request on {issue_identifier}.")
    } else {
        format!("Respond to Linear comment request on {issue_identifier}: {stripped}")
    }
}

fn strip_linear_command(body: &str) -> Option<&str> {
    let trimmed = body.trim();
    if let Some(rest) = trimmed.strip_prefix("/swb") {
        Some(rest.trim())
    } else if let Some(rest) = trimmed.strip_prefix("/stackbench") {
        Some(rest.trim())
    } else {
        None
    }
}

fn has_stackbench_label(data: &Value) -> bool {
    data.get("labels")
        .and_then(Value::as_array)
        .map(|labels| {
            labels.iter().any(|label| {
                label
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| {
                        let normalized = name.trim().to_ascii_lowercase();
                        normalized == "stackbench" || normalized == "swb"
                    })
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn linear_external_id(data: &Value) -> String {
    data.get("identifier")
        .and_then(Value::as_str)
        .or_else(|| data.get("id").and_then(Value::as_str))
        .or_else(|| {
            data.get("issue")
                .and_then(|issue| issue.get("identifier"))
                .and_then(Value::as_str)
        })
        .unwrap_or("linear")
        .to_string()
}

fn generate_slack_task_id(team_id: &str, channel_id: &str) -> String {
    format!(
        "SLACK-{}-{}-{}",
        sanitize_task_component(team_id),
        sanitize_task_component(channel_id),
        OffsetDateTime::now_utc().unix_timestamp()
    )
}

fn generate_linear_task_id(data: &Value) -> String {
    let base = data
        .get("identifier")
        .and_then(Value::as_str)
        .or_else(|| data.get("id").and_then(Value::as_str))
        .unwrap_or("LINEAR");
    sanitize_task_component(base)
}

fn sanitize_task_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_ascii_uppercase()
}

fn slack_command_external_id(form: &SlackCommandForm) -> String {
    match form.trigger_id.as_deref() {
        Some(trigger_id) if !trigger_id.trim().is_empty() => {
            format!("{}/{}", form.channel_id, trigger_id)
        }
        _ => format!(
            "{}/{}/{}",
            form.team_id,
            form.channel_id,
            OffsetDateTime::now_utc().unix_timestamp()
        ),
    }
}

fn require_value(tokens: &mut VecDeque<String>, flag: &str) -> Result<String, IngressHttpError> {
    tokens
        .pop_front()
        .ok_or_else(|| IngressHttpError::BadRequest(format!("missing value for {flag}")))
}

fn verify_slack_request(headers: &HeaderMap, body: &[u8]) -> Result<(), IngressHttpError> {
    let Some(secret) = env::var_os(SLACK_SIGNING_SECRET_ENV) else {
        return Ok(());
    };
    let secret = secret
        .into_string()
        .map_err(|_| IngressHttpError::Auth("invalid slack signing secret".to_string()))?;
    let timestamp = header_text(headers, SLACK_TIMESTAMP_HEADER)?
        .ok_or_else(|| IngressHttpError::Auth("missing X-Slack-Request-Timestamp".to_string()))?;
    let timestamp_value = timestamp
        .parse::<i64>()
        .map_err(|_| IngressHttpError::Auth("invalid X-Slack-Request-Timestamp".to_string()))?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if (now - timestamp_value).abs() > SLACK_ALLOWED_SKEW_SECS {
        return Err(IngressHttpError::Auth(
            "stale slack request timestamp".to_string(),
        ));
    }
    let signature = header_text(headers, SLACK_SIGNATURE_HEADER)?
        .ok_or_else(|| IngressHttpError::Auth("missing X-Slack-Signature".to_string()))?;
    let base = format!("v0:{timestamp}:{}", String::from_utf8_lossy(body));
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| IngressHttpError::Auth("invalid slack signing secret".to_string()))?;
    mac.update(base.as_bytes());
    let expected = format!("v0={}", hex::encode(mac.finalize().into_bytes()));
    if expected != signature {
        return Err(IngressHttpError::Auth(
            "invalid slack signature".to_string(),
        ));
    }
    Ok(())
}

fn verify_linear_request(
    headers: &HeaderMap,
    body: &[u8],
    webhook_timestamp: Option<i64>,
) -> Result<(), IngressHttpError> {
    let Some(secret) = env::var_os(LINEAR_WEBHOOK_SECRET_ENV) else {
        return Ok(());
    };
    let secret = secret
        .into_string()
        .map_err(|_| IngressHttpError::Auth("invalid linear webhook secret".to_string()))?;
    let signature = header_text(headers, LINEAR_SIGNATURE_HEADER)?
        .ok_or_else(|| IngressHttpError::Auth("missing Linear-Signature".to_string()))?;
    let timestamp = webhook_timestamp.ok_or_else(|| {
        IngressHttpError::Auth("missing webhookTimestamp in Linear payload".to_string())
    })?;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|_| IngressHttpError::Auth("invalid linear webhook secret".to_string()))?;
    mac.update(format!("{timestamp}.").as_bytes());
    mac.update(body);
    let expected = hex::encode(mac.finalize().into_bytes());
    if expected != signature {
        return Err(IngressHttpError::Auth(
            "invalid linear signature".to_string(),
        ));
    }
    Ok(())
}

fn header_text(headers: &HeaderMap, name: &str) -> Result<Option<String>, IngressHttpError> {
    headers.get(name).map(header_to_string).transpose()
}

fn header_to_string(value: &HeaderValue) -> Result<String, IngressHttpError> {
    value
        .to_str()
        .map(|value| value.to_string())
        .map_err(|_| IngressHttpError::BadRequest("invalid header encoding".to_string()))
}

fn slack_json_response(body: Value) -> Response {
    let mut response = json_response(StatusCode::OK, body);
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

#[derive(Debug, Error)]
pub enum IngressHttpError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Auth(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Ignored(String),
    #[error("form decode failed: {0}")]
    Form(#[from] serde_urlencoded::de::Error),
    #[error("json decode failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] swb_config::ConfigError),
    #[error(transparent)]
    Queue(#[from] swb_queue_sqlite::QueueError),
    #[error(transparent)]
    Receiver(#[from] swb_receiver::ReceiverError),
    #[error(transparent)]
    State(#[from] swb_state::StateError),
}

impl IntoResponse for IngressHttpError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            Self::BadRequest(_) => (StatusCode::BAD_REQUEST, "bad_request"),
            Self::Auth(_) => (StatusCode::UNAUTHORIZED, "auth_failed"),
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            Self::Ignored(_) => (StatusCode::ACCEPTED, "ignored"),
            Self::Form(_) | Self::Json(_) => (StatusCode::BAD_REQUEST, "decode_failed"),
            Self::Config(_) | Self::Queue(_) | Self::Receiver(_) | Self::State(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        json_response(
            status,
            json!({
                "error": code,
                "detail": self.to_string(),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use tower::ServiceExt;

    use swb_config::{save_persona_to_root, save_profile_to_root, PersonaDraft, ProfileDraft};
    use swb_state::SqliteStateStore;

    use super::{app, parse_ingress_text};

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = env::temp_dir().join(format!("swb-ingress-test-{serial}"));
        let _ = fs::remove_dir_all(&root);
        root
    }

    fn seed_root(root: &PathBuf) {
        fs::create_dir_all(root.join("swb/prompts/runtime")).unwrap();
        fs::write(
            root.join("swb/prompts/runtime/default.md"),
            "Use the repo and keep findings concrete.",
        )
        .unwrap();
        save_profile_to_root(
            root,
            &ProfileDraft {
                id: "eng-review".to_string(),
                display_name: "Engineering Review".to_string(),
                description: "Review code for defects.".to_string(),
                workflow: Some("default".to_string()),
                adapter: Some("codex".to_string()),
                gstack_id: None,
                instructions_markdown: "Find defects and explain risk.".to_string(),
            },
        )
        .unwrap();
        save_persona_to_root(
            root,
            &PersonaDraft {
                id: "slack-review".to_string(),
                display_name: "Slack Review".to_string(),
                description: "Slack ingress persona".to_string(),
                ingress: Some("slack".to_string()),
                default_profile: "eng-review".to_string(),
                default_workflow: None,
                default_adapter: None,
            },
        )
        .unwrap();
        save_persona_to_root(
            root,
            &PersonaDraft {
                id: "linear-review".to_string(),
                display_name: "Linear Review".to_string(),
                description: "Linear ingress persona".to_string(),
                ingress: Some("linear".to_string()),
                default_profile: "eng-review".to_string(),
                default_workflow: None,
                default_adapter: None,
            },
        )
        .unwrap();
    }

    #[test]
    fn parse_ingress_text_recognizes_flags_and_prompt() {
        let parsed = parse_ingress_text(
            "--task ABC-123 --persona slack-review --workflow default review the issue",
        )
        .unwrap();
        assert_eq!(parsed.task_id.as_deref(), Some("ABC-123"));
        assert_eq!(parsed.persona_id.as_deref(), Some("slack-review"));
        assert_eq!(parsed.workflow.as_deref(), Some("default"));
        assert_eq!(parsed.prompt.as_deref(), Some("review the issue"));
    }

    #[tokio::test]
    async fn slack_command_enqueues_run_and_external_refs() {
        let root = unique_temp_root();
        seed_root(&root);
        let app = app(&root);
        let response = app
            .oneshot(
                Request::post("/ingress/slack/command")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "team_id=T1&channel_id=C1&user_id=U1&response_url=https%3A%2F%2Fexample.test%2Fslack&text=--task%20ABC-123%20review%20the%20issue",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let state = SqliteStateStore::open(&root).unwrap();
        let runs = state.list_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].task_id, "ABC-123");
        assert_eq!(runs[0].persona_id.as_deref(), Some("slack-review"));

        let refs = state.list_external_refs_for_run(&runs[0].run_id).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].system, "slack");

        let outbound = state
            .list_outbound_updates(Some("slack"), Some("pending"), 10)
            .unwrap();
        assert_eq!(outbound.len(), 1);
        assert!(outbound[0].body.contains("Queued run"));

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn linear_comment_command_enqueues_run() {
        let root = unique_temp_root();
        seed_root(&root);
        let app = app(&root);
        let body = json!({
            "action": "create",
            "type": "Comment",
            "webhookTimestamp": 12345,
            "data": {
                "id": "comment-1",
                "body": "/swb --task ABC-124 review the rollout plan",
                "issue": {
                    "identifier": "ABC-124"
                }
            },
            "url": "https://linear.app/example/issue/ABC-124"
        });
        let response = app
            .oneshot(
                Request::post("/ingress/linear/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let state = SqliteStateStore::open(&root).unwrap();
        let runs = state.list_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].task_id, "ABC-124");
        assert_eq!(runs[0].persona_id.as_deref(), Some("linear-review"));

        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn linear_issue_without_stackbench_label_is_ignored() {
        let root = unique_temp_root();
        seed_root(&root);
        let app = app(&root);
        let body = json!({
            "action": "create",
            "type": "Issue",
            "webhookTimestamp": 12345,
            "data": {
                "identifier": "ABC-125",
                "title": "Regression report",
                "labels": [
                    { "name": "bug" }
                ]
            }
        });
        let response = app
            .oneshot(
                Request::post("/ingress/linear/webhook")
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let payload = response.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice::<Value>(&payload).unwrap();
        assert_eq!(json["error"], "ignored");

        fs::remove_dir_all(root).unwrap();
    }
}
