use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
/// Empty model choices use the provider's configured/recommended default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentModels {
    pub coordinator: String,
    pub worker: String,
    pub reviewer: String,
    pub complex_work: String,
}
impl AgentModels {
    pub fn select(&self, role: &str, complex: bool) -> Option<&str> {
        let choice = match role {
            "coordinator" => &self.coordinator,
            "reviewer" => &self.reviewer,
            _ if complex && !self.complex_work.trim().is_empty() => &self.complex_work,
            _ => &self.worker,
        };
        let value = choice.trim();
        (!value.is_empty()).then_some(value)
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub worktrees_directory: String,
    pub copy_on_select: bool,
    pub enabled_agents: Vec<String>,
    #[serde(default)]
    pub agent_models: BTreeMap<String, AgentModels>,
    pub max_agents_per_coordinator: usize,
    #[serde(default = "default_resume_interrupted")]
    pub resume_interrupted_work: bool,
    pub appearance: String,
    pub close_empty_workspaces: bool,
    pub close_empty_tabs: bool,
    pub allow_workspace_deletion: bool,
    pub bindings: BTreeMap<String, Vec<String>>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            worktrees_directory: crate::worktree_template::DEFAULT.into(),
            copy_on_select: false,
            enabled_agents: vec!["claude".into(), "codex".into()],
            agent_models: BTreeMap::new(),
            max_agents_per_coordinator: 8,
            resume_interrupted_work: true,
            appearance: "system".into(),
            close_empty_workspaces: false,
            close_empty_tabs: true,
            allow_workspace_deletion: true,
            bindings: BTreeMap::from([
                ("find".into(), vec!["cmd+f".into(), "ctrl+f".into()]),
                ("find_next".into(), vec!["cmd+g".into()]),
                ("find_previous".into(), vec!["cmd+shift+g".into()]),
                (
                    "palette".into(),
                    vec!["ctrl+shift+p".into(), "cmd+shift+p".into()],
                ),
                (
                    "split_right".into(),
                    vec!["ctrl+shift+e".into(), "cmd+d".into()],
                ),
                (
                    "split_down".into(),
                    vec!["ctrl+shift+o".into(), "cmd+shift+d".into()],
                ),
                (
                    "new_tab".into(),
                    vec!["ctrl+shift+t".into(), "cmd+t".into()],
                ),
                ("close_pane".into(), vec!["ctrl+shift+w".into()]),
                ("new_worktree".into(), vec!["ctrl+shift+m".into()]),
                ("open_worktree".into(), vec!["ctrl+shift+g".into()]),
                ("close_workspace".into(), vec!["ctrl+shift+q".into()]),
                ("new_workspace".into(), vec!["ctrl+shift+n".into()]),
                ("open_workspace".into(), vec!["cmd+o".into()]),
            ]),
        }
    }
}
pub fn path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap()).join(".config"))
        .join("jam/config.toml")
}
pub fn normalize_chord(value: &str) -> Result<String> {
    let lower = value.to_lowercase();
    let parts: Vec<_> = lower.split('+').collect();
    if parts.len() < 2 {
        bail!("Shortcut needs modifiers")
    }
    let mut sorted = Vec::new();
    for modifier in ["ctrl", "shift", "alt", "cmd"] {
        if parts[..parts.len() - 1].contains(&modifier) {
            sorted.push(modifier)
        }
    }
    if sorted.len() != parts.len() - 1 {
        bail!("Unknown or repeated modifier")
    }
    sorted.push(parts[parts.len() - 1]);
    Ok(sorted.join("+"))
}
impl Config {
    fn add_missing_defaults(&mut self) {
        let occupied: std::collections::BTreeSet<_> =
            self.bindings.values().flatten().cloned().collect();
        for (action, keys) in Config::default().bindings {
            self.bindings
                .entry(action)
                .or_insert_with(|| keys.into_iter().filter(|k| !occupied.contains(k)).collect());
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !(1..=64).contains(&self.max_agents_per_coordinator) {
            bail!("Agent limit must be between 1 and 64")
        }
        if !["system", "light", "dark"].contains(&self.appearance.as_str()) {
            bail!("Appearance must be system, light, or dark")
        }
        let mut providers = std::collections::HashSet::new();
        for agent in &self.enabled_agents {
            if !["claude", "codex"].contains(&agent.as_str()) || !providers.insert(agent) {
                bail!("Enabled agents must be unique Claude/Codex entries")
            }
        }
        for (provider, models) in &self.agent_models {
            if !["claude", "codex"].contains(&provider.as_str()) {
                bail!("Unknown model provider {provider}")
            }
            for model in [
                &models.coordinator,
                &models.worker,
                &models.reviewer,
                &models.complex_work,
            ] {
                let value = model.trim();
                if value.len() > 200
                    || value.starts_with('-')
                    || value.chars().any(|c| c.is_control() || c.is_whitespace())
                {
                    bail!("Model must be a model ID without whitespace (or blank for Jam default)")
                }
            }
        }
        let mut seen = BTreeMap::new();
        for (action, keys) in &self.bindings {
            if ![
                "palette",
                "split_right",
                "split_down",
                "find",
                "find_next",
                "find_previous",
                "new_tab",
                "open_workspace",
                "new_workspace",
                "new_worktree",
                "open_worktree",
                "close_workspace",
                "close_pane",
                "settings",
                "agent_integrations",
            ]
            .contains(&action.as_str())
            {
                bail!("Unknown action {action}")
            }
            for key in keys {
                let key = normalize_chord(key)?;
                let parts: Vec<_> = key.split('+').collect();
                if parts.len() < 2
                    || !parts[..parts.len() - 1]
                        .iter()
                        .any(|p| ["ctrl", "cmd", "alt"].contains(p))
                    || parts[..parts.len() - 1]
                        .iter()
                        .any(|p| !["ctrl", "cmd", "alt", "shift"].contains(p))
                    || parts.last().unwrap().chars().count() != 1
                {
                    bail!("Invalid shortcut {key}; use modifiers and one letter")
                };
                if let Some(other) = seen.insert(key.clone(), action) {
                    bail!("{key} is already assigned to {other}")
                }
            }
        }
        crate::worktree_template::normalize(&self.worktrees_directory)?;
        Ok(())
    }
}
pub fn decode(text: &str) -> Result<Config> {
    let mut c: Config = toml::from_str(text)?;
    for keys in c.bindings.values_mut() {
        for key in keys {
            *key = normalize_chord(key)?;
        }
    }
    c.worktrees_directory = crate::worktree_template::normalize(&c.worktrees_directory)?;
    c.validate()?;
    c.add_missing_defaults();
    c.validate()?;
    Ok(c)
}
pub fn load() -> Result<Config> {
    load_at(&path())
}
fn load_at(path: &std::path::Path) -> Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(s) => decode(&s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e.into()),
    }
}
fn target(path: &std::path::Path) -> Result<PathBuf> {
    if std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        Ok(std::fs::canonicalize(path)?)
    } else {
        Ok(path.to_path_buf())
    }
}
pub fn revision_at(path: &std::path::Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::MetadataExt;
    match std::fs::read(path) {
        Ok(bytes) => {
            let m = std::fs::metadata(path)?;
            let mut h = Sha256::new();
            h.update(bytes);
            h.update(format!(
                "{}:{}:{}:{}:{}",
                m.dev(),
                m.ino(),
                m.mtime(),
                m.mtime_nsec(),
                target(path)?.display()
            ));
            Ok(format!("{:x}", h.finalize()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("missing".into()),
        Err(e) => Err(e.into()),
    }
}
pub fn snapshot() -> Result<serde_json::Value> {
    let p = path();
    let before = revision_at(&p)?;
    let c = load_at(&p)?;
    let after = revision_at(&p)?;
    if before != after {
        bail!("Settings changed while opening; reopen Settings")
    }
    Ok(serde_json::json!({"config":c,"revision":after,"path":p}))
}
pub fn save(c: &Config) -> Result<()> {
    save_checked_at(&path(), c, None)
}
pub fn save_checked(c: &Config, revision: &str) -> Result<()> {
    save_checked_at(&path(), c, Some(revision))
}
fn save_checked_at(path: &std::path::Path, c: &Config, expected: Option<&str>) -> Result<()> {
    use fs2::FileExt;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    c.validate()?;
    let p = target(path)?;
    let parent = p.parent().context("Settings path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(parent.join(".jam-settings.lock"))?;
    lock.lock_exclusive()?;
    let before = revision_at(path)?;
    if expected.is_some_and(|e| e != before) {
        bail!("Settings changed outside this window. Close and reopen Settings before saving.")
    }
    let tmp = parent.join(format!(".jam-config-{}.tmp", crate::id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        if let Ok(m) = std::fs::metadata(&p) {
            file.set_permissions(m.permissions())?;
        }
        file.write_all(toml::to_string_pretty(c)?.as_bytes())?;
        file.sync_all()?;
        if revision_at(path)? != before {
            bail!("Settings changed outside this window. Close and reopen Settings before saving.")
        }
        std::fs::rename(&tmp, &p)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(tmp);
    result
}
/// Preferences distributed by the app; credentials and local paths never travel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostSettings {
    #[serde(default = "default_worktree_template")]
    pub worktrees_directory: String,
    pub enabled_agents: Vec<String>,
    #[serde(default)]
    pub agent_models: BTreeMap<String, AgentModels>,
    pub max_agents_per_coordinator: usize,
    #[serde(default = "default_resume_interrupted")]
    pub resume_interrupted_work: bool,
    pub close_empty_workspaces: bool,
    pub close_empty_tabs: bool,
    pub allow_workspace_deletion: bool,
}
fn default_resume_interrupted() -> bool {
    true
}
fn default_worktree_template() -> String {
    crate::worktree_template::DEFAULT.into()
}
impl From<Config> for HostSettings {
    fn from(c: Config) -> Self {
        Self {
            worktrees_directory: c.worktrees_directory,
            enabled_agents: c.enabled_agents,
            agent_models: c.agent_models,
            max_agents_per_coordinator: c.max_agents_per_coordinator,
            resume_interrupted_work: c.resume_interrupted_work,
            close_empty_workspaces: c.close_empty_workspaces,
            close_empty_tabs: c.close_empty_tabs,
            allow_workspace_deletion: c.allow_workspace_deletion,
        }
    }
}
impl HostSettings {
    pub fn validate(&self) -> Result<()> {
        Config {
            worktrees_directory: self.worktrees_directory.clone(),
            enabled_agents: self.enabled_agents.clone(),
            agent_models: self.agent_models.clone(),
            max_agents_per_coordinator: self.max_agents_per_coordinator,
            ..Config::default()
        }
        .validate()
    }
}
static HOST: std::sync::LazyLock<std::sync::RwLock<Option<HostSettings>>> =
    std::sync::LazyLock::new(|| std::sync::RwLock::new(None));
fn host_path() -> PathBuf {
    crate::state_dir().join("app-settings.json")
}
pub fn host_settings() -> HostSettings {
    let mut current = HOST.write().unwrap();
    current
        .get_or_insert_with(|| {
            std::fs::read(host_path())
                .ok()
                .and_then(|b| serde_json::from_slice::<HostSettings>(&b).ok())
                .filter(|c| c.validate().is_ok())
                .unwrap_or_else(|| load().unwrap_or_default().into())
        })
        .clone()
}
pub fn apply_host_settings(settings: HostSettings) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    settings.validate()?;
    let mut current = HOST.write().unwrap();
    if current.as_ref() == Some(&settings) {
        return Ok(());
    }
    let p = host_path();
    std::fs::create_dir_all(p.parent().unwrap())?;
    let tmp = p.with_extension(format!("{}.tmp", crate::id()));
    let result = (|| -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(&serde_json::to_vec(&settings)?)?;
        file.sync_all()?;
        std::fs::rename(&tmp, p)?;
        Ok(())
    })();
    let _ = std::fs::remove_file(tmp);
    result?;
    *current = Some(settings);
    Ok(())
}
pub fn agent_enabled(provider: &str) -> bool {
    host_settings().enabled_agents.iter().any(|p| p == provider)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_preferences_round_trip_and_empty_values_keep_provider_defaults() {
        let c = decode("[agent_models.claude]\ncoordinator = 'opus'\nworker = 'sonnet'\nreviewer = 'review-model'\ncomplex_work = ''\n").unwrap();
        let models = &c.agent_models["claude"];
        assert_eq!(models.select("coordinator", false), Some("opus"));
        assert_eq!(models.select("worker", true), Some("sonnet"));
        assert_eq!(models.select("reviewer", false), Some("review-model"));
        assert_eq!(AgentModels::default().select("worker", false), None);
        let encoded = toml::to_string(&c).unwrap();
        assert_eq!(decode(&encoded).unwrap().agent_models, c.agent_models);
        let mut bad = c.clone();
        bad.agent_models.get_mut("claude").unwrap().worker = "model\n--option".into();
        assert!(bad.validate().is_err());
        let mut old = serde_json::to_value(HostSettings::from(Config::default())).unwrap();
        old.as_object_mut().unwrap().remove("agent_models");
        assert!(
            serde_json::from_value::<HostSettings>(old)
                .unwrap()
                .agent_models
                .is_empty()
        );
    }
    #[test]
    fn old_worktree_directories_load_as_explicit_templates() {
        let c = decode("worktrees_directory = '~/custom'\n").unwrap();
        assert_eq!(c.worktrees_directory, "~/custom/{{repo}}/{{worktree}}");
        assert!(decode("worktrees_directory = '~/{{unknown}}'\n").is_err());
        let mut old = serde_json::to_value(HostSettings::from(Config::default())).unwrap();
        old.as_object_mut().unwrap().remove("worktrees_directory");
        let migrated: HostSettings = serde_json::from_value(old).unwrap();
        assert_eq!(
            migrated.worktrees_directory,
            crate::worktree_template::DEFAULT
        );
    }
    #[test]
    fn checked_save_preserves_external_edits_and_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let actual = dir.path().join("actual.toml");
        let link = dir.path().join("config.toml");
        let mut c = Config::default();
        save_checked_at(&actual, &c, Some("missing")).unwrap();
        symlink(&actual, &link).unwrap();
        let revision = revision_at(&link).unwrap();
        c.appearance = "light".into();
        save_checked_at(&link, &c, Some(&revision)).unwrap();
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(load_at(&actual).unwrap().appearance, "light");
        let revision = revision_at(&link).unwrap();
        std::fs::write(&actual, "# changed externally\nappearance = 'dark'\n").unwrap();
        let changed = std::fs::read(&actual).unwrap();
        assert!(
            save_checked_at(&link, &c, Some(&revision))
                .unwrap_err()
                .to_string()
                .contains("reopen")
        );
        assert_eq!(std::fs::read(&actual).unwrap(), changed);
        // A file created since opening the editor cannot be overwritten either.
        assert!(save_checked_at(&actual, &c, Some("missing")).is_err());
    }
    #[test]
    fn invalid_drafts_never_replace_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "# keep this\n").unwrap();
        let mut c = Config {
            max_agents_per_coordinator: 0,
            ..Config::default()
        };
        assert!(save_checked_at(&path, &c, Some(&revision_at(&path).unwrap())).is_err());
        c.max_agents_per_coordinator = 5;
        c.enabled_agents.push("unknown".into());
        assert!(c.validate().is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# keep this\n");
    }
    #[test]
    fn new_defaults_preserve_custom_and_disabled_actions() {
        let mut c: Config =
            toml::from_str("[bindings]\nnew_tab = []\nsettings = ['ctrl+shift+w']").unwrap();
        c.add_missing_defaults();
        assert!(c.bindings["new_tab"].is_empty());
        assert!(c.bindings["close_pane"].is_empty());
        assert_eq!(c.bindings["new_worktree"], ["ctrl+shift+m"]);
        assert!(!c.copy_on_select);
        c.validate().unwrap();
    }
    #[test]
    fn conflicts_are_rejected() {
        let mut c = Config::default();
        assert!(c.validate().is_ok());
        c.bindings
            .insert("new_tab".into(), vec!["ctrl+shift+p".into()]);
        assert!(c.validate().is_err());
    }
    #[test]
    fn unmodified_keys_are_rejected() {
        let mut c = Config::default();
        c.bindings.insert("palette".into(), vec!["p".into()]);
        assert!(c.validate().is_err());
    }
}
