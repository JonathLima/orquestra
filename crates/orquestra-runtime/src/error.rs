use thiserror::Error;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("Invalid session ID {0:?}: session ID must be a UUID v4")]
    InvalidSessionId(String),

    #[error("Invalid ticket ID {0:?}: ticket ID must be a safe filename")]
    InvalidTicketId(String),

    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Invalid state transition: {0}")]
    InvalidTransition(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl From<String> for RuntimeError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for RuntimeError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_string())
    }
}

impl From<orquestra_config::ConfigError> for RuntimeError {
    fn from(e: orquestra_config::ConfigError) -> Self {
        Self::Other(format!("config error: {e}"))
    }
}

impl From<orquestra_config::profile::ProfileError> for RuntimeError {
    fn from(e: orquestra_config::profile::ProfileError) -> Self {
        Self::Other(format!("profile error: {e}"))
    }
}
