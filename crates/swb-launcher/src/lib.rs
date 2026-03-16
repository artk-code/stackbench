use std::path::Path;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use swb_adapters::{
    execute_adapter, execution_result_payload, AdapterExecutionError, AdapterRunContext,
};
use swb_config::{ConfigError, EvaluatorConfig, SwbConfig};
use swb_core::{AdapterEventPayload, IngestEnvelope, IngestKind, RunRecord, RunState};
use swb_eval::{run_evaluator, EvalError};
use swb_jj::{ensure_run_workspace, JjError};
use swb_queue_sqlite::{QueueError, SqliteIngestQueue};
use swb_receiver::{Receiver, ReceiverError};
use swb_state::{SqliteStateStore, StateError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchReport {
    pub considered: usize,
    pub awaiting_review: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchOptions {
    pub only_run_id: Option<String>,
    pub interval: Duration,
    pub max_cycles: Option<usize>,
}

impl Default for WatchOptions {
    fn default() -> Self {
        Self {
            only_run_id: None,
            interval: Duration::from_secs(1),
            max_cycles: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WatchReport {
    pub cycles: usize,
    pub considered: usize,
    pub awaiting_review: usize,
    pub failed: usize,
    pub skipped: usize,
}

#[derive(Debug, Error)]
pub enum LauncherError {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),
    #[error("queue error: {0}")]
    Queue(#[from] QueueError),
    #[error("receiver error: {0}")]
    Receiver(#[from] ReceiverError),
    #[error("state error: {0}")]
    State(#[from] StateError),
    #[error("adapter execution error: {0}")]
    Adapter(#[from] AdapterExecutionError),
    #[error("evaluation error: {0}")]
    Eval(#[from] EvalError),
    #[error("jj error: {0}")]
    Jj(#[from] JjError),
    #[error("configured adapter not found for run {run_id}: {adapter}")]
    MissingAdapter { run_id: String, adapter: String },
}

pub fn run_once(
    root: impl AsRef<Path>,
    only_run_id: Option<&str>,
) -> Result<LaunchReport, LauncherError> {
    let root = root.as_ref();
    let config = SwbConfig::load_from_root(root)?;
    let queue = SqliteIngestQueue::open(root)?;
    let receiver = Receiver::open(root)?;
    receiver.drain_pending()?;
    let state = SqliteStateStore::open(root)?;
    let evaluator = config.evaluators.first().cloned().unwrap_or_default();
    let mut report = LaunchReport {
        considered: 0,
        awaiting_review: 0,
        failed: 0,
        skipped: 0,
    };

    for run in state.list_runs()? {
        if let Some(run_id) = only_run_id {
            if run.run_id != run_id {
                continue;
            }
        }

        if run.state != RunState::Queued {
            report.skipped += 1;
            continue;
        }

        report.considered += 1;
        process_run(root, &config, &queue, &receiver, &run, &evaluator)?;
        let updated_state = SqliteStateStore::open(root)?
            .get_run(&run.run_id)?
            .map(|updated| updated.state)
            .unwrap_or(RunState::Failed);
        match updated_state {
            RunState::AwaitingReview => report.awaiting_review += 1,
            RunState::Failed => report.failed += 1,
            _ => report.skipped += 1,
        }
    }

    Ok(report)
}

pub fn watch<F>(
    root: impl AsRef<Path>,
    options: WatchOptions,
    mut on_cycle: F,
) -> Result<WatchReport, LauncherError>
where
    F: FnMut(usize, &LaunchReport),
{
    let root = root.as_ref().to_path_buf();
    let mut cycles = 0;
    let mut report = WatchReport {
        cycles: 0,
        considered: 0,
        awaiting_review: 0,
        failed: 0,
        skipped: 0,
    };

    loop {
        if options
            .max_cycles
            .is_some_and(|max_cycles| cycles >= max_cycles)
        {
            break;
        }

        cycles += 1;
        let cycle_report = run_once(&root, options.only_run_id.as_deref())?;
        report.cycles = cycles;
        report.considered += cycle_report.considered;
        report.awaiting_review += cycle_report.awaiting_review;
        report.failed += cycle_report.failed;
        report.skipped += cycle_report.skipped;
        on_cycle(cycles, &cycle_report);

        if options
            .max_cycles
            .is_some_and(|max_cycles| cycles >= max_cycles)
        {
            break;
        }

        thread::sleep(options.interval);
    }

    Ok(report)
}

fn process_run(
    root: &Path,
    config: &SwbConfig,
    queue: &SqliteIngestQueue,
    receiver: &Receiver,
    run: &RunRecord,
    evaluator: &EvaluatorConfig,
) -> Result<(), LauncherError> {
    enqueue_and_drain(
        queue,
        receiver,
        IngestEnvelope::state_change(run.run_id.clone(), IngestKind::RunStarted, None),
    )?;

    let workspace_root = ensure_run_workspace(root, config, &run.run_id)?;
    let adapter =
        config
            .find_adapter(&run.adapter)
            .ok_or_else(|| LauncherError::MissingAdapter {
                run_id: run.run_id.clone(),
                adapter: run.adapter.clone(),
            })?;

    let execution = execute_adapter(
        adapter,
        &AdapterRunContext {
            repo_root: root,
            workspace_root: &workspace_root,
            run_id: &run.run_id,
            task_id: &run.task_id,
            workflow: &run.workflow,
            prompt: run.prompt.as_deref(),
        },
    )?;

    enqueue_and_drain(
        queue,
        receiver,
        IngestEnvelope {
            run_id: run.run_id.clone(),
            ts: swb_core::now_utc_rfc3339(),
            kind: IngestKind::AdapterEvent,
            payload: json!(AdapterEventPayload {
                step_id: "primary".to_string(),
                adapter: run.adapter.clone(),
                event_kind: "command_completed".to_string(),
                payload: execution_result_payload(&execution),
            }),
        },
    )?;

    if !execution.success {
        let reason = summarize_failure(&execution.stderr, &execution.stdout, execution.exit_code);
        enqueue_and_drain(
            queue,
            receiver,
            IngestEnvelope::state_change(run.run_id.clone(), IngestKind::RunFailed, Some(reason)),
        )?;
        return Ok(());
    }

    enqueue_and_drain(
        queue,
        receiver,
        IngestEnvelope::state_change(run.run_id.clone(), IngestKind::RunEvaluating, None),
    )?;

    let evaluation = run_evaluator(&workspace_root, evaluator)?;
    enqueue_and_drain(
        queue,
        receiver,
        IngestEnvelope {
            run_id: run.run_id.clone(),
            ts: swb_core::now_utc_rfc3339(),
            kind: IngestKind::AdapterEvent,
            payload: json!(AdapterEventPayload {
                step_id: "evaluation".to_string(),
                adapter: evaluator.name.clone(),
                event_kind: "evaluation_completed".to_string(),
                payload: json!({
                    "passed": evaluation.passed,
                    "results": evaluation
                        .results
                        .iter()
                        .map(|result| json!({
                            "command": result.command,
                            "success": result.success,
                            "exit_code": result.exit_code,
                            "stdout": result.stdout,
                            "stderr": result.stderr,
                        }))
                        .collect::<Vec<_>>(),
                }),
            }),
        },
    )?;
    if evaluation.passed {
        enqueue_and_drain(
            queue,
            receiver,
            IngestEnvelope::state_change(run.run_id.clone(), IngestKind::RunAwaitingReview, None),
        )?;
    } else {
        let reason = evaluation
            .results
            .iter()
            .find(|result| !result.success)
            .map(|result| {
                format!(
                    "evaluation failed: {} (exit {})",
                    result.command, result.exit_code
                )
            })
            .unwrap_or_else(|| "evaluation failed".to_string());
        enqueue_and_drain(
            queue,
            receiver,
            IngestEnvelope::state_change(run.run_id.clone(), IngestKind::RunFailed, Some(reason)),
        )?;
    }

    Ok(())
}

fn enqueue_and_drain(
    queue: &SqliteIngestQueue,
    receiver: &Receiver,
    envelope: IngestEnvelope,
) -> Result<(), LauncherError> {
    queue.enqueue(&envelope)?;
    receiver.drain_pending()?;
    Ok(())
}

fn summarize_failure(stderr: &str, stdout: &str, exit_code: i32) -> String {
    if !stderr.trim().is_empty() {
        format!("adapter exit {exit_code}: {}", stderr.trim())
    } else if !stdout.trim().is_empty() {
        format!("adapter exit {exit_code}: {}", stdout.trim())
    } else {
        format!("adapter exit {exit_code}")
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use swb_core::{IngestEnvelope, RunRequest, RunState};
    use swb_queue_sqlite::SqliteIngestQueue;
    use swb_state::SqliteStateStore;

    use super::{run_once, watch, WatchOptions};

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-launcher-test-{serial}"))
    }

    #[test]
    fn launcher_executes_run_and_moves_to_awaiting_review() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("swb.toml"),
            format!(
                r#"
                    [integration]
                    script_path = "{script_path}"
                    jj_bin = "jj"
                    base_revset = "trunk()"

                    [[adapters]]
                    name = "shell"
                    command = "sh"
                    args = ["-lc", "printf '%s' \"$SWB_PROMPT\" > execution.txt"]
                    prompt_mode = "env"

                    [[workflows]]
                    name = "default"
                    adapters = ["shell"]

                    [[evaluators]]
                    name = "checks"
                    commands = ["test -f execution.txt", "test \"$(cat execution.txt)\" = \"hello\""]
                "#,
                script_path = root.join("fake-jj.sh").display()
            ),
        )
        .unwrap();
        fs::write(
            root.join("fake-jj.sh"),
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(root.join("fake-jj.sh")).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(root.join("fake-jj.sh"), perms).unwrap();
        }

        let queue = SqliteIngestQueue::open(&root).unwrap();
        let request = RunRequest::new("TASK-1", "default", "shell", Some("hello".to_string()));
        queue
            .enqueue(&IngestEnvelope::run_requested(&request))
            .unwrap();

        let report = run_once(&root, None).unwrap();
        assert_eq!(report.awaiting_review, 1);

        let state = SqliteStateStore::open(&root).unwrap();
        let run = state.get_run(&request.run_id).unwrap().unwrap();
        assert_eq!(run.state, RunState::AwaitingReview);
        assert!(root
            .join(".swb/v2/workspaces")
            .join(&request.run_id)
            .join("execution.txt")
            .exists());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn watch_respects_max_cycles_and_aggregates_reports() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("swb.toml"),
            format!(
                r#"
                    [integration]
                    script_path = "{script_path}"
                    jj_bin = "jj"
                    base_revset = "trunk()"

                    [[adapters]]
                    name = "shell"
                    command = "sh"
                    args = ["-lc", "printf '%s' \"$SWB_PROMPT\" > execution.txt"]
                    prompt_mode = "env"

                    [[workflows]]
                    name = "default"
                    adapters = ["shell"]

                    [[evaluators]]
                    name = "checks"
                    commands = ["test -f execution.txt", "test \"$(cat execution.txt)\" = \"hello\""]
                "#,
                script_path = root.join("fake-jj.sh").display()
            ),
        )
        .unwrap();
        fs::write(
            root.join("fake-jj.sh"),
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(root.join("fake-jj.sh")).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(root.join("fake-jj.sh"), perms).unwrap();
        }

        let queue = SqliteIngestQueue::open(&root).unwrap();
        let request = RunRequest::new("TASK-2", "default", "shell", Some("hello".to_string()));
        queue
            .enqueue(&IngestEnvelope::run_requested(&request))
            .unwrap();

        let mut observed_cycles = Vec::new();
        let report = watch(
            &root,
            WatchOptions {
                only_run_id: Some(request.run_id.clone()),
                interval: Duration::ZERO,
                max_cycles: Some(2),
            },
            |cycle, launch_report| observed_cycles.push((cycle, launch_report.clone())),
        )
        .unwrap();

        assert_eq!(report.cycles, 2);
        assert_eq!(report.considered, 1);
        assert_eq!(report.awaiting_review, 1);
        assert_eq!(report.failed, 0);
        assert_eq!(report.skipped, 1);
        assert_eq!(observed_cycles.len(), 2);
        assert_eq!(observed_cycles[0].0, 1);
        assert_eq!(observed_cycles[0].1.awaiting_review, 1);
        assert_eq!(observed_cycles[1].0, 2);
        assert_eq!(observed_cycles[1].1.skipped, 1);

        let state = SqliteStateStore::open(&root).unwrap();
        let run = state.get_run(&request.run_id).unwrap().unwrap();
        assert_eq!(run.state, RunState::AwaitingReview);

        fs::remove_dir_all(root).unwrap();
    }
}
