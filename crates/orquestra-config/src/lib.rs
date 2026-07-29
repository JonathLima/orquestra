pub mod profile;

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrquestraConfig {
    pub config_version: u32,
    pub verification: VerificationConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationConfig {
    #[serde(default)]
    pub profiles: Vec<VerificationProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationProfile {
    pub name: String,
    pub argv: Vec<String>,
    pub relative_dir: Option<String>,
    pub timeout_seconds: Option<u64>,
    pub max_output_bytes: Option<u64>,
    pub expected_exit_code: Option<i32>,
    #[serde(default)]
    pub expected_artifacts: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResult {
    pub profile_name: String,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub artifacts: Vec<ArtifactVerification>,
    pub duration_ms: u64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactVerification {
    pub path: String,
    pub exists: bool,
    pub size_bytes: Option<u64>,
    pub content_hash: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("Unsupported config version: {0}")]
    UnsupportedVersion(u32),
    #[error("Profile '{0}' has empty argv")]
    EmptyArgv(String),
    #[error("Profile '{0}' not found")]
    ProfileNotFound(String),
    #[error("Profile executor error: {0}")]
    Executor(#[from] profile::ProfileError),
}

impl OrquestraConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn load_default() -> Self {
        Self {
            config_version: 1,
            verification: VerificationConfig { profiles: vec![] },
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.config_version < 1 {
            return Err(ConfigError::UnsupportedVersion(self.config_version));
        }
        for p in &self.verification.profiles {
            if p.argv.is_empty() {
                return Err(ConfigError::EmptyArgv(p.name.clone()));
            }
        }
        Ok(())
    }

    pub fn get_profile(&self, name: &str) -> Option<&VerificationProfile> {
        self.verification.profiles.iter().find(|p| p.name == name)
    }
}

impl Default for OrquestraConfig {
    fn default() -> Self {
        Self::load_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_valid_config_minimal() {
        let toml_str = r#"
config_version = 1

[verification]
profiles = []
"#;
        let config: OrquestraConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.config_version, 1);
        assert!(config.verification.profiles.is_empty());
    }

    #[test]
    fn load_valid_config_with_profile() {
        let toml_str = r#"
config_version = 1

[[verification.profiles]]
name = "default-test"
argv = ["cargo", "test"]
relative_dir = "../"
timeout_seconds = 120
max_output_bytes = 1048576
expected_exit_code = 0
expected_artifacts = []
"#;
        let config: OrquestraConfig = toml::from_str(toml_str).unwrap();
        let profile = config.get_profile("default-test").unwrap();
        assert_eq!(profile.argv, vec!["cargo", "test"]);
        assert_eq!(profile.relative_dir.as_deref(), Some("../"));
        assert_eq!(profile.timeout_seconds, Some(120));
    }

    #[test]
    fn rejects_version_zero() {
        let toml_str = r#"
config_version = 0

[verification]
profiles = []
"#;
        let config: OrquestraConfig = toml::from_str(toml_str).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Unsupported"));
    }

    #[test]
    fn rejects_empty_argv() {
        let toml_str = r#"
config_version = 1

[[verification.profiles]]
name = "bad"
argv = []
"#;
        let config: OrquestraConfig = toml::from_str(toml_str).unwrap();
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("empty argv"));
    }

    #[test]
    fn get_profile_by_name() {
        let config = OrquestraConfig::load_default();
        assert!(config.get_profile("nonexistent").is_none());
    }

    #[test]
    fn load_from_file_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
config_version = 1

[[verification.profiles]]
name = "clippy"
argv = ["cargo", "clippy", "-D", "warnings"]
expected_exit_code = 0
"#,
        )
        .unwrap();
        let config = OrquestraConfig::load(&path).unwrap();
        assert_eq!(config.config_version, 1);
        let profile = config.get_profile("clippy").unwrap();
        assert_eq!(profile.argv, vec!["cargo", "clippy", "-D", "warnings"]);
    }

    #[test]
    fn profile_env_var_parsed() {
        let toml_str = r#"
config_version = 1

[[verification.profiles]]
name = "env-test"
argv = ["sh", "-c", "echo $MY_VAR"]
env = ["MY_VAR=hello"]
"#;
        let config: OrquestraConfig = toml::from_str(toml_str).unwrap();
        let profile = config.get_profile("env-test").unwrap();
        assert_eq!(profile.env, vec!["MY_VAR=hello"]);
    }
}
