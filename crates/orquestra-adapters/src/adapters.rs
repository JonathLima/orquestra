use crate::capability::Capability;
use orquestra_core::config::home_dir;
use orquestra_core::error::OrquestraError;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CliInfo {
    pub name: &'static str,
    pub version: String,
    pub binary_path: PathBuf,
    pub skills_dir: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    pub project_skills_dir: Option<PathBuf>,
    pub agents_dir: Option<PathBuf>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

pub trait HostAdapter: std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;
    fn capabilities(&self) -> Vec<Capability> {
        vec![]
    }
    fn tool_map(&self) -> HashMap<&'static str, &'static str> {
        HashMap::new()
    }
    fn detect(&self) -> Result<Option<CliInfo>, OrquestraError>;
    fn install_plan(&self, skills: &[crate::install::SkillRef]) -> crate::install::InstallPlan;
    fn notes(&self) -> Vec<&'static str> {
        vec![]
    }
    fn agents_dir(&self) -> Option<PathBuf> {
        Some(home_dir().join(".agents"))
    }
}

fn run_cmd(program: &str, arg: &str) -> Result<String, OrquestraError> {
    let output = Command::new(program)
        .arg(arg)
        .output()
        .map_err(|e| OrquestraError::from(format!("Failed to run {program}: {e}")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(OrquestraError::from(format!("{program} exited with error")))
    }
}

fn project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_default()
}

fn project_orquestra_dir() -> PathBuf {
    project_dir().join(".orquestra")
}

fn project_agents_skills_dir() -> PathBuf {
    project_dir().join(".agents").join("skills")
}

fn project_opencode_skills_dir() -> PathBuf {
    project_dir().join(".opencode").join("skills")
}

#[derive(Debug)]
pub struct CodexAdapter;

impl HostAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Subagents,
            Capability::InstructionsOnly,
            Capability::FileSystem,
            Capability::WebSearch,
        ]
    }

    fn tool_map(&self) -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("askUser", "skill orquestra-grill"),
            ("spawnSubagent", "fork / shell"),
            ("readFile", "Read tool"),
            ("writeFile", "Write tool"),
            ("webSearch", "MCP search_web"),
            ("webFetch", "MCP web_fetch"),
        ])
    }

    fn detect(&self) -> Result<Option<CliInfo>, OrquestraError> {
        let path = which::which("codex").ok();
        match path {
            Some(binary_path) => {
                let version = run_cmd("codex", "--version").unwrap_or_default();
                let home = home_dir();
                Ok(Some(CliInfo {
                    name: "codex",
                    version,
                    binary_path,
                    skills_dir: Some(project_agents_skills_dir()),
                    config_dir: None,
                    project_skills_dir: Some(project_agents_skills_dir()),
                    agents_dir: Some(home.join(".agents")),
                    confidence: Confidence::High,
                }))
            }
            None => Ok(None),
        }
    }

    fn install_plan(&self, skills: &[crate::install::SkillRef]) -> crate::install::InstallPlan {
        use crate::install::{FileOperation, InstallPlan};
        let target = project_agents_skills_dir();
        let mut ops = vec![];
        for skill in skills {
            ops.push(FileOperation::CreateDir {
                path: target.join(&skill.name),
            });
            ops.push(FileOperation::CopySkill {
                skill_name: skill.name.clone(),
                target_dir: target.join(&skill.name),
            });
            ops.push(FileOperation::WriteToolsJson {
                target_file: target.join(&skill.name).join("tools.json"),
                tool_map: self.tool_map(),
            });
        }
        ops.push(FileOperation::WriteDiscoveryBlock {
            target_file: project_dir().join("AGENTS.md"),
        });
        InstallPlan {
            host: "codex".to_string(),
            target_skills_dir: target,
            project_orquestra_dir: project_orquestra_dir(),
            operations: ops,
        }
    }
}

#[derive(Debug)]
pub struct ClaudeCodeAdapter;

impl HostAdapter for ClaudeCodeAdapter {
    fn name(&self) -> &'static str {
        "claude-code"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Subagents,
            Capability::NonInteractive,
            Capability::FileSystem,
            Capability::WebSearch,
        ]
    }

    fn tool_map(&self) -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("askUser", "skill orquestra-grill"),
            ("spawnSubagent", "claude fork / claude --print"),
            ("readFile", "Read tool"),
            ("writeFile", "Write tool"),
            ("webSearch", "MCP search_web"),
            ("webFetch", "MCP web_fetch"),
        ])
    }

    fn detect(&self) -> Result<Option<CliInfo>, OrquestraError> {
        let path = which::which("claude").ok();
        match path {
            Some(binary_path) => {
                let version = run_cmd("claude", "--version").unwrap_or_default();
                let cwd = std::env::current_dir().unwrap_or_default();
                let home = home_dir();
                Ok(Some(CliInfo {
                    name: "claude-code",
                    version,
                    binary_path,
                    skills_dir: Some(cwd.join(".claude").join("skills")),
                    config_dir: Some(cwd.join(".claude")),
                    project_skills_dir: Some(cwd.join(".claude").join("skills")),
                    agents_dir: Some(home.join(".agents")),
                    confidence: Confidence::High,
                }))
            }
            None => Ok(None),
        }
    }

    fn install_plan(&self, skills: &[crate::install::SkillRef]) -> crate::install::InstallPlan {
        use crate::install::{FileOperation, InstallPlan};
        let cwd = project_dir();
        let target = cwd.join(".claude").join("skills");
        let mut ops = vec![];
        for skill in skills {
            ops.push(FileOperation::CreateDir {
                path: target.join(&skill.name),
            });
            ops.push(FileOperation::CopySkill {
                skill_name: skill.name.clone(),
                target_dir: target.join(&skill.name),
            });
            ops.push(FileOperation::WriteToolsJson {
                target_file: target.join(&skill.name).join("tools.json"),
                tool_map: self.tool_map(),
            });
        }
        ops.push(FileOperation::WriteDiscoveryBlock {
            target_file: cwd.join("AGENTS.md"),
        });
        InstallPlan {
            host: "claude-code".to_string(),
            target_skills_dir: target,
            project_orquestra_dir: project_orquestra_dir(),
            operations: ops,
        }
    }

    fn notes(&self) -> Vec<&'static str> {
        vec!["Uses project-local .claude/ directory"]
    }
}

#[derive(Debug)]
pub struct OpenCodeAdapter;

impl HostAdapter for OpenCodeAdapter {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Subagents,
            Capability::NonInteractive,
            Capability::Hooks,
            Capability::FileSystem,
            Capability::WebSearch,
        ]
    }

    fn tool_map(&self) -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("askUser", "skill orquestra-grill"),
            ("spawnSubagent", "Task tool"),
            ("readFile", "Read tool"),
            ("writeFile", "Write tool"),
            ("webSearch", "WebSearch tool"),
            ("webFetch", "WebFetch tool"),
        ])
    }

    fn detect(&self) -> Result<Option<CliInfo>, OrquestraError> {
        let path = which::which("opencode").ok();
        match path {
            Some(binary_path) => {
                let version = run_cmd("opencode", "--version").unwrap_or_default();
                let home = home_dir();
                Ok(Some(CliInfo {
                    name: "opencode",
                    version,
                    binary_path,
                    skills_dir: Some(project_opencode_skills_dir()),
                    config_dir: Some(home.join(".config").join("opencode")),
                    project_skills_dir: Some(project_opencode_skills_dir()),
                    agents_dir: Some(home.join(".agents")),
                    confidence: Confidence::High,
                }))
            }
            None => Ok(None),
        }
    }

    fn install_plan(&self, skills: &[crate::install::SkillRef]) -> crate::install::InstallPlan {
        use crate::install::{FileOperation, InstallPlan};
        let target = project_opencode_skills_dir();
        let mut ops = vec![];
        for skill in skills {
            ops.push(FileOperation::CreateDir {
                path: target.join(&skill.name),
            });
            ops.push(FileOperation::CopySkill {
                skill_name: skill.name.clone(),
                target_dir: target.join(&skill.name),
            });
            ops.push(FileOperation::WriteToolsJson {
                target_file: target.join(&skill.name).join("tools.json"),
                tool_map: self.tool_map(),
            });
        }
        ops.push(FileOperation::WriteDiscoveryBlock {
            target_file: project_dir().join("AGENTS.md"),
        });
        InstallPlan {
            host: "opencode".to_string(),
            target_skills_dir: target,
            project_orquestra_dir: project_orquestra_dir(),
            operations: ops,
        }
    }
}

#[derive(Debug)]
pub struct AntigravityAdapter;

impl HostAdapter for AntigravityAdapter {
    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Subagents,
            Capability::FileSystem,
            Capability::WebSearch,
        ]
    }

    fn tool_map(&self) -> HashMap<&'static str, &'static str> {
        HashMap::from([
            ("askUser", "skill orquestra-grill"),
            ("spawnSubagent", "subagente nativo"),
            ("readFile", "Read tool"),
            ("writeFile", "Write tool"),
            ("webSearch", "WIE web_search_advanced"),
            ("webFetch", "WIE fetch_page"),
        ])
    }

    fn detect(&self) -> Result<Option<CliInfo>, OrquestraError> {
        let path = which::which("gemini").ok();
        match path {
            Some(binary_path) => {
                let version = run_cmd("gemini", "--version").unwrap_or_default();
                let home = home_dir();
                Ok(Some(CliInfo {
                    name: "antigravity",
                    version,
                    binary_path,
                    skills_dir: Some(project_agents_skills_dir()),
                    config_dir: Some(home.join(".gemini").join("config")),
                    project_skills_dir: Some(project_agents_skills_dir()),
                    agents_dir: Some(home.join(".agents")),
                    confidence: Confidence::Low,
                }))
            }
            None => Ok(None),
        }
    }

    fn install_plan(&self, skills: &[crate::install::SkillRef]) -> crate::install::InstallPlan {
        use crate::install::{FileOperation, InstallPlan};
        let target = project_agents_skills_dir();
        let mut ops = vec![];
        for skill in skills {
            ops.push(FileOperation::CreateDir {
                path: target.join(&skill.name),
            });
            ops.push(FileOperation::CopySkill {
                skill_name: skill.name.clone(),
                target_dir: target.join(&skill.name),
            });
            ops.push(FileOperation::WriteToolsJson {
                target_file: target.join(&skill.name).join("tools.json"),
                tool_map: self.tool_map(),
            });
        }
        ops.push(FileOperation::WriteDiscoveryBlock {
            target_file: project_dir().join("AGENTS.md"),
        });
        InstallPlan {
            host: "antigravity".to_string(),
            target_skills_dir: target,
            project_orquestra_dir: project_orquestra_dir(),
            operations: ops,
        }
    }

    fn notes(&self) -> Vec<&'static str> {
        vec!["Sunset 2026 — may be deprecated"]
    }
}

pub fn all_adapters() -> Vec<Box<dyn HostAdapter>> {
    vec![
        Box::new(CodexAdapter),
        Box::new(ClaudeCodeAdapter),
        Box::new(OpenCodeAdapter),
        Box::new(AntigravityAdapter),
    ]
}

pub fn get_adapter(name: &str) -> Option<Box<dyn HostAdapter>> {
    all_adapters().into_iter().find(|a| a.name() == name)
}

pub fn detect_all_adapters() -> Vec<CliInfo> {
    let mut detected = Vec::new();
    for adapter in all_adapters() {
        match adapter.detect() {
            Ok(Some(info)) => detected.push(info),
            Ok(None) => {}
            Err(e) => tracing::warn!("{}(detect): {e}", adapter.name()),
        }
    }
    detected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileOperation;
    use crate::install::SkillRef;

    #[test]
    fn test_detect_all_returns_vec() {
        let result = detect_all_adapters();
        assert!(result.iter().all(|c| !c.name.is_empty()));
    }

    #[test]
    fn test_codex_adapter_does_not_panic() {
        let adapter = CodexAdapter;
        let result = adapter.detect();
        assert!(result.is_ok());
    }

    #[test]
    fn test_cli_info_serde() {
        let info = CliInfo {
            name: "test",
            version: "1.0".to_string(),
            binary_path: PathBuf::from("/usr/bin/test"),
            skills_dir: Some(PathBuf::from("/home/user/.agents/skills")),
            config_dir: None,
            project_skills_dir: None,
            agents_dir: Some(PathBuf::from("/home/user/.agents")),
            confidence: Confidence::High,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("1.0"));
        assert!(json.contains("High"));
    }

    #[test]
    fn test_each_adapter_has_capabilities() {
        for adapter in all_adapters() {
            let caps = adapter.capabilities();
            assert!(!caps.is_empty(), "{} has no capabilities", adapter.name());
        }
    }

    #[test]
    fn test_each_adapter_has_full_tool_map() {
        for adapter in all_adapters() {
            let map = adapter.tool_map();
            assert!(
                map.contains_key("askUser"),
                "{} missing askUser",
                adapter.name()
            );
            assert!(
                map.contains_key("spawnSubagent"),
                "{} missing spawnSubagent",
                adapter.name()
            );
            assert!(
                map.contains_key("readFile"),
                "{} missing readFile",
                adapter.name()
            );
            assert!(
                map.contains_key("writeFile"),
                "{} missing writeFile",
                adapter.name()
            );
            assert!(
                map.contains_key("webSearch"),
                "{} missing webSearch",
                adapter.name()
            );
            assert!(
                map.contains_key("webFetch"),
                "{} missing webFetch",
                adapter.name()
            );
        }
    }

    #[test]
    fn test_each_adapter_routes_questions_to_the_embedded_orquestra_grill() {
        for adapter in all_adapters() {
            assert_eq!(
                adapter.tool_map().get("askUser"),
                Some(&"skill orquestra-grill"),
                "{} must not depend on an externally installed grill skill",
                adapter.name()
            );
        }
    }

    #[test]
    fn test_opencode_uses_project_local_skills_dir() {
        let adapter = OpenCodeAdapter;
        let skills = vec![SkillRef {
            name: "test".to_string(),
            content: "",
        }];
        let plan = adapter.install_plan(&skills);
        let expected = project_opencode_skills_dir();
        assert_eq!(
            plan.target_skills_dir, expected,
            "OpenCode should install Orquestra skills project-locally"
        );
    }

    #[test]
    fn test_get_adapter_by_name() {
        assert!(get_adapter("opencode").is_some());
        assert!(get_adapter("codex").is_some());
        assert!(get_adapter("claude-code").is_some());
        assert!(get_adapter("antigravity").is_some());
        assert!(get_adapter("unknown").is_none());
    }

    #[test]
    fn test_install_plan_has_expected_operations() {
        let adapter = OpenCodeAdapter;
        let skills = vec![SkillRef {
            name: "test-skill".to_string(),
            content: "# Test\n",
        }];
        let plan = adapter.install_plan(&skills);
        assert_eq!(plan.host, "opencode");
        assert_eq!(plan.operations.len(), 4);
        // create_dir skill, copy_skill, write_tools_json, write_discovery_block
        assert!(matches!(
            plan.operations[0],
            FileOperation::CreateDir { .. }
        ));
        assert!(matches!(
            plan.operations[1],
            FileOperation::CopySkill { .. }
        ));
        assert!(matches!(
            plan.operations[2],
            FileOperation::WriteToolsJson { .. }
        ));
        assert!(matches!(
            plan.operations[3],
            FileOperation::WriteDiscoveryBlock { .. }
        ));
    }

    #[test]
    fn test_install_plan_does_not_write_global_agents_dir() {
        for adapter in all_adapters() {
            let skills = vec![SkillRef {
                name: "test".to_string(),
                content: "",
            }];
            let plan = adapter.install_plan(&skills);
            let writes_global_agents = plan.operations.iter().any(|op| {
                matches!(op, FileOperation::CreateDir { path } if path == &home_dir().join(".agents"))
                    || matches!(op, FileOperation::WriteDiscoveryBlock { target_file } if target_file == &home_dir().join(".agents").join("AGENTS.md"))
            });
            assert!(
                !writes_global_agents,
                "{} install_plan must not modify ~/.agents by default",
                adapter.name()
            );
        }
    }
}
