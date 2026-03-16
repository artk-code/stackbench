use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use swb_core::SwbPaths;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthStrategy {
    #[default]
    None,
    CodexLoginStatus,
    CommandStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    #[default]
    ArgvLast,
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdapterCapabilities {
    pub streaming: bool,
    pub cancellation: bool,
    pub artifacts: bool,
    pub auth: bool,
}

impl Default for AdapterCapabilities {
    fn default() -> Self {
        Self {
            streaming: false,
            cancellation: false,
            artifacts: true,
            auth: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdapterConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub auth_strategy: AuthStrategy,
    pub auth_status_args: Vec<String>,
    pub auth_login_args: Vec<String>,
    pub auth_login_device_args: Vec<String>,
    pub prompt_mode: PromptMode,
    pub capabilities: AdapterCapabilities,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            name: "codex".to_string(),
            command: "codex".to_string(),
            args: vec!["exec".to_string(), "--skip-git-repo-check".to_string()],
            auth_strategy: AuthStrategy::CodexLoginStatus,
            auth_status_args: vec!["login".to_string(), "status".to_string()],
            auth_login_args: vec!["login".to_string()],
            auth_login_device_args: vec!["login".to_string(), "--device-auth".to_string()],
            prompt_mode: PromptMode::ArgvLast,
            capabilities: AdapterCapabilities {
                streaming: true,
                cancellation: true,
                artifacts: true,
                auth: true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkflowConfig {
    pub name: String,
    pub adapters: Vec<String>,
}

impl Default for WorkflowConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            adapters: vec!["codex".to_string(), "claude_code".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EvaluatorConfig {
    pub name: String,
    pub commands: Vec<String>,
}

impl Default for EvaluatorConfig {
    fn default() -> Self {
        Self {
            name: "repo_checks".to_string(),
            commands: vec!["cargo test --workspace".to_string()],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IntegrationConfig {
    pub script_path: String,
    pub jj_bin: String,
    pub base_revset: String,
}

impl Default for IntegrationConfig {
    fn default() -> Self {
        Self {
            script_path: "scripts/swb-jj.sh".to_string(),
            jj_bin: "jj".to_string(),
            base_revset: "trunk()".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RepoConfig {
    pub default_workflow: String,
}

impl Default for RepoConfig {
    fn default() -> Self {
        Self {
            default_workflow: "default".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SwbConfig {
    pub repo: RepoConfig,
    pub adapters: Vec<AdapterConfig>,
    pub workflows: Vec<WorkflowConfig>,
    pub evaluators: Vec<EvaluatorConfig>,
    pub integration: IntegrationConfig,
}

impl Default for SwbConfig {
    fn default() -> Self {
        Self {
            repo: RepoConfig::default(),
            adapters: vec![
                AdapterConfig::default(),
                AdapterConfig {
                    name: "claude_code".to_string(),
                    command: "claude".to_string(),
                    args: Vec::new(),
                    auth_strategy: AuthStrategy::None,
                    auth_status_args: Vec::new(),
                    auth_login_args: Vec::new(),
                    auth_login_device_args: Vec::new(),
                    prompt_mode: PromptMode::ArgvLast,
                    capabilities: AdapterCapabilities {
                        streaming: true,
                        cancellation: true,
                        artifacts: true,
                        auth: false,
                    },
                },
            ],
            workflows: vec![WorkflowConfig::default()],
            evaluators: vec![EvaluatorConfig::default()],
            integration: IntegrationConfig::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("workflow not found: {0}")]
    UnknownWorkflow(String),
    #[error("adapter not found: {0}")]
    UnknownAdapter(String),
    #[error("workflow references unknown adapter '{adapter}'")]
    UnknownWorkflowAdapter { adapter: String },
    #[error("workflow '{workflow}' does not allow adapter '{adapter}'")]
    AdapterNotAllowed { workflow: String, adapter: String },
}

impl SwbConfig {
    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let paths = SwbPaths::new(root);
        if !paths.config_path.exists() {
            return Ok(Self::default());
        }

        let raw = fs::read_to_string(&paths.config_path)?;
        let config = toml::from_str::<Self>(&raw)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        for workflow in &self.workflows {
            for adapter in &workflow.adapters {
                if self.find_adapter(adapter).is_none() {
                    return Err(ConfigError::UnknownWorkflowAdapter {
                        adapter: adapter.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    pub fn resolve_workflow(
        &self,
        requested: Option<&str>,
    ) -> Result<&WorkflowConfig, ConfigError> {
        let name = requested.unwrap_or(&self.repo.default_workflow);
        self.workflows
            .iter()
            .find(|workflow| workflow.name == name)
            .ok_or_else(|| ConfigError::UnknownWorkflow(name.to_string()))
    }

    pub fn resolve_adapter<'a>(
        &'a self,
        workflow: &'a WorkflowConfig,
        requested: Option<&str>,
    ) -> Result<&'a AdapterConfig, ConfigError> {
        let adapter_name = match requested {
            Some(value) => value.to_string(),
            None => workflow
                .adapters
                .first()
                .cloned()
                .ok_or_else(|| ConfigError::UnknownAdapter("<none>".to_string()))?,
        };

        if !workflow
            .adapters
            .iter()
            .any(|candidate| candidate == &adapter_name)
        {
            return Err(ConfigError::AdapterNotAllowed {
                workflow: workflow.name.clone(),
                adapter: adapter_name,
            });
        }

        self.find_adapter(&adapter_name)
            .ok_or(ConfigError::UnknownAdapter(adapter_name))
    }

    pub fn find_adapter(&self, name: &str) -> Option<&AdapterConfig> {
        self.adapters.iter().find(|adapter| adapter.name == name)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{AuthStrategy, ConfigError, SwbConfig};

    fn unique_temp_root() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("swb-config-test-{serial}"))
    }

    #[test]
    fn missing_config_uses_defaults() {
        let root = unique_temp_root();
        let config = SwbConfig::load_from_root(&root).unwrap();
        assert_eq!(config.repo.default_workflow, "default");
        assert_eq!(config.adapters.len(), 2);
    }

    #[test]
    fn load_custom_config_file() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("swb.toml"),
            r#"
                [repo]
                default_workflow = "review"

                [[adapters]]
                name = "codex"
                command = "codex"
                auth_strategy = "codex_login_status"

                [[workflows]]
                name = "review"
                adapters = ["codex"]
            "#,
        )
        .unwrap();

        let config = SwbConfig::load_from_root(&root).unwrap();
        let workflow = config.resolve_workflow(None).unwrap();
        let adapter = config.resolve_adapter(workflow, None).unwrap();
        assert_eq!(workflow.name, "review");
        assert_eq!(adapter.auth_strategy, AuthStrategy::CodexLoginStatus);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workflow_validation_rejects_missing_adapter() {
        let config = SwbConfig {
            workflows: vec![super::WorkflowConfig {
                name: "default".to_string(),
                adapters: vec!["missing".to_string()],
            }],
            ..SwbConfig::default()
        };

        let error = config.validate().unwrap_err();
        assert!(matches!(error, ConfigError::UnknownWorkflowAdapter { .. }));
    }
}
