use std::env;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::Serialize;
use serde_json::{json, Value};
use swb_config::{AdapterCapabilities, AdapterConfig, AuthStrategy, PromptMode, SwbConfig};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisteredAdapter {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub auth_strategy: AuthStrategy,
    pub prompt_mode: PromptMode,
    pub capabilities: AdapterCapabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterDoctorReport {
    pub name: String,
    pub command: String,
    pub available: bool,
    pub logged_in: Option<bool>,
    pub auth_method: Option<String>,
    pub login_supported: bool,
    pub device_login_supported: bool,
    pub login_command: Option<String>,
    pub device_login_command: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterLoginMode {
    Default,
    Device,
}

impl Display for AdapterLoginMode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Default => "default",
            Self::Device => "device",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterLoginResult {
    pub name: String,
    pub mode: String,
    pub available: bool,
    pub success: bool,
    pub exit_code: i32,
    pub command: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterRunContext<'a> {
    pub repo_root: &'a Path,
    pub workspace_root: &'a Path,
    pub run_id: &'a str,
    pub task_id: &'a str,
    pub workflow: &'a str,
    pub prompt: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterExecutionResult {
    pub adapter: String,
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
}

#[derive(Debug, Error)]
pub enum AdapterExecutionError {
    #[error("adapter command '{command}' could not be launched: {error}")]
    CommandLaunch {
        command: String,
        error: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum AdapterAuthError {
    #[error("adapter not found: {0}")]
    MissingAdapter(String),
    #[error("adapter '{name}' does not support {mode} login")]
    LoginUnsupported {
        name: String,
        mode: AdapterLoginMode,
    },
    #[error("adapter auth command '{command}' could not be launched: {error}")]
    CommandLaunch {
        command: String,
        error: std::io::Error,
    },
}

pub fn registry_from_config(config: &SwbConfig) -> Vec<RegisteredAdapter> {
    config
        .adapters
        .iter()
        .map(|adapter| RegisteredAdapter {
            name: adapter.name.clone(),
            command: adapter.command.clone(),
            args: adapter.args.clone(),
            auth_strategy: adapter.auth_strategy.clone(),
            prompt_mode: adapter.prompt_mode.clone(),
            capabilities: adapter.capabilities.clone(),
        })
        .collect()
}

pub fn doctor_from_config(config: &SwbConfig) -> Vec<AdapterDoctorReport> {
    config
        .adapters
        .iter()
        .map(run_adapter_doctor)
        .collect::<Vec<_>>()
}

pub fn auth_status_from_config(
    config: &SwbConfig,
    requested: Option<&str>,
) -> Result<Vec<AdapterDoctorReport>, AdapterAuthError> {
    match requested {
        Some(name) => Ok(vec![run_adapter_doctor(
            config
                .find_adapter(name)
                .ok_or_else(|| AdapterAuthError::MissingAdapter(name.to_string()))?,
        )]),
        None => Ok(doctor_from_config(config)),
    }
}

pub fn login_adapter(
    config: &SwbConfig,
    adapter_name: &str,
    mode: AdapterLoginMode,
) -> Result<AdapterLoginResult, AdapterAuthError> {
    let adapter = config
        .find_adapter(adapter_name)
        .ok_or_else(|| AdapterAuthError::MissingAdapter(adapter_name.to_string()))?;
    let available = command_is_available(&adapter.command);
    if !available {
        return Ok(AdapterLoginResult {
            name: adapter.name.clone(),
            mode: mode.to_string(),
            available: false,
            success: false,
            exit_code: -1,
            command: None,
            stdout: String::new(),
            stderr: String::new(),
            detail: "command not found in PATH".to_string(),
        });
    }

    let Some(args) = login_args_for(adapter, mode) else {
        return Err(AdapterAuthError::LoginUnsupported {
            name: adapter.name.clone(),
            mode,
        });
    };
    let command_text = format!("{} {}", adapter.command, args.join(" "));
    let output = Command::new(&adapter.command)
        .args(&args)
        .output()
        .map_err(|error| AdapterAuthError::CommandLaunch {
            command: command_text.clone(),
            error,
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr.clone()
    } else if !stdout.is_empty() {
        stdout.clone()
    } else if output.status.success() {
        "login command completed".to_string()
    } else {
        "login command failed".to_string()
    };

    Ok(AdapterLoginResult {
        name: adapter.name.clone(),
        mode: mode.to_string(),
        available: true,
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        command: Some(command_text),
        stdout,
        stderr,
        detail,
    })
}

pub fn execute_adapter(
    adapter: &AdapterConfig,
    context: &AdapterRunContext<'_>,
) -> Result<AdapterExecutionResult, AdapterExecutionError> {
    let mut command = Command::new(&adapter.command);
    command.args(&adapter.args);
    if adapter.prompt_mode == PromptMode::ArgvLast {
        if let Some(prompt) = context.prompt {
            command.arg(prompt);
        }
    }

    command.current_dir(context.workspace_root);
    command.env("SWB_REPO_ROOT", context.repo_root);
    command.env("SWB_WORKSPACE_ROOT", context.workspace_root);
    command.env("SWB_RUN_ID", context.run_id);
    command.env("SWB_TASK_ID", context.task_id);
    command.env("SWB_WORKFLOW", context.workflow);
    command.env("SWB_ADAPTER", &adapter.name);
    if let Some(prompt) = context.prompt {
        command.env("SWB_PROMPT", prompt);
    }

    let started_at = Instant::now();
    let output = command
        .output()
        .map_err(|error| AdapterExecutionError::CommandLaunch {
            command: adapter.command.clone(),
            error,
        })?;
    let duration_ms = started_at.elapsed().as_millis();

    Ok(AdapterExecutionResult {
        adapter: adapter.name.clone(),
        success: output.status.success(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        duration_ms,
    })
}

pub fn execution_result_payload(result: &AdapterExecutionResult) -> Value {
    json!({
        "adapter": result.adapter,
        "success": result.success,
        "exit_code": result.exit_code,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "duration_ms": result.duration_ms
    })
}

fn run_adapter_doctor(adapter: &AdapterConfig) -> AdapterDoctorReport {
    let available = command_is_available(&adapter.command);
    let login_command = login_args_for(adapter, AdapterLoginMode::Default)
        .map(|args| format!("{} {}", adapter.command, args.join(" ")));
    let device_login_command = login_args_for(adapter, AdapterLoginMode::Device)
        .map(|args| format!("{} {}", adapter.command, args.join(" ")));
    let mut report = AdapterDoctorReport {
        name: adapter.name.clone(),
        command: adapter.command.clone(),
        available,
        logged_in: None,
        auth_method: None,
        login_supported: login_command.is_some(),
        device_login_supported: device_login_command.is_some(),
        login_command,
        device_login_command,
        detail: if available {
            "command found".to_string()
        } else {
            "command not found in PATH".to_string()
        },
    };

    if !available {
        return report;
    }

    let Some(status_args) = status_args_for(adapter) else {
        return report;
    };

    let output = Command::new(&adapter.command).args(&status_args).output();
    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let status_text = if stderr.is_empty() {
                stdout.clone()
            } else if stdout.is_empty() {
                stderr.clone()
            } else {
                format!("{stdout}\n{stderr}")
            };
            let normalized = status_text.to_ascii_lowercase();
            report.logged_in = Some(login_status_from_output(
                adapter,
                &normalized,
                output.status.success(),
            ));
            report.auth_method = parse_auth_method(adapter, &normalized);
            report.detail = if status_text.is_empty() {
                format!("{} returned no output", status_command_label(adapter))
            } else {
                status_text
            };
        }
        Err(error) => {
            report.logged_in = Some(false);
            report.detail = format!("failed to run {}: {error}", status_command_label(adapter));
        }
    }

    report
}

fn command_is_available(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return PathBuf::from(command).is_file();
    }

    env::var_os("PATH")
        .into_iter()
        .flat_map(|value| env::split_paths(&value).collect::<Vec<_>>())
        .map(|entry| entry.join(command))
        .any(|candidate| candidate.is_file())
}

fn status_args_for(adapter: &AdapterConfig) -> Option<Vec<String>> {
    match adapter.auth_strategy {
        AuthStrategy::None => None,
        AuthStrategy::CodexLoginStatus => {
            if adapter.auth_status_args.is_empty() {
                Some(vec!["login".to_string(), "status".to_string()])
            } else {
                Some(adapter.auth_status_args.clone())
            }
        }
        AuthStrategy::CommandStatus => {
            if adapter.auth_status_args.is_empty() {
                None
            } else {
                Some(adapter.auth_status_args.clone())
            }
        }
    }
}

fn login_args_for(adapter: &AdapterConfig, mode: AdapterLoginMode) -> Option<Vec<String>> {
    match mode {
        AdapterLoginMode::Default => {
            if !adapter.auth_login_args.is_empty() {
                return Some(adapter.auth_login_args.clone());
            }
        }
        AdapterLoginMode::Device => {
            if !adapter.auth_login_device_args.is_empty() {
                return Some(adapter.auth_login_device_args.clone());
            }
        }
    }

    match (adapter.auth_strategy.clone(), mode) {
        (AuthStrategy::CodexLoginStatus, AdapterLoginMode::Default) => {
            Some(vec!["login".to_string()])
        }
        (AuthStrategy::CodexLoginStatus, AdapterLoginMode::Device) => {
            Some(vec!["login".to_string(), "--device-auth".to_string()])
        }
        _ => None,
    }
}

fn login_status_from_output(
    adapter: &AdapterConfig,
    status_text: &str,
    command_succeeded: bool,
) -> bool {
    let logged_out_markers = [
        "not logged in",
        "logged out",
        "login required",
        "unauthorized",
    ];
    match adapter.auth_strategy {
        AuthStrategy::None => false,
        AuthStrategy::CodexLoginStatus | AuthStrategy::CommandStatus => {
            command_succeeded
                && !logged_out_markers
                    .iter()
                    .any(|marker| status_text.contains(marker))
        }
    }
}

fn parse_auth_method(adapter: &AdapterConfig, status_text: &str) -> Option<String> {
    match adapter.auth_strategy {
        AuthStrategy::CodexLoginStatus => {
            if status_text.contains("chatgpt") {
                Some("chatgpt".to_string())
            } else if status_text.contains("api key") {
                Some("api_key".to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn status_command_label(adapter: &AdapterConfig) -> String {
    let args = status_args_for(adapter).unwrap_or_default();
    if args.is_empty() {
        adapter.command.clone()
    } else {
        format!("{} {}", adapter.command, args.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use swb_config::{AdapterConfig, SwbConfig};

    use super::{doctor_from_config, login_adapter, AdapterLoginMode};

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-adapters-test-{serial}"))
    }

    #[test]
    fn codex_doctor_reports_logged_in_status() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_codex = root.join("codex");
        fs::write(
            &fake_codex,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$*\" == \"login status\" ]]; then echo \"Logged in using ChatGPT\"; exit 0; fi\necho unexpected >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_codex).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_codex, perms).unwrap();
        }

        let config = SwbConfig {
            adapters: vec![AdapterConfig {
                command: fake_codex.display().to_string(),
                ..AdapterConfig::default()
            }],
            ..SwbConfig::default()
        };

        let reports = doctor_from_config(&config);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].logged_in, Some(true));
        assert_eq!(reports[0].auth_method.as_deref(), Some("chatgpt"));
        assert!(reports[0].device_login_supported);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn codex_device_login_runs_configured_command() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let fake_codex = root.join("codex");
        fs::write(
            &fake_codex,
            "#!/usr/bin/env bash\nset -euo pipefail\nif [[ \"$*\" == \"login --device-auth\" ]]; then echo \"Open browser and enter code\"; exit 0; fi\necho unexpected >&2\nexit 2\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake_codex).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake_codex, perms).unwrap();
        }

        let config = SwbConfig {
            adapters: vec![AdapterConfig {
                command: fake_codex.display().to_string(),
                ..AdapterConfig::default()
            }],
            ..SwbConfig::default()
        };

        let result = login_adapter(&config, "codex", AdapterLoginMode::Device).unwrap();
        assert!(result.success);
        assert!(result.stdout.contains("Open browser"));
        assert_eq!(result.mode, "device");

        fs::remove_dir_all(root).unwrap();
    }
}
