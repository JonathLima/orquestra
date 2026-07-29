use directories::ProjectDirs;
use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Deserializer, Serialize, de};
use std::path::PathBuf;

use crate::security::SecurityConfig;

pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
    Jsonl,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Human => write!(f, "human"),
            Self::Json => write!(f, "json"),
            Self::Jsonl => write!(f, "jsonl"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            "jsonl" => Ok(Self::Jsonl),
            _ => Err(format!(
                "Invalid output format: {s}. Expected human, json, or jsonl"
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub output: OutputFormat,

    #[serde(default = "default_log_level")]
    pub log_level: String,

    #[serde(default)]
    pub project: ProjectConfig,

    #[serde(default)]
    pub security: SecurityConfig,

    #[serde(default)]
    pub init: InitConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output: OutputFormat::default(),
            log_level: default_log_level(),
            project: ProjectConfig::default(),
            security: SecurityConfig::default(),
            init: InitConfig::default(),
        }
    }
}

fn default_log_level() -> String {
    "warn".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    #[serde(default)]
    pub skills_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokensSource {
    #[default]
    Telemetry,
    Estimated,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitConfig {
    #[serde(
        default = "default_min_confidence",
        deserialize_with = "deserialize_min_confidence"
    )]
    pub min_confidence: f32,
    #[serde(default = "default_max_tickets")]
    pub max_tickets: usize,
    #[serde(default = "default_min_rounds")]
    pub min_rounds: u32,
    #[serde(default = "default_true")]
    pub auto_research: bool,
    #[serde(default)]
    pub max_contradictions: usize,
    #[serde(default = "default_max_tickets_hard_limit")]
    pub max_tickets_hard_limit: usize,
    #[serde(default = "default_true")]
    pub require_primary_source: bool,
    #[serde(default = "default_host")]
    pub default_host: String,
    #[serde(default)]
    pub tokens_source: TokensSource,
    #[serde(default)]
    pub research: InitResearchConfig,
    #[serde(default)]
    pub artifacts: InitArtifactsConfig,
}

impl Default for InitConfig {
    fn default() -> Self {
        Self {
            min_confidence: default_min_confidence(),
            max_tickets: default_max_tickets(),
            min_rounds: default_min_rounds(),
            auto_research: default_true(),
            max_contradictions: 0,
            max_tickets_hard_limit: default_max_tickets_hard_limit(),
            require_primary_source: default_true(),
            default_host: default_host(),
            tokens_source: TokensSource::default(),
            research: InitResearchConfig::default(),
            artifacts: InitArtifactsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitResearchConfig {
    #[serde(default = "default_min_sources_per_topic")]
    pub min_sources_per_topic: usize,
    #[serde(default = "default_min_agreement")]
    pub min_agreement_for_confirmed: usize,
    #[serde(default = "default_min_reliability")]
    #[serde(deserialize_with = "deserialize_normalized_score")]
    pub min_reliability_score: f32,
    #[serde(default = "default_max_research_loops")]
    pub max_research_loops: u32,
    #[serde(default = "default_true")]
    pub prefer_official_docs: bool,
    #[serde(default)]
    pub allow_user_override: bool,
}

impl Default for InitResearchConfig {
    fn default() -> Self {
        Self {
            min_sources_per_topic: default_min_sources_per_topic(),
            min_agreement_for_confirmed: default_min_agreement(),
            min_reliability_score: default_min_reliability(),
            max_research_loops: default_max_research_loops(),
            prefer_official_docs: default_true(),
            allow_user_override: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitArtifactsConfig {
    #[serde(default = "default_max_lines")]
    pub max_lines_per_file: usize,
    #[serde(default = "default_min_lines")]
    pub min_lines_per_file: usize,
    #[serde(default = "default_true")]
    pub auto_split: bool,
    #[serde(default = "default_true")]
    pub require_index: bool,
}

impl Default for InitArtifactsConfig {
    fn default() -> Self {
        Self {
            max_lines_per_file: default_max_lines(),
            min_lines_per_file: default_min_lines(),
            auto_split: default_true(),
            require_index: default_true(),
        }
    }
}

fn default_max_tickets() -> usize {
    8
}
fn default_min_confidence() -> f32 {
    0.95
}
fn default_min_rounds() -> u32 {
    3
}
fn default_true() -> bool {
    true
}
fn default_max_tickets_hard_limit() -> usize {
    12
}
fn default_host() -> String {
    "opencode".to_string()
}
fn default_min_sources_per_topic() -> usize {
    5
}
fn default_min_agreement() -> usize {
    2
}
fn default_min_reliability() -> f32 {
    0.95
}
fn default_max_research_loops() -> u32 {
    3
}
fn default_max_lines() -> usize {
    300
}
fn default_min_lines() -> usize {
    50
}

fn deserialize_min_confidence<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    if value.is_finite() && (0.80..=0.99).contains(&value) {
        Ok(value)
    } else {
        Err(de::Error::custom(
            "init.min_confidence must be between 0.80 and 0.99 inclusive",
        ))
    }
}

fn deserialize_normalized_score<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    if !value.is_finite() {
        return Err(de::Error::custom("score must be finite"));
    }
    let normalized = if value > 1.0 { value / 10.0 } else { value };
    Ok(normalized.clamp(0.0, 1.0))
}

pub struct ConfigPaths {
    pub global: PathBuf,
    pub project: Option<PathBuf>,
}

pub fn default_config_paths() -> Option<ConfigPaths> {
    let proj_dirs = ProjectDirs::from("", "", "orquestra")?;
    let global = proj_dirs.config_dir().join("config.toml");
    Some(ConfigPaths {
        global,
        project: None,
    })
}

#[allow(clippy::result_large_err)]
pub fn load_config(
    output_override: Option<OutputFormat>,
    log_level_override: Option<String>,
    project_dir: Option<PathBuf>,
) -> Result<Config, figment::Error> {
    let mut figment = Figment::from(Serialized::defaults(Config::default()));

    // Layer 2: Global config
    if let Some(paths) = default_config_paths()
        && paths.global.exists()
    {
        figment = figment.merge(Toml::file(&paths.global));
    }

    // Layer 3: Environment variables
    figment = figment.merge(Env::prefixed("ORQUESTRA_"));

    // Layer 4: Project config. Project-local security policy is authoritative
    // for local harness runs and must not be loosened by ambient env vars.
    if let Some(proj_dir) = &project_dir {
        let proj_cfg = proj_dir.join(".orquestra").join("config.toml");
        if proj_cfg.exists() {
            figment = figment.merge(Toml::file(&proj_cfg));
        }
    }

    let mut config: Config = figment.extract()?;

    if let Some(of) = output_override {
        config.output = of;
    }
    if let Some(ll) = log_level_override {
        config.log_level = ll;
    }

    Ok(config)
}

pub fn init_tracing(level: &str) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::builder()
        .with_default_directive(level.parse().unwrap_or(tracing::Level::WARN.into()))
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_output_format_display() {
        assert_eq!(OutputFormat::Human.to_string(), "human");
        assert_eq!(OutputFormat::Json.to_string(), "json");
        assert_eq!(OutputFormat::Jsonl.to_string(), "jsonl");
    }

    #[test]
    fn test_output_format_from_str() {
        assert_eq!(
            OutputFormat::from_str("human").unwrap(),
            OutputFormat::Human
        );
        assert_eq!(OutputFormat::from_str("json").unwrap(), OutputFormat::Json);
        assert_eq!(
            OutputFormat::from_str("jsonl").unwrap(),
            OutputFormat::Jsonl
        );
        assert!(OutputFormat::from_str("invalid").is_err());
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::default();
        assert_eq!(config.output, OutputFormat::Human);
        assert_eq!(config.log_level, "warn");
        assert!(config.project.skills_paths.is_empty());
        assert!(!config.security.allow_external_brain);
    }

    #[test]
    fn test_config_with_overrides() {
        let config =
            load_config(Some(OutputFormat::Json), Some("debug".to_string()), None).unwrap();
        assert_eq!(config.output, OutputFormat::Json);
        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn test_init_config_defaults() {
        let init = InitConfig::default();
        assert!((init.min_confidence - 0.95).abs() < f32::EPSILON);
        assert_eq!(init.max_tickets, 8);
        assert_eq!(init.min_rounds, 3);
        assert!(init.auto_research);
        assert_eq!(init.max_contradictions, 0);
        assert_eq!(init.max_tickets_hard_limit, 12);
        assert!(init.require_primary_source);
        assert_eq!(init.default_host, "opencode");
        assert_eq!(init.tokens_source, TokensSource::Telemetry);
        assert_eq!(init.research.min_sources_per_topic, 5);
        assert_eq!(init.research.min_agreement_for_confirmed, 2);
        assert!((init.research.min_reliability_score - 0.95).abs() < f32::EPSILON);
        assert_eq!(init.research.max_research_loops, 3);
        assert!(init.research.prefer_official_docs);
        assert!(!init.research.allow_user_override);
        assert_eq!(init.artifacts.max_lines_per_file, 300);
        assert_eq!(init.artifacts.min_lines_per_file, 50);
        assert!(init.artifacts.auto_split);
        assert!(init.artifacts.require_index);
    }

    #[test]
    fn test_init_config_parses_full_section() {
        let toml = r#"
            [init]
            max_tickets = 5
            min_rounds = 2
            auto_research = false
            max_contradictions = 2
            max_tickets_hard_limit = 10
            require_primary_source = false
            default_host = "codex"
            tokens_source = "manual"

            [init.research]
            min_sources_per_topic = 3
            min_agreement_for_confirmed = 2
            min_reliability_score = 6.5
            max_research_loops = 5
            prefer_official_docs = false
            allow_user_override = false

            [init.artifacts]
            max_lines_per_file = 200
            min_lines_per_file = 30
            auto_split = false
            require_index = false
        "#;
        let figment = figment::Figment::new().merge(figment::providers::Toml::string(toml));
        let config: Config = figment.extract().expect("parse toml");
        assert_eq!(config.init.max_tickets, 5);
        assert_eq!(config.init.min_rounds, 2);
        assert!(!config.init.auto_research);
        assert_eq!(config.init.max_contradictions, 2);
        assert_eq!(config.init.max_tickets_hard_limit, 10);
        assert!(!config.init.require_primary_source);
        assert_eq!(config.init.default_host, "codex");
        assert_eq!(config.init.tokens_source, TokensSource::Manual);
        assert_eq!(config.init.research.min_sources_per_topic, 3);
        assert!((config.init.research.min_reliability_score - 0.65).abs() < f32::EPSILON);
        assert_eq!(config.init.research.max_research_loops, 5);
        assert!(!config.init.research.prefer_official_docs);
        assert_eq!(config.init.artifacts.max_lines_per_file, 200);
        assert_eq!(config.init.artifacts.min_lines_per_file, 30);
        assert!(!config.init.artifacts.auto_split);
    }

    #[test]
    fn test_init_config_partial_section_uses_defaults() {
        let toml = r#"
            [init]
            max_tickets = 6
        "#;
        let figment = figment::Figment::new().merge(figment::providers::Toml::string(toml));
        let config: Config = figment.extract().expect("parse toml");
        assert_eq!(config.init.max_tickets, 6);
        assert_eq!(config.init.min_rounds, 3);
        assert!((config.init.min_confidence - 0.95).abs() < f32::EPSILON);
        assert_eq!(config.init.research.min_sources_per_topic, 5);
        assert_eq!(config.init.artifacts.max_lines_per_file, 300);
    }

    #[test]
    fn test_init_min_confidence_accepts_inclusive_bounds() {
        for value in [0.80, 0.99] {
            let toml = format!("[init]\nmin_confidence = {value}");
            let figment = figment::Figment::new().merge(figment::providers::Toml::string(&toml));
            let config: Config = figment.extract().expect("inclusive bound must be valid");
            assert!((config.init.min_confidence - value).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn test_init_min_confidence_rejects_values_outside_bounds() {
        for value in [0.79, 1.0] {
            let toml = format!("[init]\nmin_confidence = {value}");
            let figment = figment::Figment::new().merge(figment::providers::Toml::string(&toml));
            assert!(
                figment.extract::<Config>().is_err(),
                "{value} must be rejected"
            );
        }
    }

    #[test]
    fn test_legacy_research_reliability_is_normalized_at_deserialization() {
        let toml = "[init.research]\nmin_reliability_score = 7.0";
        let figment = figment::Figment::new().merge(figment::providers::Toml::string(toml));
        let config: Config = figment.extract().expect("legacy config remains readable");
        assert!((config.init.research.min_reliability_score - 0.7).abs() < f32::EPSILON);
    }
}
