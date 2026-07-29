use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlanError {
    #[error("Invalid plan: {0}")]
    Invalid(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl From<String> for PlanError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for PlanError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_string())
    }
}
