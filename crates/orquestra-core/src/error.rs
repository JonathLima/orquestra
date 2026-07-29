use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrquestraError {
    #[error("Config error: {0}")]
    Config(#[from] figment::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("CLI detection failed: {0}")]
    CliDetection(String),

    #[error("Process exited with status {0}")]
    ProcessExit(i32),

    #[error("{0}")]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

impl OrquestraError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Config(_) => 3,
            Self::Io(_) => 4,
            Self::CliDetection(_) => 1,
            Self::ProcessExit(code) => *code,
            Self::Other(_) => 1,
        }
    }
}

impl From<String> for OrquestraError {
    fn from(msg: String) -> Self {
        Self::Other(msg.into())
    }
}

impl From<&str> for OrquestraError {
    fn from(msg: &str) -> Self {
        Self::Other(msg.to_string().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exit_code_config_other() {
        let err = OrquestraError::from(String::from("test config"));
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn test_exit_code_other_from_str() {
        let err = OrquestraError::from("general error");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn test_exit_code_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = OrquestraError::Io(io_err);
        assert_eq!(err.exit_code(), 4);
    }

    #[test]
    fn test_exit_code_cli_detection() {
        let err = OrquestraError::CliDetection("not found".to_string());
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn test_error_display() {
        let err = OrquestraError::from("something broke");
        assert!(err.to_string().contains("something broke"));
    }

    #[test]
    fn test_exit_code_config_from_figment() {
        let err = OrquestraError::Config(figment::Error::from("config parse failed"));
        assert_eq!(err.exit_code(), 3);
    }
}
