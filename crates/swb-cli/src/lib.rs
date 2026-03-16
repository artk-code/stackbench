use std::collections::VecDeque;
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};
use swb_adapters::{
    auth_status_from_config, doctor_from_config, login_adapter, registry_from_config,
    AdapterLoginMode,
};
use swb_config::SwbConfig;
use swb_core::{
    AdapterEventPayload, IngestEnvelope, IngestKind, RunLogRecord, RunRequest, RunRequestedPayload,
    RunRecord, RunState, StateChangePayload,
};
use swb_jj::integrate_run;
use swb_launcher::{run_once, watch, WatchOptions};
use swb_queue_sqlite::SqliteIngestQueue;
use swb_receiver::Receiver;
use swb_state::SqliteStateStore;

pub fn run_cli<W: Write, E: Write>(
    root_override: Option<&Path>,
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> i32 {
    match dispatch(root_override, args, stdout, stderr) {
        Ok(code) => code,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            1
        }
    }
}

fn dispatch<W: Write, E: Write>(
    root_override: Option<&Path>,
    args: &[String],
    stdout: &mut W,
    stderr: &mut E,
) -> Result<i32, String> {
    let mut args = VecDeque::from(args.to_vec());
    let Some(command) = args.pop_front() else {
        write_usage(stdout).map_err(|error| error.to_string())?;
        return Ok(0);
    };

    let root = resolve_root(root_override);

    match command.as_str() {
        "run" => handle_run(root, args, stdout),
        "launcher" => handle_launcher(root, args, stdout),
        "receiver" => handle_receiver(root, args, stdout),
        "adapter" => handle_adapter(root, args, stdout),
        "help" | "--help" | "-h" => {
            write_usage(stdout).map_err(|error| error.to_string())?;
            Ok(0)
        }
        other => {
            let _ = writeln!(stderr, "unknown command: {other}");
            write_usage(stderr).map_err(|error| error.to_string())?;
            Ok(2)
        }
    }
}

fn handle_run<W: Write>(
    root: PathBuf,
    mut args: VecDeque<String>,
    stdout: &mut W,
) -> Result<i32, String> {
    let Some(subcommand) = args.pop_front() else {
        return Err(
            "run requires a subcommand: start | status | list | logs | approve | reject | integrate"
                .to_string(),
        );
    };

    match subcommand.as_str() {
        "start" => {
            let task_id = args
                .pop_front()
                .ok_or_else(|| "usage: swb run start <TASK_ID> [--workflow NAME] [--adapter NAME] [--prompt TEXT]".to_string())?;
            let mut workflow_name = None;
            let mut adapter_name = None;
            let mut prompt = None;
            let mut output_json = false;

            while let Some(flag) = args.pop_front() {
                match flag.as_str() {
                    "--workflow" => {
                        workflow_name = Some(require_flag_value(&mut args, "--workflow")?)
                    }
                    "--adapter" => adapter_name = Some(require_flag_value(&mut args, "--adapter")?),
                    "--prompt" => prompt = Some(require_flag_value(&mut args, "--prompt")?),
                    "--json" => output_json = true,
                    other => return Err(format!("unknown flag for run start: {other}")),
                }
            }

            let config = SwbConfig::load_from_root(&root).map_err(|error| error.to_string())?;
            let workflow = config
                .resolve_workflow(workflow_name.as_deref())
                .map_err(|error| error.to_string())?;
            let adapter = config
                .resolve_adapter(workflow, adapter_name.as_deref())
                .map_err(|error| error.to_string())?;

            let request = RunRequest::new(task_id, &workflow.name, &adapter.name, prompt);
            let queue = SqliteIngestQueue::open(&root).map_err(|error| error.to_string())?;
            let queued = queue
                .enqueue(&IngestEnvelope::run_requested(&request))
                .map_err(|error| error.to_string())?;

            if output_json {
                write_json(
                    stdout,
                    &json!({
                        "status": "queued",
                        "run_id": request.run_id,
                        "task_id": request.task_id,
                        "workflow": request.workflow,
                        "adapter": request.adapter,
                        "queue_entry": queued.id,
                    }),
                )?;
            } else {
                writeln!(
                    stdout,
                    "queued\trun_id={}\ttask_id={}\tworkflow={}\tadapter={}\tqueue_entry={}",
                    request.run_id, request.task_id, request.workflow, request.adapter, queued.id
                )
                .map_err(|error| error.to_string())?;
            }
            Ok(0)
        }
        "status" => {
            let run_id = args
                .pop_front()
                .ok_or_else(|| "usage: swb run status <RUN_ID>".to_string())?;
            let output_json = take_bool_flag(&mut args, "--json");
            if let Some(flag) = args.front() {
                return Err(format!("unknown flag for run status: {flag}"));
            }
            let receiver = Receiver::open(&root).map_err(|error| error.to_string())?;
            let drain = receiver
                .drain_pending()
                .map_err(|error| format!("receiver drain failed: {error}"))?;
            let state = SqliteStateStore::open(&root).map_err(|error| error.to_string())?;
            let Some(run) = state.get_run(&run_id).map_err(|error| error.to_string())? else {
                return Err(format!("run not found: {run_id}"));
            };

            if output_json {
                write_json(
                    stdout,
                    &json!({
                        "run": run,
                        "drained": drain.processed,
                    }),
                )?;
            } else {
                writeln!(stdout, "run_id={}", run.run_id).map_err(|error| error.to_string())?;
                writeln!(stdout, "task_id={}", run.task_id).map_err(|error| error.to_string())?;
                writeln!(stdout, "workflow={}", run.workflow).map_err(|error| error.to_string())?;
                writeln!(stdout, "adapter={}", run.adapter).map_err(|error| error.to_string())?;
                writeln!(stdout, "state={}", run.state).map_err(|error| error.to_string())?;
                writeln!(stdout, "last_event={}", run.last_event_kind)
                    .map_err(|error| error.to_string())?;
                writeln!(stdout, "updated_at={}", run.updated_at)
                    .map_err(|error| error.to_string())?;
                writeln!(stdout, "drained={}", drain.processed)
                    .map_err(|error| error.to_string())?;
            }
            Ok(0)
        }
        "list" => {
            let output_json = take_bool_flag(&mut args, "--json");
            if let Some(flag) = args.front() {
                return Err(format!("unknown flag for run list: {flag}"));
            }
            let receiver = Receiver::open(&root).map_err(|error| error.to_string())?;
            let _ = receiver
                .drain_pending()
                .map_err(|error| format!("receiver drain failed: {error}"))?;
            let state = SqliteStateStore::open(&root).map_err(|error| error.to_string())?;
            let runs = state.list_runs().map_err(|error| error.to_string())?;
            if output_json {
                write_json(stdout, &json!({ "runs": runs }))?;
            } else {
                for run in runs {
                    writeln!(
                        stdout,
                        "{}\t{}\t{}\t{}\t{}",
                        run.run_id, run.state, run.task_id, run.workflow, run.adapter
                    )
                    .map_err(|error| error.to_string())?;
                }
            }
            Ok(0)
        }
        "logs" => {
            let run_id = args
                .pop_front()
                .ok_or_else(|| "usage: swb run logs <RUN_ID> [--limit N]".to_string())?;
            let mut limit = 200_usize;
            let mut output_json = false;
            while let Some(flag) = args.pop_front() {
                match flag.as_str() {
                    "--limit" => {
                        let value = require_flag_value(&mut args, "--limit")?;
                        limit = value
                            .parse::<usize>()
                            .map_err(|_| format!("invalid value for --limit: {value}"))?;
                    }
                    "--json" => output_json = true,
                    other => return Err(format!("unknown flag for run logs: {other}")),
                }
            }

            let receiver = Receiver::open(&root).map_err(|error| error.to_string())?;
            let _ = receiver
                .drain_pending()
                .map_err(|error| format!("receiver drain failed: {error}"))?;
            let state = SqliteStateStore::open(&root).map_err(|error| error.to_string())?;
            if state
                .get_run(&run_id)
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Err(format!("run not found: {run_id}"));
            }

            let records = state
                .list_run_logs(&run_id, limit)
                .map_err(|error| error.to_string())?;
            if output_json {
                write_json(stdout, &json!({ "run_id": run_id, "logs": records }))?;
            } else {
                for record in records {
                    writeln!(stdout, "{}", format_run_log(&record))
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(0)
        }
        "approve" => {
            let run_id = args
                .pop_front()
                .ok_or_else(|| "usage: swb run approve <RUN_ID> [--reason TEXT]".to_string())?;
            let mut reason = None;
            let mut output_json = false;
            while let Some(flag) = args.pop_front() {
                match flag.as_str() {
                    "--reason" => reason = Some(require_flag_value(&mut args, "--reason")?),
                    "--json" => output_json = true,
                    other => return Err(format!("unknown flag for run approve: {other}")),
                }
            }
            change_run_state(
                &root,
                &run_id,
                IngestKind::RunApproved,
                reason,
                stdout,
                output_json,
            )
        }
        "reject" => {
            let run_id = args
                .pop_front()
                .ok_or_else(|| "usage: swb run reject <RUN_ID> [--reason TEXT]".to_string())?;
            let mut reason = None;
            let mut output_json = false;
            while let Some(flag) = args.pop_front() {
                match flag.as_str() {
                    "--reason" => reason = Some(require_flag_value(&mut args, "--reason")?),
                    "--json" => output_json = true,
                    other => return Err(format!("unknown flag for run reject: {other}")),
                }
            }
            change_run_state(
                &root,
                &run_id,
                IngestKind::RunRejected,
                reason,
                stdout,
                output_json,
            )
        }
        "integrate" => {
            let run_id = args.pop_front().ok_or_else(|| {
                "usage: swb run integrate <RUN_ID> [--message TEXT]".to_string()
            })?;
            let mut message = None;
            let mut output_json = false;
            while let Some(flag) = args.pop_front() {
                match flag.as_str() {
                    "--message" => message = Some(require_flag_value(&mut args, "--message")?),
                    "--json" => output_json = true,
                    other => return Err(format!("unknown flag for run integrate: {other}")),
                }
            }

            let receiver = Receiver::open(&root).map_err(|error| error.to_string())?;
            let _ = receiver
                .drain_pending()
                .map_err(|error| format!("receiver drain failed: {error}"))?;
            let state = SqliteStateStore::open(&root).map_err(|error| error.to_string())?;
            let Some(run) = state.get_run(&run_id).map_err(|error| error.to_string())? else {
                return Err(format!("run not found: {run_id}"));
            };
            if run.state != RunState::Approved {
                return Err(format!(
                    "run must be approved before integration: state={}",
                    run.state
                ));
            }

            let config = SwbConfig::load_from_root(&root).map_err(|error| error.to_string())?;
            let integration = integrate_run(&root, &config, &run_id, message.as_deref())
                .map_err(|error| error.to_string())?;
            let updated = apply_run_state_change(
                &root,
                &run_id,
                IngestKind::RunIntegrated,
                Some(format!("change_id={}", integration.change_id)),
            )?;
            if output_json {
                write_json(
                    stdout,
                    &json!({
                        "run": updated,
                        "integration": {
                            "workspace_root": integration.workspace_root.display().to_string(),
                            "change_id": integration.change_id,
                            "detail": integration.detail,
                        }
                    }),
                )?;
            } else {
                writeln!(stdout, "run_id={}", updated.run_id)
                    .map_err(|error| error.to_string())?;
                writeln!(stdout, "state={}", updated.state).map_err(|error| error.to_string())?;
                writeln!(stdout, "workspace={}", integration.workspace_root.display())
                    .map_err(|error| error.to_string())?;
                if !integration.detail.is_empty() {
                    writeln!(stdout, "detail={}", integration.detail)
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(0)
        }
        other => Err(format!("unknown run subcommand: {other}")),
    }
}

fn handle_launcher<W: Write>(
    root: PathBuf,
    mut args: VecDeque<String>,
    stdout: &mut W,
) -> Result<i32, String> {
    let Some(subcommand) = args.pop_front() else {
        return Err("launcher requires a subcommand: run-once | watch".to_string());
    };

    match subcommand.as_str() {
        "run-once" => {
            let mut run_id = None;
            let mut output_json = false;
            while let Some(arg) = args.pop_front() {
                match arg.as_str() {
                    "--json" => output_json = true,
                    other if other.starts_with("--") => {
                        return Err(format!("unknown flag for launcher run-once: {other}"));
                    }
                    other => {
                        if run_id.is_some() {
                            return Err(
                                "usage: swb launcher run-once [RUN_ID] [--json]".to_string()
                            );
                        }
                        run_id = Some(other.to_string());
                    }
                }
            }
            let report = run_once(&root, run_id.as_deref()).map_err(|error| error.to_string())?;
            if output_json {
                write_json(stdout, &json!(report))?;
            } else {
                writeln!(
                    stdout,
                    "considered={}\tawaiting_review={}\tfailed={}\tskipped={}",
                    report.considered, report.awaiting_review, report.failed, report.skipped
                )
                .map_err(|error| error.to_string())?;
            }
            Ok(0)
        }
        "watch" => {
            let mut run_id = None;
            let mut interval_ms = 1_000_u64;
            let mut max_cycles = None;
            let mut output_json = false;

            while let Some(arg) = args.pop_front() {
                match arg.as_str() {
                    "--interval-ms" => {
                        let value = require_flag_value(&mut args, "--interval-ms")?;
                        interval_ms = value
                            .parse::<u64>()
                            .map_err(|_| format!("invalid value for --interval-ms: {value}"))?;
                    }
                    "--max-cycles" => {
                        let value = require_flag_value(&mut args, "--max-cycles")?;
                        max_cycles = Some(
                            value
                                .parse::<usize>()
                                .map_err(|_| format!("invalid value for --max-cycles: {value}"))?,
                        );
                    }
                    "--json" => output_json = true,
                    other if other.starts_with("--") => {
                        return Err(format!("unknown flag for launcher watch: {other}"));
                    }
                    other => {
                        if run_id.is_some() {
                            return Err(
                                "usage: swb launcher watch [RUN_ID] [--interval-ms N] [--max-cycles N]".to_string(),
                            );
                        }
                        run_id = Some(other.to_string());
                    }
                }
            }

            let mut write_error = None;
            let report = watch(
                &root,
                WatchOptions {
                    only_run_id: run_id,
                    interval: Duration::from_millis(interval_ms),
                    max_cycles,
                },
                |cycle, cycle_report| {
                    if write_error.is_none() {
                        let result = if output_json {
                            writeln!(
                                stdout,
                                "{}",
                                serde_json::to_string(&json!({
                                    "type": "cycle",
                                    "cycle": cycle,
                                    "considered": cycle_report.considered,
                                    "awaiting_review": cycle_report.awaiting_review,
                                    "failed": cycle_report.failed,
                                    "skipped": cycle_report.skipped,
                                }))
                                .unwrap_or_else(|_| "{\"type\":\"cycle_error\"}".to_string())
                            )
                        } else {
                            writeln!(
                                stdout,
                                "cycle={}\tconsidered={}\tawaiting_review={}\tfailed={}\tskipped={}",
                                cycle,
                                cycle_report.considered,
                                cycle_report.awaiting_review,
                                cycle_report.failed,
                                cycle_report.skipped
                            )
                        };
                        if let Err(error) = result {
                            write_error = Some(error.to_string());
                        }
                    }
                },
            )
            .map_err(|error| error.to_string())?;
            if let Some(error) = write_error {
                return Err(error);
            }

            if output_json {
                write_json(
                    stdout,
                    &json!({
                        "type": "summary",
                        "cycles": report.cycles,
                        "considered": report.considered,
                        "awaiting_review": report.awaiting_review,
                        "failed": report.failed,
                        "skipped": report.skipped,
                    }),
                )?;
            } else {
                writeln!(
                    stdout,
                    "cycles={}\tconsidered={}\tawaiting_review={}\tfailed={}\tskipped={}",
                    report.cycles,
                    report.considered,
                    report.awaiting_review,
                    report.failed,
                    report.skipped
                )
                .map_err(|error| error.to_string())?;
            }
            Ok(0)
        }
        other => Err(format!("unknown launcher subcommand: {other}")),
    }
}

fn handle_receiver<W: Write>(
    root: PathBuf,
    mut args: VecDeque<String>,
    stdout: &mut W,
) -> Result<i32, String> {
    let Some(subcommand) = args.pop_front() else {
        return Err("receiver requires a subcommand: drain".to_string());
    };

    match subcommand.as_str() {
        "drain" => {
            let receiver = Receiver::open(&root).map_err(|error| error.to_string())?;
            let report = receiver
                .drain_pending()
                .map_err(|error| format!("receiver drain failed: {error}"))?;
            writeln!(
                stdout,
                "processed={}\tskipped={}",
                report.processed, report.skipped
            )
            .map_err(|error| error.to_string())?;
            Ok(0)
        }
        other => Err(format!("unknown receiver subcommand: {other}")),
    }
}

fn handle_adapter<W: Write>(
    root: PathBuf,
    mut args: VecDeque<String>,
    stdout: &mut W,
) -> Result<i32, String> {
    let Some(subcommand) = args.pop_front() else {
        return Err("adapter requires a subcommand: list | doctor | auth".to_string());
    };

    let config = SwbConfig::load_from_root(&root).map_err(|error| error.to_string())?;

    match subcommand.as_str() {
        "list" => {
            let output_json = take_bool_flag(&mut args, "--json");
            if let Some(flag) = args.front() {
                return Err(format!("unknown flag for adapter list: {flag}"));
            }
            let adapters = registry_from_config(&config);
            if output_json {
                write_json(stdout, &json!({ "adapters": adapters }))?;
            } else {
                for adapter in adapters {
                    writeln!(
                        stdout,
                        "{}\tcommand={}\tauth={:?}\tprompt_mode={:?}\tstreaming={}\tartifacts={}",
                        adapter.name,
                        adapter.command,
                        adapter.auth_strategy,
                        adapter.prompt_mode,
                        adapter.capabilities.streaming,
                        adapter.capabilities.artifacts
                    )
                    .map_err(|error| error.to_string())?;
                }
            }
            Ok(0)
        }
        "doctor" => {
            let output_json = take_bool_flag(&mut args, "--json");
            if let Some(flag) = args.front() {
                return Err(format!("unknown flag for adapter doctor: {flag}"));
            }
            let reports = doctor_from_config(&config);
            if output_json {
                write_json(stdout, &json!({ "adapters": reports }))?;
            } else {
                for adapter in reports {
                    writeln!(
                        stdout,
                        "{}\tavailable={}\tlogged_in={}\tauth_method={}\tlogin_supported={}\tdevice_login_supported={}\tdetail={}",
                        adapter.name,
                        adapter.available,
                        adapter
                            .logged_in
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "n/a".to_string()),
                        adapter.auth_method.unwrap_or_else(|| "n/a".to_string()),
                        adapter.login_supported,
                        adapter.device_login_supported,
                        adapter.detail
                    )
                    .map_err(|error| error.to_string())?;
                }
            }
            Ok(0)
        }
        "auth" => {
            let Some(action) = args.pop_front() else {
                return Err("adapter auth requires a subcommand: status | login".to_string());
            };
            match action.as_str() {
                "status" => {
                    let mut adapter_name = None;
                    let mut output_json = false;
                    while let Some(arg) = args.pop_front() {
                        match arg.as_str() {
                            "--json" => output_json = true,
                            other if other.starts_with("--") => {
                                return Err(format!(
                                    "unknown flag for adapter auth status: {other}"
                                ));
                            }
                            other => {
                                if adapter_name.is_some() {
                                    return Err(
                                        "usage: swb adapter auth status [ADAPTER] [--json]"
                                            .to_string(),
                                    );
                                }
                                adapter_name = Some(other.to_string());
                            }
                        }
                    }
                    let reports = auth_status_from_config(&config, adapter_name.as_deref())
                        .map_err(|error| error.to_string())?;
                    if output_json {
                        write_json(stdout, &json!({ "adapters": reports }))?;
                    } else {
                        for adapter in reports {
                            writeln!(
                                stdout,
                                "{}\tavailable={}\tlogged_in={}\tlogin_supported={}\tdevice_login_supported={}\tdetail={}",
                                adapter.name,
                                adapter.available,
                                adapter
                                    .logged_in
                                    .map(|value| value.to_string())
                                    .unwrap_or_else(|| "n/a".to_string()),
                                adapter.login_supported,
                                adapter.device_login_supported,
                                adapter.detail
                            )
                            .map_err(|error| error.to_string())?;
                        }
                    }
                    Ok(0)
                }
                "login" => {
                    let adapter_name = args.pop_front().ok_or_else(|| {
                        "usage: swb adapter auth login <ADAPTER> [--device] [--json]".to_string()
                    })?;
                    let mut mode = AdapterLoginMode::Default;
                    let mut output_json = false;
                    while let Some(flag) = args.pop_front() {
                        match flag.as_str() {
                            "--device" => mode = AdapterLoginMode::Device,
                            "--json" => output_json = true,
                            other => {
                                return Err(format!(
                                    "unknown flag for adapter auth login: {other}"
                                ));
                            }
                        }
                    }
                    let result = login_adapter(&config, &adapter_name, mode)
                        .map_err(|error| error.to_string())?;
                    if output_json {
                        write_json(stdout, &json!(result))?;
                    } else {
                        writeln!(
                            stdout,
                            "adapter={}\tmode={}\tsuccess={}\tavailable={}\texit_code={}",
                            result.name,
                            result.mode,
                            result.success,
                            result.available,
                            result.exit_code
                        )
                        .map_err(|error| error.to_string())?;
                        if let Some(command) = result.command {
                            writeln!(stdout, "command={command}")
                                .map_err(|error| error.to_string())?;
                        }
                        if !result.stdout.is_empty() {
                            writeln!(stdout, "stdout={}", result.stdout)
                                .map_err(|error| error.to_string())?;
                        }
                        if !result.stderr.is_empty() {
                            writeln!(stdout, "stderr={}", result.stderr)
                                .map_err(|error| error.to_string())?;
                        }
                    }
                    Ok(0)
                }
                other => Err(format!("unknown adapter auth subcommand: {other}")),
            }
        }
        other => Err(format!("unknown adapter subcommand: {other}")),
    }
}

fn require_flag_value(args: &mut VecDeque<String>, flag: &str) -> Result<String, String> {
    args.pop_front()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn take_bool_flag(args: &mut VecDeque<String>, flag: &str) -> bool {
    if let Some(position) = args.iter().position(|arg| arg == flag) {
        args.remove(position);
        true
    } else {
        false
    }
}

fn write_json<W: Write>(stdout: &mut W, value: &Value) -> Result<(), String> {
    writeln!(
        stdout,
        "{}",
        serde_json::to_string(value).map_err(|error| error.to_string())?
    )
    .map_err(|error| error.to_string())
}

fn format_run_log(record: &RunLogRecord) -> String {
    let mut fields = vec![
        format!("entry={}", record.entry_id),
        format!("ts={}", record.envelope.ts),
        format!("applied_at={}", record.applied_at),
        format!("kind={}", record.envelope.kind),
    ];

    match record.envelope.kind {
        IngestKind::RunRequested => {
            if let Ok(payload) =
                serde_json::from_value::<RunRequestedPayload>(record.envelope.payload.clone())
            {
                fields.push(format!("task_id={}", payload.task_id));
                fields.push(format!("workflow={}", payload.workflow));
                fields.push(format!("adapter={}", payload.adapter));
                if let Some(prompt) = payload.prompt {
                    push_text_field(&mut fields, "prompt", &prompt);
                }
            } else {
                push_json_payload(&mut fields, &record.envelope.payload);
            }
        }
        IngestKind::RunStarted
        | IngestKind::RunEvaluating
        | IngestKind::RunAwaitingReview
        | IngestKind::RunApproved
        | IngestKind::RunRejected
        | IngestKind::RunIntegrated
        | IngestKind::RunFailed
        | IngestKind::RunCancelled => {
            if let Ok(payload) =
                serde_json::from_value::<StateChangePayload>(record.envelope.payload.clone())
            {
                if let Some(reason) = payload.reason {
                    push_text_field(&mut fields, "reason", &reason);
                }
            } else {
                push_json_payload(&mut fields, &record.envelope.payload);
            }
        }
        IngestKind::AdapterEvent => {
            if let Ok(payload) =
                serde_json::from_value::<AdapterEventPayload>(record.envelope.payload.clone())
            {
                fields.push(format!("step={}", payload.step_id));
                fields.push(format!("adapter={}", payload.adapter));
                fields.push(format!("event={}", payload.event_kind));
                summarize_event_payload(&mut fields, &payload.payload);
            } else {
                push_json_payload(&mut fields, &record.envelope.payload);
            }
        }
    }

    fields.join("\t")
}

fn summarize_event_payload(fields: &mut Vec<String>, payload: &Value) {
    if let Some(success) = payload.get("success").and_then(Value::as_bool) {
        fields.push(format!("success={success}"));
    }
    if let Some(passed) = payload.get("passed").and_then(Value::as_bool) {
        fields.push(format!("passed={passed}"));
    }
    if let Some(exit_code) = payload.get("exit_code").and_then(Value::as_i64) {
        fields.push(format!("exit_code={exit_code}"));
    }
    if let Some(duration_ms) = payload.get("duration_ms").and_then(Value::as_u64) {
        fields.push(format!("duration_ms={duration_ms}"));
    }
    if let Some(results) = payload.get("results").and_then(Value::as_array) {
        fields.push(format!("results={}", results.len()));
        let failed = results
            .iter()
            .filter(|result| result.get("success").and_then(Value::as_bool) == Some(false))
            .count();
        fields.push(format!("failed={failed}"));
    }
    if let Some(stdout) = payload.get("stdout").and_then(Value::as_str) {
        push_text_field(fields, "stdout", stdout);
    }
    if let Some(stderr) = payload.get("stderr").and_then(Value::as_str) {
        push_text_field(fields, "stderr", stderr);
    }

    if fields
        .last()
        .is_some_and(|field| field.starts_with("event="))
    {
        push_json_payload(fields, payload);
    }
}

fn push_text_field(fields: &mut Vec<String>, key: &str, value: &str) {
    let compact = compact_text(value);
    if compact.is_empty() {
        return;
    }
    if let Ok(encoded) = serde_json::to_string(&compact) {
        fields.push(format!("{key}={encoded}"));
    } else {
        fields.push(format!("{key}={compact}"));
    }
}

fn push_json_payload(fields: &mut Vec<String>, payload: &Value) {
    push_text_field(fields, "payload", &payload.to_string());
}

fn compact_text(value: &str) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut output = String::new();
    for (index, ch) in compact.chars().enumerate() {
        if index >= 160 {
            output.push_str("...");
            break;
        }
        output.push(ch);
    }
    output
}

fn change_run_state<W: Write>(
    root: &Path,
    run_id: &str,
    kind: IngestKind,
    reason: Option<String>,
    stdout: &mut W,
    output_json: bool,
) -> Result<i32, String> {
    let updated = apply_run_state_change(root, run_id, kind, reason)?;
    if output_json {
        write_json(stdout, &json!({ "run": updated }))?;
    } else {
        writeln!(stdout, "run_id={}", updated.run_id).map_err(|error| error.to_string())?;
        writeln!(stdout, "state={}", updated.state).map_err(|error| error.to_string())?;
    }
    Ok(0)
}

fn apply_run_state_change(
    root: &Path,
    run_id: &str,
    kind: IngestKind,
    reason: Option<String>,
) -> Result<RunRecord, String> {
    let queue = SqliteIngestQueue::open(root).map_err(|error| error.to_string())?;
    let receiver = Receiver::open(root).map_err(|error| error.to_string())?;
    let state = SqliteStateStore::open(root).map_err(|error| error.to_string())?;
    let Some(existing) = state.get_run(run_id).map_err(|error| error.to_string())? else {
        return Err(format!("run not found: {run_id}"));
    };

    queue
        .enqueue(&IngestEnvelope::state_change(
            run_id.to_string(),
            kind,
            reason,
        ))
        .map_err(|error| error.to_string())?;
    let _ = receiver
        .drain_pending()
        .map_err(|error| format!("receiver drain failed: {error}"))?;
    let updated = SqliteStateStore::open(root)
        .map_err(|error| error.to_string())?
        .get_run(run_id)
        .map_err(|error| error.to_string())?
        .unwrap_or(existing);
    Ok(updated)
}

fn write_usage<W: Write>(writer: &mut W) -> std::io::Result<()> {
    writeln!(writer, "Stackbench CLI")?;
    writeln!(
        writer,
        "  swb run start <TASK_ID> [--workflow NAME] [--adapter NAME] [--prompt TEXT] [--json]"
    )?;
    writeln!(writer, "  swb run status <RUN_ID> [--json]")?;
    writeln!(writer, "  swb run list [--json]")?;
    writeln!(writer, "  swb run logs <RUN_ID> [--limit N] [--json]")?;
    writeln!(writer, "  swb run approve <RUN_ID> [--reason TEXT] [--json]")?;
    writeln!(writer, "  swb run reject <RUN_ID> [--reason TEXT] [--json]")?;
    writeln!(writer, "  swb run integrate <RUN_ID> [--message TEXT] [--json]")?;
    writeln!(writer, "  swb launcher run-once [RUN_ID] [--json]")?;
    writeln!(
        writer,
        "  swb launcher watch [RUN_ID] [--interval-ms N] [--max-cycles N] [--json]"
    )?;
    writeln!(writer, "  swb receiver drain")?;
    writeln!(writer, "  swb adapter list [--json]")?;
    writeln!(writer, "  swb adapter doctor [--json]")?;
    writeln!(writer, "  swb adapter auth status [ADAPTER] [--json]")?;
    writeln!(
        writer,
        "  swb adapter auth login <ADAPTER> [--device] [--json]"
    )?;
    Ok(())
}

fn resolve_root(root_override: Option<&Path>) -> PathBuf {
    if let Some(root) = root_override {
        return root.to_path_buf();
    }

    env::var("SWB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::Value;

    use super::run_cli;

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-cli-test-{serial}"))
    }

    fn run(root: &PathBuf, args: &[&str]) -> (i32, String, String) {
        let args = args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let exit_code = run_cli(Some(root.as_path()), &args, &mut stdout, &mut stderr);
        (
            exit_code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn run_start_and_status_flow() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();

        let (exit_code, stdout, stderr) = run(&root, &["run", "start", "TASK-1"]);
        assert_eq!(exit_code, 0);
        assert!(stderr.is_empty());
        let run_id = stdout
            .split_whitespace()
            .find_map(|part| part.strip_prefix("run_id="))
            .unwrap()
            .to_string();

        let (status_code, status_stdout, _) = run(&root, &["run", "status", &run_id]);
        assert_eq!(status_code, 0);
        assert!(status_stdout.contains("state=queued"));
        assert!(status_stdout.contains("drained=1"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adapter_list_uses_default_config() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();

        let (exit_code, stdout, _) = run(&root, &["adapter", "list"]);
        assert_eq!(exit_code, 0);
        assert!(stdout.contains("codex"));
        assert!(stdout.contains("claude_code"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approve_and_integrate_flow() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_jj = root.join("fake-jj");
        let fake_script = root.join("fake-jj.sh");
        fs::write(
            &fake_jj,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"log\" ]]; then echo change-test-123; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        fs::write(
            &fake_script,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\nif [[ \"$1\" == \"integrate\" ]]; then echo \"$*\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&fake_jj, &fake_script] {
                let mut perms = fs::metadata(path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(path, perms).unwrap();
            }
        }
        fs::write(
            root.join("swb.toml"),
            format!(
                r#"
                    [integration]
                    script_path = "{script_path}"
                    jj_bin = "{jj_bin}"
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
                    commands = ["test -f execution.txt"]
                "#,
                script_path = fake_script.display(),
                jj_bin = fake_jj.display(),
            ),
        )
        .unwrap();

        let (start_code, start_stdout, _) =
            run(&root, &["run", "start", "TASK-2", "--adapter", "shell"]);
        assert_eq!(start_code, 0);
        let run_id = start_stdout
            .split_whitespace()
            .find_map(|part| part.strip_prefix("run_id="))
            .unwrap()
            .to_string();

        let (launch_code, launch_stdout, _) = run(&root, &["launcher", "run-once", &run_id]);
        assert_eq!(launch_code, 0);
        assert!(launch_stdout.contains("awaiting_review=1"));

        let (approve_code, approve_stdout, _) = run(&root, &["run", "approve", &run_id]);
        assert_eq!(approve_code, 0);
        assert!(approve_stdout.contains("state=approved"));

        let (integrate_code, integrate_stdout, _) = run(&root, &["run", "integrate", &run_id]);
        assert_eq!(integrate_code, 0);
        assert!(integrate_stdout.contains("state=integrated"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn approve_and_integrate_support_json_output() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_jj = root.join("fake-jj");
        let fake_script = root.join("fake-jj.sh");
        fs::write(
            &fake_jj,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"log\" ]]; then echo change-test-456; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        fs::write(
            &fake_script,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\nif [[ \"$1\" == \"integrate\" ]]; then echo \"$*\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for path in [&fake_jj, &fake_script] {
                let mut perms = fs::metadata(path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(path, perms).unwrap();
            }
        }
        fs::write(
            root.join("swb.toml"),
            format!(
                r#"
                    [integration]
                    script_path = "{script_path}"
                    jj_bin = "{jj_bin}"
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
                    commands = ["test -f execution.txt"]
                "#,
                script_path = fake_script.display(),
                jj_bin = fake_jj.display(),
            ),
        )
        .unwrap();

        let (start_code, start_stdout, _) =
            run(&root, &["run", "start", "TASK-JSON", "--adapter", "shell"]);
        assert_eq!(start_code, 0);
        let run_id = start_stdout
            .split_whitespace()
            .find_map(|part| part.strip_prefix("run_id="))
            .unwrap()
            .to_string();

        let (launch_code, _, launch_stderr) = run(&root, &["launcher", "run-once", &run_id]);
        assert_eq!(launch_code, 0);
        assert!(launch_stderr.is_empty());

        let (approve_code, approve_stdout, approve_stderr) =
            run(&root, &["run", "approve", &run_id, "--json"]);
        assert_eq!(approve_code, 0);
        assert!(approve_stderr.is_empty());
        let approve_json: Value = serde_json::from_str(&approve_stdout).unwrap();
        assert_eq!(approve_json["run"]["run_id"], run_id);
        assert_eq!(approve_json["run"]["state"], "approved");

        let (integrate_code, integrate_stdout, integrate_stderr) = run(
            &root,
            &["run", "integrate", &run_id, "--message", "ship it", "--json"],
        );
        assert_eq!(integrate_code, 0);
        assert!(integrate_stderr.is_empty());
        let integrate_json: Value = serde_json::from_str(&integrate_stdout).unwrap();
        assert_eq!(integrate_json["run"]["run_id"], run_id);
        assert_eq!(integrate_json["run"]["state"], "integrated");
        assert_eq!(integrate_json["integration"]["change_id"], "change-test-456");
        assert_eq!(
            integrate_json["integration"]["workspace_root"]
                .as_str()
                .unwrap(),
            root.join(".swb")
                .join("workspaces")
                .join(&run_id)
                .display()
                .to_string()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn launcher_watch_processes_run_in_bounded_mode() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_script = root.join("fake-jj.sh");
        fs::write(
            &fake_script,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_script, perms).unwrap();
        }
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
                    commands = ["test -f execution.txt"]
                "#,
                script_path = fake_script.display(),
            ),
        )
        .unwrap();

        let (start_code, start_stdout, _) =
            run(&root, &["run", "start", "TASK-3", "--adapter", "shell"]);
        assert_eq!(start_code, 0);
        let run_id = start_stdout
            .split_whitespace()
            .find_map(|part| part.strip_prefix("run_id="))
            .unwrap()
            .to_string();

        let (watch_code, watch_stdout, watch_stderr) = run(
            &root,
            &[
                "launcher",
                "watch",
                &run_id,
                "--interval-ms",
                "0",
                "--max-cycles",
                "1",
            ],
        );
        assert_eq!(watch_code, 0);
        assert!(watch_stderr.is_empty());
        assert!(watch_stdout.contains("cycle=1"));
        assert!(watch_stdout.contains("cycles=1"));
        assert!(watch_stdout.contains("awaiting_review=1"));

        let (status_code, status_stdout, _) = run(&root, &["run", "status", &run_id]);
        assert_eq!(status_code, 0);
        assert!(status_stdout.contains("state=awaiting_review"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_logs_show_projected_history() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_script = root.join("fake-jj.sh");
        fs::write(
            &fake_script,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_script, perms).unwrap();
        }
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
                    commands = ["test -f execution.txt"]
                "#,
                script_path = fake_script.display(),
            ),
        )
        .unwrap();

        let (start_code, start_stdout, _) = run(
            &root,
            &[
                "run",
                "start",
                "TASK-4",
                "--adapter",
                "shell",
                "--prompt",
                "hello",
            ],
        );
        assert_eq!(start_code, 0);
        let run_id = start_stdout
            .split_whitespace()
            .find_map(|part| part.strip_prefix("run_id="))
            .unwrap()
            .to_string();

        let (launch_code, _, launch_stderr) = run(&root, &["launcher", "run-once", &run_id]);
        assert_eq!(launch_code, 0);
        assert!(launch_stderr.is_empty());

        let (logs_code, logs_stdout, logs_stderr) =
            run(&root, &["run", "logs", &run_id, "--limit", "10"]);
        assert_eq!(logs_code, 0);
        assert!(logs_stderr.is_empty());
        assert!(logs_stdout.contains("kind=run_requested"));
        assert!(logs_stdout.contains("kind=run_started"));
        assert!(logs_stdout.contains("event=command_completed"));
        assert!(logs_stdout.contains("event=evaluation_completed"));
        assert!(logs_stdout.contains("passed=true"));
        assert!(logs_stdout.contains("kind=run_awaiting_review"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn run_logs_support_json_output() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_script = root.join("fake-jj.sh");
        fs::write(
            &fake_script,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$1\" == \"lane-add\" ]]; then mkdir -p \"$4\"; exit 0; fi\necho unsupported >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_script).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_script, perms).unwrap();
        }
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
                    commands = ["test -f execution.txt"]
                "#,
                script_path = fake_script.display(),
            ),
        )
        .unwrap();

        let (start_code, start_stdout, _) = run(
            &root,
            &[
                "run",
                "start",
                "TASK-LOG-JSON",
                "--adapter",
                "shell",
                "--prompt",
                "hello json",
            ],
        );
        assert_eq!(start_code, 0);
        let run_id = start_stdout
            .split_whitespace()
            .find_map(|part| part.strip_prefix("run_id="))
            .unwrap()
            .to_string();

        let (launch_code, _, launch_stderr) = run(&root, &["launcher", "run-once", &run_id]);
        assert_eq!(launch_code, 0);
        assert!(launch_stderr.is_empty());

        let (logs_code, logs_stdout, logs_stderr) =
            run(&root, &["run", "logs", &run_id, "--limit", "10", "--json"]);
        assert_eq!(logs_code, 0);
        assert!(logs_stderr.is_empty());
        let logs_json: Value = serde_json::from_str(&logs_stdout).unwrap();
        assert_eq!(logs_json["run_id"], run_id);
        assert!(logs_json["logs"].as_array().unwrap().len() >= 4);

        fs::remove_dir_all(root).unwrap();
    }
}
