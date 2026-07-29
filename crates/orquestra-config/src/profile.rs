use crate::{ArtifactVerification, ProfileResult, VerificationProfile};
use std::path::Path;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Execution failed: {0}")]
    Execution(String),
    #[error("Timeout after {0}s")]
    Timeout(u64),
    #[error("Output truncated")]
    OutputTruncated,
}

pub fn execute_profile(
    profile: &VerificationProfile,
    ticket_dir: &Path,
) -> Result<ProfileResult, ProfileError> {
    let start = Instant::now();

    let working_dir = match &profile.relative_dir {
        Some(rel) => ticket_dir.join(rel),
        None => ticket_dir.to_path_buf(),
    };

    let mut cmd = Command::new(&profile.argv[0]);
    if profile.argv.len() > 1 {
        cmd.args(&profile.argv[1..]);
    }
    cmd.current_dir(&working_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    for pair in &profile.env {
        if let Some((k, v)) = pair.split_once('=') {
            cmd.env(k, v);
        }
    }

    let mut child = cmd.spawn().map_err(|e| {
        ProfileError::Execution(format!("failed to spawn {}: {e}", profile.argv[0]))
    })?;

    let max_output = profile.max_output_bytes.unwrap_or(16 * 1024 * 1024) as usize;
    let timeout = profile.timeout_seconds.map(std::time::Duration::from_secs);

    let (output, timed_out) = if let Some(timeout_dur) = timeout {
        let deadline = Instant::now() + timeout_dur;
        loop {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ProfileError::Timeout(timeout_dur.as_secs()));
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(e) => return Err(ProfileError::Execution(e.to_string())),
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|e| ProfileError::Execution(format!("wait failed: {e}")))?;
        (output, false)
    } else {
        let output = child
            .wait_with_output()
            .map_err(|e| ProfileError::Execution(format!("wait failed: {e}")))?;
        (output, false)
    };

    if timed_out {
        return Err(ProfileError::Timeout(timeout.unwrap().as_secs()));
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let (stdout, stderr) = if stdout.len() > max_output || stderr.len() > max_output {
        (
            truncate_utf8(&stdout, max_output),
            truncate_utf8(&stderr, max_output),
        )
    } else {
        (stdout.to_string(), stderr.to_string())
    };

    let artifacts: Vec<ArtifactVerification> = profile
        .expected_artifacts
        .iter()
        .map(|a| {
            let abs_path = ticket_dir.join(a);
            let exists = abs_path.exists();
            let size = if exists {
                abs_path.metadata().ok().map(|m| m.len())
            } else {
                None
            };
            ArtifactVerification {
                path: a.clone(),
                exists,
                size_bytes: size,
                content_hash: None,
            }
        })
        .collect();

    Ok(ProfileResult {
        profile_name: profile.name.clone(),
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        artifacts,
        duration_ms,
        timestamp: chrono::Utc::now().to_rfc3339(),
    })
}

fn truncate_utf8(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut truncated = String::new();
    for c in s.chars() {
        if truncated.len() + c.len_utf8() > max.saturating_sub(3) {
            break;
        }
        truncated.push(c);
    }
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::VerificationProfile;

    #[cfg(windows)]
    fn shell_argv(command: &str) -> Vec<String> {
        vec!["cmd".to_string(), "/c".to_string(), command.to_string()]
    }

    #[cfg(not(windows))]
    fn shell_argv(command: &str) -> Vec<String> {
        vec!["sh".to_string(), "-c".to_string(), command.to_string()]
    }

    #[cfg(windows)]
    const SLOW_COMMAND: &str = "ping -n 5 127.0.0.1 >NUL";
    #[cfg(not(windows))]
    const SLOW_COMMAND: &str = "sleep 5";

    #[cfg(windows)]
    const ECHO_ENV_COMMAND: &str = "echo %MY_VAR%";
    #[cfg(not(windows))]
    const ECHO_ENV_COMMAND: &str = "echo $MY_VAR";

    fn echo_profile() -> VerificationProfile {
        VerificationProfile {
            name: "echo".to_string(),
            argv: shell_argv("echo hello"),
            relative_dir: None,
            timeout_seconds: None,
            max_output_bytes: Some(1024),
            expected_exit_code: Some(0),
            expected_artifacts: vec![],
            env: vec![],
        }
    }

    #[test]
    fn execute_echo_profile() {
        let profile = echo_profile();
        let dir = tempfile::tempdir().unwrap();
        let result = execute_profile(&profile, dir.path()).unwrap();
        assert_eq!(result.profile_name, "echo");
        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"), "stdout: {}", result.stdout);
    }

    #[test]
    fn execute_failing_profile() {
        let profile = VerificationProfile {
            name: "fail".to_string(),
            argv: shell_argv("exit 1"),
            relative_dir: None,
            timeout_seconds: None,
            max_output_bytes: None,
            expected_exit_code: Some(0),
            expected_artifacts: vec![],
            env: vec![],
        };
        let dir = tempfile::tempdir().unwrap();
        let result = execute_profile(&profile, dir.path()).unwrap();
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn execute_profile_with_timeout() {
        let profile = VerificationProfile {
            name: "sleep".to_string(),
            argv: shell_argv(SLOW_COMMAND),
            relative_dir: None,
            timeout_seconds: Some(1),
            max_output_bytes: None,
            expected_exit_code: None,
            expected_artifacts: vec![],
            env: vec![],
        };
        let dir = tempfile::tempdir().unwrap();
        let err = execute_profile(&profile, dir.path()).unwrap_err();
        assert!(matches!(err, ProfileError::Timeout(1)));
    }

    #[test]
    fn execute_profile_with_bounded_output() {
        let profile = VerificationProfile {
            name: "big-output".to_string(),
            argv: shell_argv("echo abcdefghij"),
            relative_dir: None,
            timeout_seconds: None,
            max_output_bytes: Some(5),
            expected_exit_code: None,
            expected_artifacts: vec![],
            env: vec![],
        };
        let dir = tempfile::tempdir().unwrap();
        let result = execute_profile(&profile, dir.path()).unwrap();
        assert!(
            result.stdout.ends_with("..."),
            "expected truncated, got: {}",
            result.stdout
        );
        assert!(result.stdout.len() <= 8, "len: {}", result.stdout.len());
    }

    #[test]
    fn expected_artifacts_missing() {
        let dir = tempfile::tempdir().unwrap();
        let profile = VerificationProfile {
            name: "check-artifacts".to_string(),
            argv: shell_argv("echo ok"),
            relative_dir: None,
            timeout_seconds: None,
            max_output_bytes: None,
            expected_exit_code: None,
            expected_artifacts: vec!["nonexistent.txt".to_string()],
            env: vec![],
        };
        let result = execute_profile(&profile, dir.path()).unwrap();
        assert!(!result.artifacts[0].exists);
    }

    #[test]
    fn expected_artifacts_present() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("output.txt"), "data").unwrap();
        let profile = VerificationProfile {
            name: "check-artifacts".to_string(),
            argv: shell_argv("echo ok"),
            relative_dir: None,
            timeout_seconds: None,
            max_output_bytes: None,
            expected_exit_code: None,
            expected_artifacts: vec!["output.txt".to_string()],
            env: vec![],
        };
        let result = execute_profile(&profile, dir.path()).unwrap();
        assert!(result.artifacts[0].exists);
        assert_eq!(result.artifacts[0].size_bytes, Some(4));
    }

    #[test]
    fn profile_env_variables() {
        let profile = VerificationProfile {
            name: "env-test".to_string(),
            argv: shell_argv(ECHO_ENV_COMMAND),
            relative_dir: None,
            timeout_seconds: None,
            max_output_bytes: None,
            expected_exit_code: None,
            expected_artifacts: vec![],
            env: vec!["MY_VAR=hello_test".to_string()],
        };
        let dir = tempfile::tempdir().unwrap();
        let result = execute_profile(&profile, dir.path()).unwrap();
        assert!(
            result.stdout.contains("hello_test"),
            "stdout: {}",
            result.stdout
        );
    }

    #[test]
    fn result_serializes_to_json() {
        let result = ProfileResult {
            profile_name: "test".to_string(),
            exit_code: 0,
            stdout: "ok".to_string(),
            stderr: String::new(),
            artifacts: vec![ArtifactVerification {
                path: "out.txt".to_string(),
                exists: true,
                size_bytes: Some(4),
                content_hash: None,
            }],
            duration_ms: 42,
            timestamp: "2026-07-27T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string_pretty(&result).unwrap();
        assert!(json.contains("profileName"));
        assert!(json.contains("exitCode"));
        assert!(json.contains("artifacts"));
    }
}
