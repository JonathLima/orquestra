use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TrustLevel {
    UserGlobal,
    UserProject,
    OrquestraBuiltin,
    BrainPending,
    BrainApproved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SkillStatus {
    Active,
    Pending,
    Stale,
    Conflict,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum Provenance {
    Local,
    BrainAdapted { from: String, retrieved_at: String },
    Downloaded { url: String, retrieved_at: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub scope: String,
    pub source_path: PathBuf,
    pub hash: String,
    pub trust: TrustLevel,
    pub status: SkillStatus,
    pub capabilities: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub provenance: Provenance,
    pub inspected_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSource {
    pub scope: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInventory {
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub sources: Vec<ScanSource>,
    pub skills: Vec<SkillInfo>,
}
