use crate::error::RuntimeError;
use crate::types::SessionEvent;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

const MAX_EVENT_LOG_BYTES: u64 = 64 * 1024 * 1024;

pub fn append_event(log_path: &Path, event: &SessionEvent) -> Result<(), RuntimeError> {
    crate::storage::ensure_no_symlink_ancestors(log_path)?;
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let line = serde_json::to_string(event)? + "\n";
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(line.as_bytes())?;
    file.sync_data()?;
    Ok(())
}

pub fn read_events(log_path: &Path) -> Result<Vec<SessionEvent>, RuntimeError> {
    if !log_path.exists() {
        return Ok(vec![]);
    }
    let content = crate::storage::read_text_limited(log_path, MAX_EVENT_LOG_BYTES)?;
    let mut events = Vec::new();
    for line in content.lines() {
        if !line.trim().is_empty() {
            let event: SessionEvent = serde_json::from_str(line)?;
            events.push(event);
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_event() -> SessionEvent {
        SessionEvent {
            ts: "2026-07-27T12:00:00.000Z".to_string(),
            session_id: uuid::Uuid::new_v4().to_string(),
            event: "test_event".to_string(),
            data: serde_json::json!({}),
        }
    }

    #[test]
    fn append_event_writes_one_json_line() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let log_path = dir.path().join("events.jsonl");

        append_event(&log_path, &test_event()).expect("append event");

        let content = std::fs::read_to_string(&log_path).expect("read event log");
        assert_eq!(content.lines().count(), 1);
        assert_eq!(read_events(&log_path).expect("read event log").len(), 1);
    }
}
