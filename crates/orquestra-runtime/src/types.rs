use orquestra_plan::ModelRecommendation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionStatus {
    Created,
    Running,
    Checkpoint,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TicketStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TicketState {
    pub id: String,
    pub status: TicketStatus,
    pub wave: u32,
    pub assigned_skill: Option<String>,
    pub model_recommendation: Option<ModelRecommendation>,
    pub dispatch_attempt_id: Option<String>,
    pub output: Option<String>,
    pub evidence: Vec<String>,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub plan_title: String,
    pub status: SessionStatus,
    pub total_waves: u32,
    pub current_wave: u32,
    pub created_at: String,
    pub updated_at: String,
    pub ticket_states: HashMap<String, TicketState>,
    #[serde(default)]
    pub inventory_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub wave_number: u32,
    pub created_at: String,
    pub approved: Option<bool>,
    pub approved_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub ts: String,
    pub session_id: String,
    pub event: String,
    pub data: serde_json::Value,
}
