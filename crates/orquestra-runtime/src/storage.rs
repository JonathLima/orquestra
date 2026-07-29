use crate::error::RuntimeError;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(25);
const LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const STALE_LOCK_AFTER: Duration = Duration::from_secs(30 * 60);

pub fn sessions_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join("sessions")
}

pub fn locks_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join("locks")
}

pub fn validate_session_id(id: &str) -> Result<(), RuntimeError> {
    let parsed =
        uuid::Uuid::parse_str(id).map_err(|_| RuntimeError::InvalidSessionId(id.to_string()))?;
    if parsed.get_version() != Some(uuid::Version::Random) {
        return Err(RuntimeError::InvalidSessionId(id.to_string()));
    }
    Ok(())
}

pub fn validate_ticket_id(ticket_id: &str) -> Result<(), RuntimeError> {
    let path = Path::new(ticket_id);
    let mut components = path.components();
    let is_single_normal_component =
        matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none();
    if ticket_id.is_empty()
        || ticket_id == "."
        || ticket_id == ".."
        || path.is_absolute()
        || ticket_id.contains(['/', '\\'])
        || !is_single_normal_component
    {
        return Err(RuntimeError::InvalidTicketId(ticket_id.to_string()));
    }
    Ok(())
}

pub fn validate_relative_path(path: &str) -> Result<(), RuntimeError> {
    let path = Path::new(path);
    let has_portable_escape = path
        .to_string_lossy()
        .split(['/', '\\'])
        .any(|component| component == "." || component == "..");
    if path.as_os_str().is_empty() || path.is_absolute() || has_portable_escape {
        return Err(RuntimeError::Other(format!(
            "path must be project-relative: {}",
            path.display()
        )));
    }
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            _ => {
                return Err(RuntimeError::Other(format!(
                    "path contains unsafe component: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

pub fn ensure_no_symlink_ancestors(path: &Path) -> Result<(), RuntimeError> {
    let project_boundary = path
        .ancestors()
        .find(|candidate| candidate.file_name() == Some(OsStr::new(".orquestra")))
        .and_then(Path::parent)
        .map(Path::to_path_buf);

    for candidate in path.ancestors() {
        let reached_boundary = project_boundary.as_deref() == Some(candidate);
        let metadata = match std::fs::symlink_metadata(candidate) {
            Ok(m) => m,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if reached_boundary {
                    break;
                }
                continue;
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() {
            return Err(RuntimeError::Other(format!(
                "Refusing to access path through symlink: {}",
                candidate.display()
            )));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(RuntimeError::Other(format!(
                    "Refusing to access path through reparse point (junction/mount): {}",
                    candidate.display()
                )));
            }
        }
        if reached_boundary {
            break;
        }
    }
    Ok(())
}

pub fn session_dir(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    validate_session_id(id)?;
    Ok(sessions_dir(project_dir).join(id))
}

pub fn session_file(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    Ok(session_dir(project_dir, id)?.join("session.json"))
}

pub fn plan_file(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    Ok(session_dir(project_dir, id)?.join("plan.json"))
}

pub fn waves_file(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    Ok(session_dir(project_dir, id)?.join("waves.json"))
}

pub fn ticket_file(project_dir: &Path, id: &str, ticket_id: &str) -> Result<PathBuf, RuntimeError> {
    validate_ticket_id(ticket_id)?;
    Ok(session_dir(project_dir, id)?
        .join("tickets")
        .join(format!("{ticket_id}.json")))
}

pub fn events_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join("events")
}

pub fn event_log_file(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    validate_session_id(id)?;
    Ok(events_dir(project_dir).join(format!("{id}.jsonl")))
}

pub fn session_lock_file(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    validate_session_id(id)?;
    Ok(locks_dir(project_dir).join(format!("{id}.lock")))
}

pub fn named_lock_file(project_dir: &Path, name: &str) -> Result<PathBuf, RuntimeError> {
    validate_ticket_id(name)?;
    Ok(locks_dir(project_dir).join(format!("{name}.lock")))
}

pub struct SessionLock {
    path: PathBuf,
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn acquire_session_lock(project_dir: &Path, id: &str) -> Result<SessionLock, RuntimeError> {
    let path = session_lock_file(project_dir, id)?;
    acquire_lock_file(path)
}

pub fn acquire_named_lock(project_dir: &Path, name: &str) -> Result<SessionLock, RuntimeError> {
    let path = named_lock_file(project_dir, name)?;
    acquire_lock_file(path)
}

fn acquire_lock_file(path: PathBuf) -> Result<SessionLock, RuntimeError> {
    ensure_no_symlink_ancestors(&path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let started = Instant::now();
    loop {
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                writeln!(
                    file,
                    "pid={} acquired_at={}",
                    std::process::id(),
                    crate::iso_now()
                )?;
                file.sync_all()?;
                return Ok(SessionLock { path });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                remove_stale_lock_if_safe(&path)?;
                if started.elapsed() >= LOCK_TIMEOUT {
                    return Err(RuntimeError::Other(format!(
                        "Timed out waiting for session lock {}",
                        path.display()
                    )));
                }
                std::thread::sleep(LOCK_RETRY_DELAY);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

fn remove_stale_lock_if_safe(path: &Path) -> Result<(), RuntimeError> {
    let metadata = std::fs::metadata(path)?;
    let Ok(modified) = metadata.modified() else {
        return Ok(());
    };
    let Ok(age) = modified.elapsed() else {
        return Ok(());
    };
    if age >= STALE_LOCK_AFTER {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

pub fn checkpoints_dir(project_dir: &Path, id: &str) -> Result<PathBuf, RuntimeError> {
    validate_session_id(id)?;
    Ok(project_dir.join(".orquestra").join("checkpoints").join(id))
}

pub fn checkpoint_file(project_dir: &Path, id: &str, wave: u32) -> Result<PathBuf, RuntimeError> {
    Ok(checkpoints_dir(project_dir, id)?.join(format!("wave-{wave}.json")))
}

fn unique_tmp_path(path: &Path) -> Result<PathBuf, RuntimeError> {
    let file_name = path
        .file_name()
        .ok_or_else(|| RuntimeError::Other(format!("Path has no filename: {}", path.display())))?;
    let mut tmp_name = OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(format!(
        ".{}.{}.tmp",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    Ok(path.with_file_name(tmp_name))
}

/// Atomically write JSON data: write to .tmp, fsync, rename, then fsync the
/// parent directory so the new directory entry persists across a crash.
pub fn atomic_write_json<T: serde::Serialize>(path: &Path, data: &T) -> Result<(), RuntimeError> {
    let json = serde_json::to_vec_pretty(data)?;
    ensure_no_symlink_ancestors(path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = unique_tmp_path(path)?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)?;
        file.write_all(&json)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    } else if let Some(parent) = path.parent()
        && let Ok(dir_handle) = std::fs::File::open(parent)
    {
        let _ = dir_handle.sync_all();
    }
    result
}

/// Read JSON data from a file.
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, RuntimeError> {
    let content = read_text_limited(path, MAX_JSON_BYTES)?;
    let data = serde_json::from_str(&content)?;
    Ok(data)
}

pub fn read_text_limited(path: &Path, max_bytes: u64) -> Result<String, RuntimeError> {
    ensure_no_symlink_ancestors(path)?;
    let metadata = std::fs::metadata(path)?;
    if metadata.len() > max_bytes {
        return Err(RuntimeError::Other(format!(
            "{} is too large: {} bytes exceeds {} bytes",
            path.display(),
            metadata.len(),
            max_bytes
        )));
    }
    let file = std::fs::File::open(path)?;
    let mut content = String::new();
    file.take(max_bytes + 1).read_to_string(&mut content)?;
    if content.len() as u64 > max_bytes {
        return Err(RuntimeError::Other(format!(
            "{} exceeds maximum size of {} bytes",
            path.display(),
            max_bytes
        )));
    }
    Ok(content)
}

pub fn list_sessions(project_dir: &Path) -> Result<Vec<String>, RuntimeError> {
    let dir = sessions_dir(project_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir()
            && let Some(name) = entry.file_name().to_str()
        {
            sessions.push(name.to_string());
        }
    }
    sessions.sort();
    Ok(sessions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_does_not_reuse_deterministic_tmp_path() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let state_dir = dir.path().join(".orquestra");
        std::fs::create_dir(&state_dir).expect("create state dir");
        let path = state_dir.join("state.json");
        let old_tmp_path = path.with_extension("tmp");
        std::fs::write(&old_tmp_path, "sentinel").expect("write sentinel");

        atomic_write_json(&path, &serde_json::json!({"state": "written"}))
            .expect("write JSON atomically");

        assert_eq!(
            std::fs::read_to_string(old_tmp_path).expect("read sentinel"),
            "sentinel"
        );
        let written: serde_json::Value = read_json(&path).expect("read written JSON");
        assert_eq!(written, serde_json::json!({"state": "written"}));
    }

    #[test]
    fn session_paths_reject_non_v4_ids() {
        let project_dir = Path::new("project");

        for id in [
            "",
            "../escape",
            "not-a-uuid",
            &uuid::Uuid::nil().to_string(),
        ] {
            assert!(session_file(project_dir, id).is_err(), "accepted {id:?}");
            assert!(event_log_file(project_dir, id).is_err(), "accepted {id:?}");
            assert!(
                checkpoint_file(project_dir, id, 1).is_err(),
                "accepted {id:?}"
            );
        }
    }

    #[test]
    fn ticket_paths_reject_unsafe_filename_ids() {
        let project_dir = Path::new("project");
        let session_id = uuid::Uuid::new_v4().to_string();

        for ticket_id in ["", ".", "..", "../escape", "a/b", r"a\b", "/absolute"] {
            assert!(
                ticket_file(project_dir, &session_id, ticket_id).is_err(),
                "accepted {ticket_id:?}"
            );
        }

        assert!(ticket_file(project_dir, &session_id, "T1").is_ok());
    }

    #[test]
    fn relative_paths_reject_escape_components() {
        for path in ["", "../x", "a/../x", "/absolute", r"a\..\x"] {
            assert!(validate_relative_path(path).is_err(), "accepted {path}");
        }

        assert!(validate_relative_path("reports/test.log").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn ensure_no_symlink_ancestors_rejects_real_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("create temp dir");
        let real_dir = dir.path().join("real");
        std::fs::create_dir(&real_dir).expect("create real dir");
        let link_dir = dir.path().join("link-to-real");
        symlink(&real_dir, &link_dir).expect("create symlink");

        let through_link = link_dir.join("state.json");
        let result = ensure_no_symlink_ancestors(&through_link);
        assert!(
            result.is_err(),
            "expected symlink rejection, got {result:?}"
        );
        let message = format!("{}", result.unwrap_err());
        assert!(
            message.contains("symlink"),
            "expected symlink error message, got: {message}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn ensure_no_symlink_ancestors_accepts_normal_paths() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let state_dir = dir.path().join(".orquestra");
        std::fs::create_dir(&state_dir).expect("create state dir");
        let sub = state_dir.join("nested");
        std::fs::create_dir(&sub).expect("create nested dir");

        let deep = sub.join("a").join("b").join("c.json");
        assert!(ensure_no_symlink_ancestors(&deep).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_json_rejects_through_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("create temp dir");
        let target_dir = dir.path().join("target");
        std::fs::create_dir(&target_dir).expect("create target dir");
        let link_dir = dir.path().join("link");
        symlink(&target_dir, &link_dir).expect("create symlink");

        let through_link = link_dir.join("state.json");
        let result = atomic_write_json(&through_link, &serde_json::json!({"k": "v"}));
        assert!(result.is_err(), "expected rejection through symlink");
    }

    #[cfg(unix)]
    #[test]
    fn session_lock_rejects_through_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().expect("create temp dir");
        let target_dir = dir.path().join("target");
        std::fs::create_dir(&target_dir).expect("create target dir");
        let link_dir = dir.path().join("link");
        symlink(&target_dir, &link_dir).expect("create symlink");

        let session_id = uuid::Uuid::new_v4().to_string();
        let result = acquire_session_lock(&link_dir, &session_id);
        assert!(result.is_err(), "expected lock rejection through symlink");
    }

    #[cfg(windows)]
    mod windows_reparse_tests {
        use super::*;

        fn can_create_symlinks() -> bool {
            let dir = tempfile::tempdir().expect("create temp dir");
            let target = dir.path().join("real_target");
            let link = dir.path().join("test_link");
            let _ = std::fs::create_dir_all(&target);
            std::os::windows::fs::symlink_dir(&target, &link).is_ok()
        }

        fn create_junction(target: &Path, link: &Path) -> std::io::Result<()> {
            std::fs::create_dir_all(target)?;
            std::os::windows::fs::symlink_dir(target, link)
        }

        #[test]
        fn ensure_no_symlink_ancestors_rejects_junction() {
            if !can_create_symlinks() {
                eprintln!("skipping: no symlink privilege");
                return;
            }
            let dir = tempfile::tempdir().expect("create temp dir");
            let real = dir.path().join("real");
            std::fs::create_dir_all(&real).expect("create real dir");
            let link = dir.path().join("junction");
            create_junction(&real, &link).expect("create junction");
            let through_junction = link.join("child");
            std::fs::create_dir_all(&through_junction).expect("create child dir");
            let result = ensure_no_symlink_ancestors(&through_junction);
            assert!(
                result.is_err(),
                "expected junction rejection, got {result:?}"
            );
            let message = format!("{}", result.unwrap_err());
            assert!(
                message.contains("symlink") || message.contains("reparse point"),
                "expected redirected-path error message, got: {message}"
            );
        }

        #[test]
        fn atomic_write_json_rejects_through_junction_parent() {
            if !can_create_symlinks() {
                eprintln!("skipping: no symlink privilege");
                return;
            }
            let dir = tempfile::tempdir().expect("create temp dir");
            let target_dir = dir.path().join("target");
            let link_dir = dir.path().join("junction");
            create_junction(&target_dir, &link_dir).expect("create junction");
            let through_junction = link_dir.join("state.json");
            let result = atomic_write_json(&through_junction, &serde_json::json!({"k": "v"}));
            assert!(result.is_err(), "expected rejection through junction");
        }

        #[test]
        fn session_lock_rejects_through_junction() {
            if !can_create_symlinks() {
                eprintln!("skipping: no symlink privilege");
                return;
            }
            let dir = tempfile::tempdir().expect("create temp dir");
            let target_dir = dir.path().join("target");
            std::fs::create_dir_all(&target_dir).expect("create target dir");
            let link_dir = dir.path().join("junction");
            create_junction(&target_dir, &link_dir).expect("create junction");
            let session_id = uuid::Uuid::new_v4().to_string();
            let result = acquire_session_lock(&link_dir, &session_id);
            assert!(result.is_err(), "expected lock rejection through junction");
        }
    }
}
