use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swb_core::{RunRequest, SwbPaths};
use thiserror::Error;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct ProfileFrontMatter {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub workflow: Option<String>,
    pub adapter: Option<String>,
    pub gstack_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PersonaDraft {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub ingress: Option<String>,
    pub default_profile: String,
    pub default_workflow: Option<String>,
    pub default_adapter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ProfileDraft {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub workflow: Option<String>,
    pub adapter: Option<String>,
    pub gstack_id: Option<String>,
    pub instructions_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProfileDefinition {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub workflow: Option<String>,
    pub adapter: Option<String>,
    pub gstack_id: String,
    pub file_path: String,
    pub instructions_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PersonaDefinition {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub ingress: Option<String>,
    pub default_profile: String,
    pub default_workflow: Option<String>,
    pub default_adapter: Option<String>,
    pub file_path: String,
}

impl Default for PersonaDefinition {
    fn default() -> Self {
        Self {
            id: String::new(),
            display_name: String::new(),
            description: String::new(),
            ingress: None,
            default_profile: String::new(),
            default_workflow: None,
            default_adapter: None,
            file_path: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedGstackLayer {
    pub kind: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedProfileExecution {
    pub profile_id: String,
    pub display_name: String,
    pub description: String,
    pub workflow: String,
    pub adapter: String,
    pub gstack_id: String,
    pub gstack_fingerprint: String,
    pub prompt: String,
    pub layers: Vec<ResolvedGstackLayer>,
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
    #[error("failed to serialize profile document: {0}")]
    ProfileSerialize(#[from] toml::ser::Error),
    #[error("workflow not found: {0}")]
    UnknownWorkflow(String),
    #[error("adapter not found: {0}")]
    UnknownAdapter(String),
    #[error("profile not found: {0}")]
    UnknownProfile(String),
    #[error("persona not found: {0}")]
    UnknownPersona(String),
    #[error("invalid profile id: {0}")]
    InvalidProfileId(String),
    #[error("invalid persona id: {0}")]
    InvalidPersonaId(String),
    #[error("profile '{path}' is missing TOML front matter")]
    MissingProfileFrontMatter { path: String },
    #[error("failed to parse profile '{path}': {error}")]
    ProfileParse { path: String, error: String },
    #[error("failed to parse persona '{path}': {error}")]
    PersonaParse { path: String, error: String },
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

    pub fn build_run_request(
        &self,
        root: impl AsRef<Path>,
        task_id: &str,
        workflow_override: Option<&str>,
        adapter_override: Option<&str>,
        persona_id: Option<&str>,
        profile_id: Option<&str>,
        prompt: Option<String>,
    ) -> Result<RunRequest, ConfigError> {
        if let Some(persona_id) = persona_id {
            let persona = get_persona_from_root(&root, persona_id, None)?;
            let effective_profile_id = profile_id.unwrap_or(&persona.default_profile);
            let resolved = self.resolve_profile_execution(
                root,
                effective_profile_id,
                workflow_override.or(persona.default_workflow.as_deref()),
                adapter_override.or(persona.default_adapter.as_deref()),
                prompt.as_deref(),
            )?;
            return Ok(RunRequest::with_gstack(
                task_id,
                &resolved.workflow,
                &resolved.adapter,
                Some(resolved.prompt),
                Some(resolved.profile_id),
                Some(persona.id),
                Some(resolved.gstack_id),
                Some(resolved.gstack_fingerprint),
            ));
        }

        if let Some(profile_id) = profile_id {
            let resolved = self.resolve_profile_execution(
                root,
                profile_id,
                workflow_override,
                adapter_override,
                prompt.as_deref(),
            )?;
            return Ok(RunRequest::with_gstack(
                task_id,
                &resolved.workflow,
                &resolved.adapter,
                Some(resolved.prompt),
                Some(resolved.profile_id),
                None,
                Some(resolved.gstack_id),
                Some(resolved.gstack_fingerprint),
            ));
        }

        let workflow = self.resolve_workflow(workflow_override)?;
        let adapter = self.resolve_adapter(workflow, adapter_override)?;
        Ok(RunRequest::new(
            task_id,
            &workflow.name,
            &adapter.name,
            prompt,
        ))
    }

    pub fn resolve_profile_execution(
        &self,
        root: impl AsRef<Path>,
        profile_id: &str,
        workflow_override: Option<&str>,
        adapter_override: Option<&str>,
        task_prompt: Option<&str>,
    ) -> Result<ResolvedProfileExecution, ConfigError> {
        let root = root.as_ref();
        let profile = get_profile_from_root(root, profile_id)?;
        let workflow = self.resolve_workflow(workflow_override.or(profile.workflow.as_deref()))?;
        let adapter =
            self.resolve_adapter(workflow, adapter_override.or(profile.adapter.as_deref()))?;
        let runtime_layer = load_runtime_layer(root)?;
        let task_prompt = task_prompt.map(str::trim).filter(|value| !value.is_empty());
        let prompt = compose_profile_prompt(&runtime_layer.content, &profile, task_prompt);
        let mut fingerprint_layers = vec![runtime_layer];
        fingerprint_layers.push(ProfileLayer {
            kind: "role".to_string(),
            source: relative_repo_path(root, &profile_path(root, &profile.id))
                .unwrap_or_else(|| format!("swb/profiles/{}.md", profile.id)),
            content: profile.instructions_markdown.clone(),
        });
        if let Some(task_prompt) = task_prompt {
            fingerprint_layers.push(ProfileLayer {
                kind: "task".to_string(),
                source: "operator_input".to_string(),
                content: task_prompt.to_string(),
            });
        }

        Ok(ResolvedProfileExecution {
            profile_id: profile.id.clone(),
            display_name: profile.display_name.clone(),
            description: profile.description.clone(),
            workflow: workflow.name.clone(),
            adapter: adapter.name.clone(),
            gstack_id: profile.gstack_id.clone(),
            gstack_fingerprint: fingerprint_gstack(
                &adapter.name,
                &workflow.name,
                &fingerprint_layers,
            ),
            prompt,
            layers: fingerprint_layers
                .into_iter()
                .map(|layer| ResolvedGstackLayer {
                    kind: layer.kind,
                    source: layer.source,
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileLayer {
    kind: String,
    source: String,
    content: String,
}

pub fn list_profiles_from_root(
    root: impl AsRef<Path>,
) -> Result<Vec<ProfileDefinition>, ConfigError> {
    let root = root.as_ref();
    let paths = SwbPaths::new(root);
    if !paths.profiles_dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    for entry in fs::read_dir(&paths.profiles_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        items.push(load_profile_file(root, &path)?);
    }

    items.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(items)
}

pub fn get_profile_from_root(
    root: impl AsRef<Path>,
    profile_id: &str,
) -> Result<ProfileDefinition, ConfigError> {
    validate_profile_id(profile_id)?;
    let path = profile_path(root.as_ref(), profile_id);
    if !path.exists() {
        return Err(ConfigError::UnknownProfile(profile_id.to_string()));
    }
    load_profile_file(root.as_ref(), &path)
}

pub fn list_personas_from_root(
    root: impl AsRef<Path>,
    ingress: Option<&str>,
) -> Result<Vec<PersonaDefinition>, ConfigError> {
    let root = root.as_ref();
    let base_dir = personas_base_dir(root, ingress);
    if !base_dir.exists() {
        return Ok(Vec::new());
    }

    let mut items = Vec::new();
    collect_personas(root, &base_dir, &mut items)?;
    items.sort_by(|left, right| {
        left.display_name
            .to_ascii_lowercase()
            .cmp(&right.display_name.to_ascii_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(items)
}

pub fn get_persona_from_root(
    root: impl AsRef<Path>,
    persona_id: &str,
    ingress: Option<&str>,
) -> Result<PersonaDefinition, ConfigError> {
    validate_persona_id(persona_id)?;
    let root = root.as_ref();
    let direct = persona_path(root, persona_id, ingress);
    if direct.exists() {
        return load_persona_file(root, &direct);
    }

    if ingress.is_none() {
        for candidate in list_personas_from_root(root, None)? {
            if candidate.id == persona_id {
                return Ok(candidate);
            }
        }
    }

    Err(ConfigError::UnknownPersona(persona_id.to_string()))
}

pub fn save_persona_to_root(
    root: impl AsRef<Path>,
    draft: &PersonaDraft,
) -> Result<PersonaDefinition, ConfigError> {
    validate_persona_id(&draft.id)?;
    let root = root.as_ref();
    let path = persona_path(root, &draft.id, draft.ingress.as_deref());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, render_persona_document(draft)?)?;
    get_persona_from_root(root, &draft.id, draft.ingress.as_deref())
}

pub fn save_profile_to_root(
    root: impl AsRef<Path>,
    draft: &ProfileDraft,
) -> Result<ProfileDefinition, ConfigError> {
    validate_profile_id(&draft.id)?;
    let root = root.as_ref();
    let path = profile_path(root, &draft.id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, render_profile_document(draft)?)?;
    get_profile_from_root(root, &draft.id)
}

fn load_profile_file(root: &Path, path: &Path) -> Result<ProfileDefinition, ConfigError> {
    let raw = fs::read_to_string(path)?;
    let (front_matter, instructions_markdown) = parse_profile_document(path, &raw)?;
    validate_profile_id(&front_matter.id)?;
    Ok(ProfileDefinition {
        id: front_matter.id.clone(),
        display_name: default_display_name(&front_matter.id, &front_matter.display_name),
        description: front_matter.description,
        workflow: front_matter.workflow,
        adapter: front_matter.adapter,
        gstack_id: front_matter
            .gstack_id
            .unwrap_or_else(|| format!("profile.{}", front_matter.id)),
        file_path: relative_repo_path(root, path).unwrap_or_else(|| path.display().to_string()),
        instructions_markdown,
    })
}

fn parse_profile_document(
    path: &Path,
    raw: &str,
) -> Result<(ProfileFrontMatter, String), ConfigError> {
    let normalized = raw.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("+++\n") else {
        return Err(ConfigError::MissingProfileFrontMatter {
            path: path.display().to_string(),
        });
    };
    let Some((front_matter, body)) = rest.split_once("\n+++\n") else {
        return Err(ConfigError::MissingProfileFrontMatter {
            path: path.display().to_string(),
        });
    };
    let front_matter = toml::from_str::<ProfileFrontMatter>(front_matter).map_err(|error| {
        ConfigError::ProfileParse {
            path: path.display().to_string(),
            error: error.to_string(),
        }
    })?;
    Ok((
        front_matter,
        body.trim_start_matches('\n').trim_end().to_string(),
    ))
}

fn render_profile_document(draft: &ProfileDraft) -> Result<String, ConfigError> {
    let front_matter = ProfileFrontMatter {
        id: draft.id.clone(),
        display_name: draft.display_name.clone(),
        description: draft.description.clone(),
        workflow: draft.workflow.clone(),
        adapter: draft.adapter.clone(),
        gstack_id: draft.gstack_id.clone(),
    };
    let front_matter = toml::to_string_pretty(&front_matter)?;
    let instructions = draft.instructions_markdown.trim_end();
    let mut rendered = String::new();
    rendered.push_str("+++\n");
    rendered.push_str(front_matter.trim_end());
    rendered.push_str("\n+++\n\n");
    rendered.push_str(instructions);
    rendered.push('\n');
    Ok(rendered)
}

fn collect_personas(
    root: &Path,
    dir: &Path,
    items: &mut Vec<PersonaDefinition>,
) -> Result<(), ConfigError> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_personas(root, &path, items)?;
            continue;
        }
        if !path.is_file() || path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        items.push(load_persona_file(root, &path)?);
    }
    Ok(())
}

fn load_persona_file(root: &Path, path: &Path) -> Result<PersonaDefinition, ConfigError> {
    let raw = fs::read_to_string(path)?;
    let persona =
        toml::from_str::<PersonaDraft>(&raw).map_err(|error| ConfigError::PersonaParse {
            path: path.display().to_string(),
            error: error.to_string(),
        })?;
    validate_persona_id(&persona.id)?;
    if persona.default_profile.trim().is_empty() {
        return Err(ConfigError::PersonaParse {
            path: path.display().to_string(),
            error: "default_profile is required".to_string(),
        });
    }

    Ok(PersonaDefinition {
        id: persona.id.clone(),
        display_name: default_display_name(&persona.id, &persona.display_name),
        description: persona.description,
        ingress: persona.ingress,
        default_profile: persona.default_profile,
        default_workflow: persona.default_workflow,
        default_adapter: persona.default_adapter,
        file_path: relative_repo_path(root, path).unwrap_or_else(|| path.display().to_string()),
    })
}

fn render_persona_document(draft: &PersonaDraft) -> Result<String, ConfigError> {
    Ok(toml::to_string_pretty(draft)?)
}

fn load_runtime_layer(root: &Path) -> Result<ProfileLayer, ConfigError> {
    let paths = SwbPaths::new(root);
    let runtime_path = paths.runtime_prompts_dir.join("default.md");
    if runtime_path.exists() {
        return Ok(ProfileLayer {
            kind: "runtime".to_string(),
            source: relative_repo_path(root, &runtime_path)
                .unwrap_or_else(|| "swb/prompts/runtime/default.md".to_string()),
            content: fs::read_to_string(runtime_path)?,
        });
    }

    Ok(ProfileLayer {
        kind: "runtime".to_string(),
        source: "builtin:runtime/default".to_string(),
        content: DEFAULT_RUNTIME_PROMPT.to_string(),
    })
}

fn compose_profile_prompt(
    runtime_prompt: &str,
    profile: &ProfileDefinition,
    task_prompt: Option<&str>,
) -> String {
    let mut prompt = String::new();
    if !runtime_prompt.trim().is_empty() {
        push_prompt_section(&mut prompt, "Stackbench Runtime", runtime_prompt.trim());
    }

    let role_body = match (
        profile.description.trim().is_empty(),
        profile.instructions_markdown.trim().is_empty(),
    ) {
        (false, false) => format!(
            "{}\n\n{}",
            profile.description.trim(),
            profile.instructions_markdown.trim()
        ),
        (false, true) => profile.description.trim().to_string(),
        (true, false) => profile.instructions_markdown.trim().to_string(),
        (true, true) => String::new(),
    };
    push_prompt_section(
        &mut prompt,
        &format!("Worker Type: {}", profile.display_name),
        &role_body,
    );

    if let Some(task_prompt) = task_prompt {
        push_prompt_section(&mut prompt, "Requested Task", task_prompt);
    }

    prompt.trim().to_string()
}

fn push_prompt_section(prompt: &mut String, title: &str, body: &str) {
    if body.trim().is_empty() {
        return;
    }
    if !prompt.is_empty() {
        prompt.push_str("\n\n");
    }
    prompt.push_str("## ");
    prompt.push_str(title);
    prompt.push_str("\n");
    prompt.push_str(body.trim());
}

fn fingerprint_gstack(adapter: &str, workflow: &str, layers: &[ProfileLayer]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("adapter={adapter}\nworkflow={workflow}\n").as_bytes());
    for layer in layers {
        hasher.update(layer.kind.as_bytes());
        hasher.update(b"\n");
        hasher.update(layer.source.as_bytes());
        hasher.update(b"\n");
        hasher.update(layer.content.as_bytes());
        hasher.update(b"\n--\n");
    }
    format!("sha256:{}", hex_encode(&hasher.finalize()))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(b"0123456789abcdef"[(byte >> 4) as usize]));
        encoded.push(char::from(b"0123456789abcdef"[(byte & 0x0f) as usize]));
    }
    encoded
}

fn validate_profile_id(id: &str) -> Result<(), ConfigError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidProfileId(id.to_string()))
    }
}

fn validate_persona_id(id: &str) -> Result<(), ConfigError> {
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if valid {
        Ok(())
    } else {
        Err(ConfigError::InvalidPersonaId(id.to_string()))
    }
}

fn profile_path(root: &Path, profile_id: &str) -> PathBuf {
    SwbPaths::new(root)
        .profiles_dir
        .join(format!("{profile_id}.md"))
}

fn personas_base_dir(root: &Path, ingress: Option<&str>) -> PathBuf {
    let base = SwbPaths::new(root).personas_dir;
    match ingress.map(str::trim).filter(|value| !value.is_empty()) {
        Some(ingress) => base.join(ingress),
        None => base,
    }
}

fn persona_path(root: &Path, persona_id: &str, ingress: Option<&str>) -> PathBuf {
    personas_base_dir(root, ingress).join(format!("{persona_id}.toml"))
}

fn default_display_name(id: &str, display_name: &str) -> String {
    if display_name.trim().is_empty() {
        id.to_string()
    } else {
        display_name.trim().to_string()
    }
}

fn relative_repo_path(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

const DEFAULT_RUNTIME_PROMPT: &str = "You are operating inside Stackbench. Stay inside the current repository, explain important risks clearly, and leave reviewable changes behind.";

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{
        get_persona_from_root, get_profile_from_root, list_personas_from_root,
        list_profiles_from_root, save_persona_to_root, save_profile_to_root, AuthStrategy,
        ConfigError, PersonaDraft, ProfileDraft, SwbConfig,
    };

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

    #[test]
    fn save_and_list_profiles_round_trip() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let saved = save_profile_to_root(
            &root,
            &ProfileDraft {
                id: "eng-review".to_string(),
                display_name: "Engineering Review".to_string(),
                description: "Review code for defects.".to_string(),
                workflow: Some("default".to_string()),
                adapter: Some("codex".to_string()),
                gstack_id: None,
                instructions_markdown: "- Find bugs\n- Call out missing tests".to_string(),
            },
        )
        .unwrap();

        assert_eq!(saved.id, "eng-review");
        assert_eq!(saved.gstack_id, "profile.eng-review");

        let listed = list_profiles_from_root(&root).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].display_name, "Engineering Review");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_profile_execution_builds_prompt_and_gstack() {
        let root = unique_temp_root();
        fs::create_dir_all(root.join("swb/prompts/runtime")).unwrap();
        fs::write(
            root.join("swb/prompts/runtime/default.md"),
            "Use the repo and keep findings concrete.",
        )
        .unwrap();
        save_profile_to_root(
            &root,
            &ProfileDraft {
                id: "eng-review".to_string(),
                display_name: "Engineering Review".to_string(),
                description: "Review a change before merge.".to_string(),
                workflow: Some("default".to_string()),
                adapter: Some("codex".to_string()),
                gstack_id: Some("eng_review_v1".to_string()),
                instructions_markdown: "Focus on correctness and regression risk.".to_string(),
            },
        )
        .unwrap();

        let config = SwbConfig::default();
        let resolved = config
            .resolve_profile_execution(
                &root,
                "eng-review",
                None,
                None,
                Some("Review the current branch for integration risk."),
            )
            .unwrap();

        assert_eq!(resolved.profile_id, "eng-review");
        assert_eq!(resolved.gstack_id, "eng_review_v1");
        assert!(resolved.gstack_fingerprint.starts_with("sha256:"));
        assert!(resolved.prompt.contains("Stackbench Runtime"));
        assert!(resolved.prompt.contains("Worker Type: Engineering Review"));
        assert!(resolved
            .prompt
            .contains("Review the current branch for integration risk."));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_and_list_personas_round_trip() {
        let root = unique_temp_root();
        fs::create_dir_all(&root).unwrap();
        let saved = save_persona_to_root(
            &root,
            &PersonaDraft {
                id: "slack-review".to_string(),
                display_name: "Slack Review".to_string(),
                description: "Dispatches review requests from Slack.".to_string(),
                ingress: Some("slack".to_string()),
                default_profile: "eng-review".to_string(),
                default_workflow: Some("default".to_string()),
                default_adapter: Some("codex".to_string()),
            },
        )
        .unwrap();

        assert_eq!(saved.id, "slack-review");
        assert_eq!(saved.ingress.as_deref(), Some("slack"));

        let listed = list_personas_from_root(&root, Some("slack")).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].default_profile, "eng-review");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn get_persona_searches_nested_ingress_directories() {
        let root = unique_temp_root();
        fs::create_dir_all(root.join("swb/personas/slack")).unwrap();
        fs::write(
            root.join("swb/personas/slack/review.toml"),
            r#"
id = "review"
display_name = "Slack Review"
description = "Slack ingress review persona"
ingress = "slack"
default_profile = "eng-review"
"#,
        )
        .unwrap();

        let persona = get_persona_from_root(&root, "review", None).unwrap();
        assert_eq!(persona.file_path, "swb/personas/slack/review.toml");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn build_run_request_uses_persona_defaults() {
        let root = unique_temp_root();
        fs::create_dir_all(root.join("swb/prompts/runtime")).unwrap();
        fs::write(
            root.join("swb/prompts/runtime/default.md"),
            "Use the repo and keep findings concrete.",
        )
        .unwrap();
        save_profile_to_root(
            &root,
            &ProfileDraft {
                id: "eng-review".to_string(),
                display_name: "Engineering Review".to_string(),
                description: "Review code before merge.".to_string(),
                workflow: Some("default".to_string()),
                adapter: Some("codex".to_string()),
                gstack_id: Some("profile.eng-review".to_string()),
                instructions_markdown: "Focus on defects and regression risk.".to_string(),
            },
        )
        .unwrap();
        save_persona_to_root(
            &root,
            &PersonaDraft {
                id: "slack-review".to_string(),
                display_name: "Slack Review".to_string(),
                description: "Slack persona".to_string(),
                ingress: Some("slack".to_string()),
                default_profile: "eng-review".to_string(),
                default_workflow: None,
                default_adapter: None,
            },
        )
        .unwrap();

        let config = SwbConfig::default();
        let request = config
            .build_run_request(
                &root,
                "TASK-123",
                None,
                None,
                Some("slack-review"),
                None,
                Some("Review the issue from Slack".to_string()),
            )
            .unwrap();

        assert_eq!(request.profile_id.as_deref(), Some("eng-review"));
        assert_eq!(request.persona_id.as_deref(), Some("slack-review"));
        assert_eq!(request.adapter, "codex");
        assert!(request
            .gstack_fingerprint
            .as_deref()
            .unwrap_or_default()
            .starts_with("sha256:"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_load_rejects_bad_ids() {
        let root = unique_temp_root();
        fs::create_dir_all(root.join("swb/profiles")).unwrap();
        fs::write(
            root.join("swb/profiles/bad/id.md"),
            "+++\nid = \"bad/id\"\n+++\n",
        )
        .unwrap_err();

        let error = get_profile_from_root(&root, "bad/id").unwrap_err();
        assert!(matches!(error, ConfigError::InvalidProfileId(_)));

        fs::remove_dir_all(root).unwrap();
    }
}
