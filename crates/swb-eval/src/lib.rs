use std::path::Path;
use std::process::Command;

use thiserror::Error;
use swb_config::EvaluatorConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationCommandResult {
    pub command: String,
    pub success: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationOutcome {
    pub passed: bool,
    pub results: Vec<EvaluationCommandResult>,
}

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("failed to execute evaluator command '{command}': {error}")]
    CommandLaunch {
        command: String,
        error: std::io::Error,
    },
}

pub fn run_evaluator(
    workspace_root: impl AsRef<Path>,
    evaluator: &EvaluatorConfig,
) -> Result<EvaluationOutcome, EvalError> {
    let mut results = Vec::new();
    for command in &evaluator.commands {
        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(workspace_root.as_ref())
            .output()
            .map_err(|error| EvalError::CommandLaunch {
                command: command.clone(),
                error,
            })?;
        let result = EvaluationCommandResult {
            command: command.clone(),
            success: output.status.success(),
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        };
        results.push(result);
    }

    let passed = results.iter().all(|result| result.success);
    Ok(EvaluationOutcome { passed, results })
}

#[cfg(test)]
mod tests {
    use swb_config::EvaluatorConfig;

    use super::run_evaluator;

    #[test]
    fn evaluator_runs_shell_commands() {
        let evaluator = EvaluatorConfig {
            name: "smoke".to_string(),
            commands: vec!["printf swb-eval".to_string()],
        };
        let outcome = run_evaluator(".", &evaluator).unwrap();
        assert!(outcome.passed);
        assert_eq!(outcome.results[0].stdout, "swb-eval");
    }
}
