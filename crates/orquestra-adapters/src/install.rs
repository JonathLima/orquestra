use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct SkillRef {
    pub name: String,
    pub content: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallPlan {
    pub host: String,
    pub target_skills_dir: PathBuf,
    pub project_orquestra_dir: PathBuf,
    pub operations: Vec<FileOperation>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum FileOperation {
    CreateDir {
        path: PathBuf,
    },
    CopySkill {
        skill_name: String,
        target_dir: PathBuf,
    },
    WriteToolsJson {
        target_file: PathBuf,
        tool_map: HashMap<&'static str, &'static str>,
    },
    WriteDiscoveryBlock {
        target_file: PathBuf,
    },
}
