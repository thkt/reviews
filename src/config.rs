use crate::traverse;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::path::{Path, PathBuf};

const TOOLS_CONFIG_FILE: &str = ".claude/tools.json";
const LEGACY_CONFIG_FILE: &str = ".claude-reviews.json";

#[derive(Debug, Deserialize)]
struct ToolsJsonConfig {
    reviews: Option<ProjectConfig>,
}

macro_rules! define_tools {
    ($($field:ident),+ $(,)?) => {
        #[derive(Debug, Clone)]
        pub struct ToolsConfig {
            $(pub $field: bool,)+
        }

        impl Default for ToolsConfig {
            fn default() -> Self {
                Self { $($field: true,)+ }
            }
        }

        #[derive(Debug, Deserialize)]
        struct ProjectToolsConfig {
            $($field: Option<bool>,)+
        }

        impl ToolsConfig {
            fn apply(&mut self, overrides: &ProjectToolsConfig) {
                $(if let Some(v) = overrides.$field { self.$field = v; })+
            }
        }
    };
}

define_tools! {
    knip,
    oxlint,
    tsgo,
    react_doctor,
    markuplint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigSource {
    Default,
    ToolsJson,
    Legacy,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub enabled: bool,
    pub skills: Option<Vec<String>>,
    pub tools: ToolsConfig,
    pub source: ConfigSource,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            skills: None,
            tools: ToolsConfig::default(),
            source: ConfigSource::Default,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProjectConfig {
    enabled: Option<bool>,
    skills: Option<Vec<String>>,
    tools: Option<ProjectToolsConfig>,
}

impl Config {
    pub fn load(start: &Path) -> Self {
        let default = Self::default();
        let Some(git_root) = Self::find_git_root(start) else {
            return default;
        };

        let tools_path = git_root.join(TOOLS_CONFIG_FILE);
        if tools_path.exists() {
            return Self::load_tools_config(&tools_path, default);
        }

        let legacy_path = git_root.join(LEGACY_CONFIG_FILE);
        if legacy_path.exists() {
            return Self::load_legacy_config(&legacy_path, default);
        }

        default
    }

    fn find_git_root(start: &Path) -> Option<PathBuf> {
        traverse::walk_ancestors(start, |dir| {
            dir.join(".git").exists().then(|| dir.to_path_buf())
        })
    }

    fn read_and_parse<T: DeserializeOwned>(path: &Path) -> Option<T> {
        let content = fs::read_to_string(path)
            .map_err(|e| eprintln!("Reviews: warning: failed to read config: {}", e))
            .ok()?;
        serde_json::from_str(&content)
            .map_err(|e| eprintln!("Reviews: warning: invalid config JSON: {}", e))
            .ok()
    }

    fn load_tools_config(path: &Path, mut base: Config) -> Config {
        let Some(tools) = Self::read_and_parse::<ToolsJsonConfig>(path) else {
            return base;
        };
        match tools.reviews {
            Some(project) => {
                base.source = ConfigSource::ToolsJson;
                base.merge(project)
            }
            None => base,
        }
    }

    fn load_legacy_config(path: &Path, mut base: Config) -> Config {
        match Self::read_and_parse::<ProjectConfig>(path) {
            Some(project) => {
                base.source = ConfigSource::Legacy;
                base.merge(project)
            }
            None => base,
        }
    }

    fn merge(mut self, project: ProjectConfig) -> Self {
        if let Some(enabled) = project.enabled {
            self.enabled = enabled;
        }
        if let Some(skills) = project.skills {
            self.skills = Some(skills);
        }
        if let Some(ref tools) = project.tools {
            self.tools.apply(tools);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::TempDir;
    use std::fs;

    #[test]
    fn default_config_all_tools_enabled() {
        let tmp = TempDir::new("config-default");
        fs::create_dir_all(tmp.join(".git")).unwrap();

        let config = Config::load(&tmp);
        assert!(config.enabled);
        assert_eq!(config.source, ConfigSource::Default);
        assert!(config.tools.knip);
        assert!(config.tools.oxlint);
        assert!(config.tools.tsgo);
        assert!(config.tools.react_doctor);
        assert!(config.tools.markuplint);
    }

    #[test]
    fn partial_config_from_tools_json() {
        let tmp = TempDir::new("config-partial");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"tools": {"knip": false}}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert!(config.enabled);
        assert_eq!(config.source, ConfigSource::ToolsJson);
        assert!(!config.tools.knip);
        assert!(config.tools.oxlint);
        assert!(config.tools.tsgo);
        assert!(config.tools.react_doctor);
    }

    #[test]
    fn enabled_false_disables_all() {
        let tmp = TempDir::new("config-disabled");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"enabled": false}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert!(!config.enabled);
        assert_eq!(config.source, ConfigSource::ToolsJson);
    }

    #[test]
    fn invalid_json_falls_back_to_default() {
        let tmp = TempDir::new("config-invalid");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(tmp.join(TOOLS_CONFIG_FILE), "not valid json{{{").unwrap();

        let config = Config::load(&tmp);
        assert!(config.enabled);
        assert!(config.tools.knip);
    }

    #[test]
    fn default_skills_is_none() {
        let tmp = TempDir::new("config-skills-default");
        fs::create_dir_all(tmp.join(".git")).unwrap();

        let config = Config::load(&tmp);
        assert!(config.skills.is_none());
    }

    #[test]
    fn skills_override_replaces_default() {
        let tmp = TempDir::new("config-skills-override");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"skills": ["audit", "preview"]}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert_eq!(config.skills, Some(vec!["audit".into(), "preview".into()]));
    }

    #[test]
    fn empty_skills_list_is_preserved() {
        let tmp = TempDir::new("config-skills-empty");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"skills": []}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert_eq!(config.skills, Some(vec![] as Vec<String>));
    }

    #[test]
    fn skills_null_treated_as_none() {
        let tmp = TempDir::new("config-skills-null");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"skills": null}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert!(config.skills.is_none());
    }

    #[test]
    fn skills_wrong_type_falls_back_to_default() {
        let tmp = TempDir::new("config-skills-wrongtype");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"skills": 42}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert!(config.skills.is_none());
        assert!(config.enabled);
    }

    #[test]
    fn loads_from_legacy_config() {
        let tmp = TempDir::new("config-legacy");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::write(
            tmp.join(LEGACY_CONFIG_FILE),
            r#"{"tools": {"knip": false}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert_eq!(config.source, ConfigSource::Legacy);
        assert!(!config.tools.knip);
        assert!(config.tools.oxlint);
    }

    #[test]
    fn tools_json_takes_priority_over_legacy() {
        let tmp = TempDir::new("config-priority");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"tools": {"knip": false}}}"#,
        )
        .unwrap();
        fs::write(
            tmp.join(LEGACY_CONFIG_FILE),
            r#"{"tools": {"oxlint": false}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert!(!config.tools.knip);
        assert!(config.tools.oxlint);
    }

    #[test]
    fn tools_json_without_reviews_key_returns_defaults() {
        let tmp = TempDir::new("config-no-key");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"formatter": {"some": "config"}}"#,
        )
        .unwrap();

        let config = Config::load(&tmp);
        assert_eq!(config.source, ConfigSource::Default);
        assert!(config.tools.knip);
        assert!(config.tools.oxlint);
    }

    #[test]
    fn finds_config_in_parent_directory() {
        let tmp = TempDir::new("config-parent");
        fs::create_dir_all(tmp.join(".git")).unwrap();
        fs::create_dir_all(tmp.join(".claude")).unwrap();
        fs::write(
            tmp.join(TOOLS_CONFIG_FILE),
            r#"{"reviews": {"tools": {"knip": false}}}"#,
        )
        .unwrap();
        let subdir = tmp.join("src").join("components");
        fs::create_dir_all(&subdir).unwrap();

        let config = Config::load(&subdir);
        assert!(!config.tools.knip);
    }
}
