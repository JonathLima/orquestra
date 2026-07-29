use std::path::{Path, PathBuf};

use crate::error::InitError;
use crate::state::InitState;

const INIT_DIR: &str = "init";
const STATE_FILE: &str = "state.json";
const METRICS_FILE: &str = "metrics.json";
const EVENTS_FILE: &str = "events.jsonl";

fn init_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join(INIT_DIR)
}

pub fn session_dir(project_dir: &Path, id: &str) -> Result<PathBuf, InitError> {
    validate_init_id(id)?;
    let dir = init_dir(project_dir).join(id);
    if dir.canonicalize().is_ok() || !dir.exists() {
        Ok(dir)
    } else {
        Err(InitError::PathTraversal(format!(
            "Init session directory {dir:?} is not within the init directory"
        )))
    }
}

pub fn state_file(project_dir: &Path, id: &str) -> Result<PathBuf, InitError> {
    validate_init_id(id)?;
    let path = init_dir(project_dir).join(id).join(STATE_FILE);
    if path.canonicalize().is_ok() || !path.exists() {
        Ok(path)
    } else {
        Err(InitError::PathTraversal(format!(
            "State file {path:?} escapes the init directory"
        )))
    }
}

pub fn metrics_file(project_dir: &Path, id: &str) -> Result<PathBuf, InitError> {
    validate_init_id(id)?;
    Ok(init_dir(project_dir).join(id).join(METRICS_FILE))
}

pub fn events_file(project_dir: &Path, id: &str) -> Result<PathBuf, InitError> {
    validate_init_id(id)?;
    Ok(init_dir(project_dir).join(id).join(EVENTS_FILE))
}

pub fn validate_init_id(id: &str) -> Result<(), InitError> {
    if id.is_empty() || id == "." || id == ".." {
        return Err(InitError::InvalidSessionId(id.to_string()));
    }
    if id.contains('/') || id.contains('\\') {
        return Err(InitError::PathTraversal(format!(
            "Init session ID {id:?} contains path separators"
        )));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(InitError::InvalidSessionId(id.to_string()));
    }
    Ok(())
}

pub fn list_sessions(project_dir: &Path) -> Result<Vec<String>, InitError> {
    let dir = init_dir(project_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| InitError::from(format!("Cannot list init sessions: {e}")))?
    {
        let entry =
            entry.map_err(|e| InitError::from(format!("Cannot read init session entry: {e}")))?;
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();
            if state_file(project_dir, &name_str).is_ok() {
                sessions.push(name_str);
            }
        }
    }
    sessions.sort();
    Ok(sessions)
}

pub fn load_state(project_dir: &Path, id: &str) -> Result<InitState, InitError> {
    let path = state_file(project_dir, id)?;
    if !path.exists() {
        return Err(InitError::SessionNotFound(id.to_string()));
    }
    let content = std::fs::read_to_string(&path)?;
    let state: InitState = serde_json::from_str(&content)?;
    Ok(state)
}

pub fn save_state(project_dir: &Path, state: &InitState) -> Result<(), InitError> {
    let dir = session_dir(project_dir, &state.id)?;
    std::fs::create_dir_all(&dir)?;
    let path = state_file(project_dir, &state.id)?;
    let json = serde_json::to_string_pretty(state)?;
    atomic_write(&path, json.as_bytes())?;
    Ok(())
}

pub fn save_metrics_json(
    project_dir: &Path,
    id: &str,
    metrics: &serde_json::Value,
) -> Result<(), InitError> {
    validate_init_id(id)?;
    let dir = init_dir(project_dir).join(id);
    std::fs::create_dir_all(&dir)?;
    let path = metrics_file(project_dir, id)?;
    let json = serde_json::to_string_pretty(metrics)?;
    atomic_write(&path, json.as_bytes())?;
    Ok(())
}

pub fn append_event(
    project_dir: &Path,
    id: &str,
    event: &str,
    data: &serde_json::Value,
) -> Result<(), InitError> {
    validate_init_id(id)?;
    let dir = init_dir(project_dir).join(id);
    std::fs::create_dir_all(&dir)?;
    let path = events_file(project_dir, id)?;
    let entry = serde_json::json!({
        "ts": chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        "event": event,
        "data": data,
    });
    let line = serde_json::to_string(&entry)? + "\n";
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

pub fn ensure_init_dirs(project_dir: &Path) -> Result<(), InitError> {
    let dir = init_dir(project_dir);
    std::fs::create_dir_all(&dir)?;
    Ok(())
}

pub(crate) fn atomic_write(path: &Path, data: &[u8]) -> Result<(), InitError> {
    let tmp_path = path.with_extension("tmp");
    std::fs::write(&tmp_path, data)?;
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, InitError> {
    let content = std::fs::read_to_string(path)?;
    let value: T = serde_json::from_str(&content)?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::InitState;

    fn tmp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn test_state(id: &str) -> InitState {
        InitState::new(id.to_string(), "opencode".to_string(), "test".to_string())
    }

    #[test]
    fn validate_init_id_rejects_empty() {
        assert!(validate_init_id("").is_err());
    }

    #[test]
    fn validate_init_id_rejects_traversal() {
        assert!(validate_init_id("../escape").is_err());
        assert!(validate_init_id("sub/../escape").is_err());
        assert!(validate_init_id("sub\\..").is_err());
    }

    #[test]
    fn validate_init_id_rejects_dot_and_dotdot() {
        assert!(validate_init_id(".").is_err());
        assert!(validate_init_id("..").is_err());
    }

    #[test]
    fn validate_init_id_accepts_valid() {
        assert!(validate_init_id("init-2026-07-28-XYZ").is_ok());
        assert!(validate_init_id("test_session_1").is_ok());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tmp_dir();
        let state = test_state("test-id-1");

        save_state(dir.path(), &state).expect("save state");
        let loaded = load_state(dir.path(), "test-id-1").expect("load state");

        assert_eq!(loaded.id, state.id);
        assert_eq!(loaded.host, state.host);
        assert_eq!(loaded.idea, state.idea);
    }

    #[test]
    fn load_nonexistent_returns_not_found() {
        let dir = tmp_dir();
        let err = load_state(dir.path(), "nonexistent").unwrap_err();
        assert!(matches!(err, InitError::SessionNotFound(_)));
    }

    #[test]
    fn list_sessions_empty_when_no_init_dir() {
        let dir = tmp_dir();
        let sessions = list_sessions(dir.path()).expect("list sessions");
        assert!(sessions.is_empty());
    }

    #[test]
    fn list_sessions_returns_saved_sessions() {
        let dir = tmp_dir();
        let state1 = test_state("session-1");
        let state2 = test_state("session-2");

        save_state(dir.path(), &state1).expect("save state 1");
        save_state(dir.path(), &state2).expect("save state 2");

        let sessions = list_sessions(dir.path()).expect("list sessions");
        assert!(sessions.contains(&"session-1".to_string()));
        assert!(sessions.contains(&"session-2".to_string()));
    }

    #[test]
    fn session_dir_creates_directory() {
        let dir = tmp_dir();
        let sd = session_dir(dir.path(), "test-id").expect("session dir");
        assert!(!sd.exists());
        std::fs::create_dir_all(&sd).expect("create session dir");
        assert!(sd.exists());
    }

    #[test]
    fn save_state_creates_dir_and_file() {
        let dir = tmp_dir();
        let state = test_state("test-save-dir");
        save_state(dir.path(), &state).expect("save state");
        let file = state_file(dir.path(), "test-save-dir").expect("state file");
        assert!(file.exists());
    }

    #[test]
    fn append_event_writes_line() {
        let dir = tmp_dir();
        append_event(
            dir.path(),
            "test-event-id",
            "test_event",
            &serde_json::json!({"key": "value"}),
        )
        .expect("append event");
        let path = events_file(dir.path(), "test-event-id").expect("events file");
        let content = std::fs::read_to_string(&path).expect("read events");
        assert!(content.contains("test_event"));
    }

    #[test]
    fn atomic_write_is_atomic() {
        let dir = tmp_dir();
        let path = dir.path().join("target.json");
        atomic_write(&path, b"hello").expect("atomic write");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "hello");
    }

    #[test]
    fn read_json_parses_existing() {
        let dir = tmp_dir();
        let path = dir.path().join("data.json");
        std::fs::write(&path, r#"{"valid": true}"#).expect("write json");
        let value: serde_json::Value = read_json(&path).expect("read json");
        assert_eq!(value["valid"], true);
    }

    #[test]
    fn ensure_init_dirs_creates_orquestra_init() {
        let dir = tmp_dir();
        ensure_init_dirs(dir.path()).expect("ensure dirs");
        assert!(dir.path().join(".orquestra").join("init").exists());
    }

    #[test]
    fn save_metrics_json_persists_metrics() {
        let dir = tmp_dir();
        let metrics = serde_json::json!({"round": 3, "tokens": 4000});
        save_metrics_json(dir.path(), "test-metrics", &metrics).expect("save metrics");
        let path = metrics_file(dir.path(), "test-metrics").expect("metrics file");
        assert!(path.exists());
        let content = std::fs::read_to_string(&path).expect("read metrics");
        assert!(content.contains("round"));
    }
}
