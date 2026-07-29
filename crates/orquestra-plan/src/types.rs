use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub schema_version: u32,
    pub title: String,
    #[serde(default)]
    pub model_policy: Option<ModelPolicy>,
    pub tickets: Vec<Ticket>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    #[serde(default)]
    pub preferred_capabilities: Vec<String>,
    pub assigned_skill: Option<String>,
    #[serde(default)]
    pub model_policy: Option<ModelPolicy>,
    #[serde(default)]
    pub model_recommendation: Option<ModelRecommendation>,
    #[serde(default)]
    pub verification: VerificationPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelPolicy {
    #[serde(default = "default_quality_target")]
    pub quality_target: String,
    #[serde(default = "default_cost_sensitivity")]
    pub cost_sensitivity: String,
    #[serde(default)]
    pub allow_web_research: bool,
    #[serde(default)]
    pub prefer_local_only: bool,
    #[serde(default)]
    pub default_host: Option<String>,
}

impl Default for ModelPolicy {
    fn default() -> Self {
        Self {
            quality_target: default_quality_target(),
            cost_sensitivity: default_cost_sensitivity(),
            allow_web_research: false,
            prefer_local_only: false,
            default_host: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelTier {
    Fast,
    Balanced,
    Frontier,
}

impl ModelTier {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
            Self::Frontier => "frontier",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelRecommendation {
    pub ticket_id: String,
    pub host: String,
    pub model: String,
    pub tier: ModelTier,
    pub reasoning_effort: ReasoningEffort,
    pub web_required: bool,
    pub estimated_cost_class: String,
    pub quality_risk: u8,
    pub reason: String,
    pub resolved_at: String,
    #[serde(default)]
    pub confirmed: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub valid_until: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationPolicy {
    #[serde(default = "default_min_score")]
    pub minimum_score: f64,
    #[serde(default)]
    pub required_evidence: Vec<String>,
}

impl Default for VerificationPolicy {
    fn default() -> Self {
        Self {
            minimum_score: default_min_score(),
            required_evidence: vec![],
        }
    }
}

fn default_min_score() -> f64 {
    0.95
}

fn default_quality_target() -> String {
    "max".to_string()
}

fn default_cost_sensitivity() -> String {
    "balanced".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub code: String,
    pub message: String,
    pub ticket_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wave {
    pub wave_number: u32,
    pub ticket_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaveResult {
    pub waves: Vec<Wave>,
    pub total_waves: u32,
    pub total_tickets: usize,
}
