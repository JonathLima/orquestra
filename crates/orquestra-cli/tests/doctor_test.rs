use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let unique = format!(
            "orquestra-doctor-test-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before the Unix epoch")
                .as_nanos(),
            TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed),
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir(&path).expect("create isolated test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn test_doctor_help() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("orquestra"));
}

#[test]
fn test_doctor_output() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.arg("doctor");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Orquestra v"));
}

#[test]
fn test_doctor_json_output() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["--output", "json", "doctor"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"version\""));
}

#[test]
fn test_version_flag() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("0.1.0"));
}

#[test]
fn test_doctor_jsonl_output() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["--output", "jsonl", "doctor"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"version\""));
}

#[test]
fn test_doctor_security_output() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["doctor", "--security"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("## Security"))
        .stdout(predicate::str::contains("external BRAIN: disabled"));
}

#[test]
fn test_invalid_output_format_exits_with_error() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["--output", "invalid", "doctor"]);
    cmd.assert().failure();
}

#[test]
fn test_doctor_counts_only_local_skill_directories() {
    let dir = TestDir::new();
    let approved = dir
        .path()
        .join(".orquestra")
        .join("skills")
        .join("approved");
    let pending = dir
        .path()
        .join(".orquestra")
        .join("skills")
        .join("_pending")
        .join("candidate");
    std::fs::create_dir_all(&approved).expect("create approved skill dir");
    std::fs::create_dir_all(&pending).expect("create pending skill dir");
    std::fs::write(approved.join("SKILL.md"), "# Approved").expect("write approved skill");
    std::fs::write(pending.join("SKILL.md"), "# Pending").expect("write pending skill");

    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.current_dir(dir.path())
        .args(["--output", "json", "doctor"]);
    let output = cmd.output().expect("run doctor");
    assert!(
        output.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse doctor output");

    assert_eq!(value["skills_count"]["local_skills"], 1);
    assert_eq!(value["skills_count"]["pending_candidates"], 1);
}
