use thiserror::Error;

#[derive(Debug, Error)]
pub enum InitError {
    #[error("Invalid init session ID {0:?}: must be a UUID v4")]
    InvalidSessionId(String),

    #[error("Init session not found: {0}")]
    SessionNotFound(String),

    #[error("Init session already exists: {0}")]
    SessionAlreadyExists(String),

    #[error("Invalid state transition: {0}")]
    InvalidTransition(String),

    #[error("Path traversal detected: {0}")]
    PathTraversal(String),

    #[error("Path escapes init directory: {0}")]
    PathEscape(String),

    #[error("Artifact path {0:?} is not within the artifacts directory")]
    ArtifactPathOutside(String),

    #[error("Convergence failed: {0}")]
    ConvergenceFailed(String),

    #[error("Research error: {0}")]
    Research(String),

    #[error("Artifact generation error: {0}")]
    Artifact(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Other(String),
}

impl From<String> for InitError {
    fn from(msg: String) -> Self {
        Self::Other(msg)
    }
}

impl From<&str> for InitError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_context() {
        let err = InitError::ConvergenceFailed("contradiction unresolved".to_string());
        assert!(err.to_string().contains("contradiction unresolved"));
    }

    #[test]
    fn from_str_and_string() {
        let from_str = InitError::from("oops");
        let from_string = InitError::from(String::from("oops"));
        assert!(from_str.to_string().contains("oops"));
        assert!(from_string.to_string().contains("oops"));
    }

    #[test]
    fn io_error_converts() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        let err: InitError = io_err.into();
        assert!(err.to_string().contains("missing"));
    }
}
