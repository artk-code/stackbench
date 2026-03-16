use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;
use swb_config::SwbConfig;
use swb_core::SwbPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JjDoctorReport {
    pub jj_available: bool,
    pub script_path: PathBuf,
    pub script_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationResult {
    pub workspace_root: PathBuf,
    pub change_id: String,
    pub detail: String,
}

#[derive(Debug, Error)]
pub enum JjError {
    #[error("jj workspace is missing for run: {0}")]
    MissingWorkspace(String),
    #[error("jj script is missing: {0}")]
    MissingScript(String),
    #[error("failed to execute jj script: {0}")]
    ScriptLaunch(#[from] std::io::Error),
    #[error("jj script failed: {0}")]
    ScriptFailure(String),
    #[error("failed to execute jj binary: {0}")]
    JjLaunch(std::io::Error),
    #[error("jj command failed: {0}")]
    JjFailure(String),
}

pub fn doctor(root: impl AsRef<Path>, config: &SwbConfig) -> JjDoctorReport {
    let script_path = resolve_script_path(root, config);
    JjDoctorReport {
        jj_available: command_is_available(&config.integration.jj_bin),
        script_available: script_path.is_file(),
        script_path,
    }
}

pub fn workspace_root_for_run(root: impl AsRef<Path>, run_id: &str) -> PathBuf {
    SwbPaths::new(root)
        .data_dir
        .join("workspaces")
        .join(run_id)
}

pub fn ensure_run_workspace(
    root: impl AsRef<Path>,
    config: &SwbConfig,
    run_id: &str,
) -> Result<PathBuf, JjError> {
    let root = root.as_ref();
    let workspace_root = workspace_root_for_run(root, run_id);
    if workspace_root.exists() {
        return Ok(workspace_root);
    }

    if let Some(parent) = workspace_root.parent() {
        fs::create_dir_all(parent).map_err(JjError::ScriptLaunch)?;
    }

    let script_path = resolve_script_path(root, config);
    if !script_path.is_file() {
        return Err(JjError::MissingScript(script_path.display().to_string()));
    }

    let output = Command::new(&script_path)
        .args([
            "lane-add",
            run_id,
            &config.integration.base_revset,
            workspace_root.to_string_lossy().as_ref(),
        ])
        .current_dir(root)
        .output()?;
    if !output.status.success() {
        return Err(JjError::ScriptFailure(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(workspace_root)
}

pub fn integrate_run(
    root: impl AsRef<Path>,
    config: &SwbConfig,
    run_id: &str,
    message: Option<&str>,
) -> Result<IntegrationResult, JjError> {
    let root = root.as_ref();
    let workspace_root = workspace_root_for_run(root, run_id);
    if !workspace_root.exists() {
        return Err(JjError::MissingWorkspace(run_id.to_string()));
    }

    let change_id = current_change_id(&workspace_root, &config.integration.jj_bin)?;
    let script_path = resolve_script_path(root, config);
    if !script_path.is_file() {
        return Err(JjError::MissingScript(script_path.display().to_string()));
    }

    let mut command = Command::new(&script_path);
    command
        .current_dir(root)
        .arg("integrate")
        .arg("--base")
        .arg(&config.integration.base_revset)
        .arg("--good")
        .arg(&change_id);
    if let Some(message) = message {
        command.arg("--message").arg(message);
    }

    let output = command.output()?;
    if !output.status.success() {
        return Err(JjError::ScriptFailure(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(IntegrationResult {
        workspace_root,
        change_id,
        detail: String::from_utf8_lossy(&output.stdout).trim().to_string(),
    })
}

fn resolve_script_path(root: impl AsRef<Path>, config: &SwbConfig) -> PathBuf {
    let candidate = PathBuf::from(&config.integration.script_path);
    if candidate.is_absolute() {
        candidate
    } else {
        root.as_ref().join(candidate)
    }
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

fn current_change_id(workspace_root: &Path, jj_bin: &str) -> Result<String, JjError> {
    let output = Command::new(jj_bin)
        .current_dir(workspace_root)
        .args(["log", "-r", "@", "--no-graph", "-T", "change_id"])
        .output()
        .map_err(JjError::JjLaunch)?;
    if !output.status.success() {
        return Err(JjError::JjFailure(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
