use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new(prefix: &str) -> Self {
        let unique = format!(
            "{}-{}-{}-{}",
            prefix,
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

fn fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn command_at(dir: &TestDir) -> Command {
    let mut command = Command::cargo_bin("orquestra-cli").expect("find CLI binary");
    command
        .env("HOME", dir.path())
        .env("USERPROFILE", dir.path())
        .current_dir(dir.path());
    command
}

fn create_skill(dir: &TestDir) {
    let skill_dir = dir.path().join(".agents").join("skills").join("test-skill");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: test-skill
description: A local skill for BRAIN tests
capabilities:
  - backend
---
# Test Skill
"#,
    )
    .expect("write skill");
}

fn create_ticket_file(dir: &TestDir) -> PathBuf {
    let ticket_file = dir.path().join("ticket.json");
    std::fs::write(
        &ticket_file,
        r#"{
  "id": "T1",
  "title": "Implement backend route",
  "objective": "Build a backend endpoint",
  "acceptanceCriteria": ["endpoint works"],
  "blockedBy": [],
  "preferredCapabilities": ["backend"],
  "assignedSkill": null
}"#,
    )
    .expect("write ticket");
    ticket_file
}

fn create_session(dir: &TestDir) -> String {
    let output = command_at(dir)
        .args([
            "--output",
            "json",
            "run",
            "create",
            &fixture_path("valid-plan.json"),
        ])
        .output()
        .expect("create session");
    assert!(
        output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse create output");
    value["id"].as_str().expect("session id").to_string()
}

#[test]
fn test_brain_policy_disables_external_search() {
    let dir = TestDir::new("orquestra-brain-policy-test");

    command_at(&dir)
        .args(["brain", "policy"])
        .assert()
        .success()
        .stdout(predicate::str::contains("externalDiscoveryEnabled: false"));

    command_at(&dir)
        .args(["brain", "search", "backend skill"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "External BRAIN discovery is disabled",
        ));
}

#[test]
fn test_brain_adapt_and_approve_stays_project_local() {
    let dir = TestDir::new("orquestra-brain-adapt-test");
    create_skill(&dir);
    let ticket_file = create_ticket_file(&dir);

    command_at(&dir).args(["skill", "scan"]).assert().success();
    let output = command_at(&dir)
        .args([
            "--output",
            "json",
            "brain",
            "adapt",
            "--ticket",
            &ticket_file.to_string_lossy(),
            "--from-skill",
            "test-skill",
        ])
        .output()
        .expect("brain adapt");
    assert!(
        output.status.success(),
        "adapt failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse adapt output");
    let candidate_id = value["candidate"]["id"]
        .as_str()
        .expect("candidate id")
        .to_string();
    let pending_path = dir
        .path()
        .join(".orquestra")
        .join("skills")
        .join("_pending")
        .join(&candidate_id);
    assert!(pending_path.join("SKILL.md").exists());

    command_at(&dir)
        .args(["brain", "approve", &candidate_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Approved"));

    assert!(
        dir.path()
            .join(".orquestra")
            .join("skills")
            .join("brain-t1-test-skill")
            .join("SKILL.md")
            .exists()
    );
}

#[test]
fn test_verify_ticket_persists_and_session_run_evaluates_report() {
    let dir = TestDir::new("orquestra-verify-test");
    let session_id = create_session(&dir);
    let report_file = dir.path().join("verification-report.json");
    std::fs::write(
        &report_file,
        format!(
            r#"{{
  "sessionId": "{session_id}",
  "ticketId": "T2",
  "skillName": "backend-skill",
  "score": 0.96,
  "summary": "Accepted",
  "evidence": [
    {{ "kind": "test", "description": "cargo test passed", "path": null }}
  ]
}}"#
        ),
    )
    .expect("write report");

    command_at(&dir)
        .args([
            "verify",
            "ticket",
            "--report",
            &report_file.to_string_lossy(),
            "--plan",
            &fixture_path("valid-plan.json"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Passed: true"));

    command_at(&dir)
        .args(["verify", "run", &session_id, "T2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Passed: true"));

    command_at(&dir)
        .args(["verify", "report", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("T2.json"));
}
