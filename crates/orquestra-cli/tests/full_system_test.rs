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
            "orquestra-full-system-test-{}-{}-{}",
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

fn command_at(dir: &TestDir) -> Command {
    let mut command = Command::cargo_bin("orquestra-cli").expect("find CLI binary");
    command
        .env("HOME", dir.path())
        .env("USERPROFILE", dir.path())
        .current_dir(dir.path());
    command
}

fn write_skill(dir: &TestDir) {
    let skill_dir = dir
        .path()
        .join(".agents")
        .join("skills")
        .join("local-harness");
    std::fs::create_dir_all(&skill_dir).expect("create skill dir");
    std::fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: local-harness
description: Build and test local harness workflows
capabilities:
  - backend
  - testing
---
# Local Harness
"#,
    )
    .expect("write skill");
}

fn write_plan(dir: &TestDir) -> PathBuf {
    let plan = dir.path().join("plan.json");
    std::fs::write(
        &plan,
        r#"{
  "schemaVersion": 1,
  "title": "Full System",
  "tickets": [
    {
      "id": "T1",
      "title": "Prepare backend state",
      "objective": "Prepare backend state locally.",
      "acceptanceCriteria": ["state exists"],
      "blockedBy": [],
      "preferredCapabilities": ["backend"],
      "assignedSkill": "local-harness",
      "verification": { "minimumScore": 0.95, "requiredEvidence": ["diff"] }
    },
    {
      "id": "T2",
      "title": "Test workflow",
      "objective": "Test workflow locally.",
      "acceptanceCriteria": ["tests pass"],
      "blockedBy": ["T1"],
      "preferredCapabilities": ["testing"],
      "assignedSkill": "local-harness",
      "verification": { "minimumScore": 0.95, "requiredEvidence": ["test"] }
    }
  ]
}"#,
    )
    .expect("write plan");
    plan
}

fn create_report(
    dir: &TestDir,
    session_id: &str,
    ticket_id: &str,
    skill_name: &str,
    evidence_kind: &str,
) -> PathBuf {
    let attempt_id = dispatch_attempt_id(dir, session_id, ticket_id);
    let report = dir.path().join(format!("{ticket_id}-report.json"));
    std::fs::write(
        &report,
        format!(
            r#"{{
  "sessionId": "{session_id}",
  "ticketId": "{ticket_id}",
  "dispatchAttemptId": "{attempt_id}",
  "skillName": "{skill_name}",
  "score": 0.96,
  "summary": "Accepted",
  "evidence": [
    {{ "kind": "{evidence_kind}", "description": "local evidence", "path": null }}
  ]
}}"#
        ),
    )
    .expect("write report");
    report
}

fn dispatch_attempt_id(dir: &TestDir, session_id: &str, ticket_id: &str) -> String {
    let output = command_at(dir)
        .args(["--output", "json", "run", "status", session_id])
        .output()
        .expect("run status");
    assert!(
        output.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse status output");
    value["session"]["ticket_states"][ticket_id]["dispatch_attempt_id"]
        .as_str()
        .expect("dispatch attempt id")
        .to_string()
}

fn create_session(dir: &TestDir, plan: &Path) -> String {
    let output = command_at(dir)
        .args(["--output", "json", "run", "create", &plan.to_string_lossy()])
        .output()
        .expect("create session");
    assert!(
        output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse session output");
    value["id"].as_str().expect("session id").to_string()
}

#[test]
fn test_full_local_harness_end_to_end() {
    let dir = TestDir::new();
    write_skill(&dir);
    let plan = write_plan(&dir);

    command_at(&dir).args(["skill", "scan"]).assert().success();
    command_at(&dir)
        .args(["skill", "match", "--ticket", &plan.to_string_lossy()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Selected: local-harness"));

    let session_id = create_session(&dir, &plan);
    command_at(&dir)
        .args(["run", "dispatch", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 1"));

    let t1_report = create_report(&dir, &session_id, "T1", "local-harness", "diff");
    command_at(&dir)
        .args([
            "verify",
            "ticket",
            "--report",
            &t1_report.to_string_lossy(),
            "--plan",
            &plan.to_string_lossy(),
        ])
        .assert()
        .success();
    command_at(&dir)
        .args([
            "run",
            "complete-ticket",
            &session_id,
            "T1",
            "--output",
            "token=secret-value completed",
            "--evidence",
            "diff",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Checkpoint"));
    command_at(&dir)
        .args(["run", "approve-wave", &session_id, "--wave", "1"])
        .assert()
        .success();

    command_at(&dir)
        .args(["run", "dispatch", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 2"));
    let t2_report = create_report(&dir, &session_id, "T2", "local-harness", "test");
    command_at(&dir)
        .args([
            "verify",
            "ticket",
            "--report",
            &t2_report.to_string_lossy(),
            "--plan",
            &plan.to_string_lossy(),
        ])
        .assert()
        .success();
    command_at(&dir)
        .args([
            "run",
            "complete-ticket",
            &session_id,
            "T2",
            "--output",
            "tests passed",
            "--evidence",
            "test",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Completed"));

    let session_json = command_at(&dir)
        .args(["--output", "json", "run", "status", &session_id])
        .output()
        .expect("status");
    let value: serde_json::Value =
        serde_json::from_slice(&session_json.stdout).expect("parse status");
    assert_eq!(
        value["session"]["ticket_states"]["T1"]["output"],
        "token=[REDACTED] completed"
    );
}

#[test]
fn test_concurrent_wave_completion_without_loss() {
    use std::thread;
    let dir = TestDir::new();
    write_skill(&dir);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("concurrent-wave-plan.json");
    let plan_path = dir.path().join("concurrent-plan.json");
    std::fs::copy(&fixture, &plan_path).expect("copy fixture");

    command_at(&dir).args(["skill", "scan"]).assert().success();
    let session_id = create_session(&dir, &plan_path);
    command_at(&dir)
        .args(["run", "start", &session_id])
        .assert()
        .success();
    command_at(&dir)
        .args(["run", "dispatch", &session_id])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 1"));

    let ticket_ids = ["P1", "P2", "P3", "P4"];
    let mut handles = Vec::new();
    let dir_path = dir.path().to_path_buf();
    let plan_path_str = plan_path.to_string_lossy().to_string();
    for ticket_id in ticket_ids {
        let dir_clone = dir_path.clone();
        let session_clone = session_id.clone();
        let plan_str = plan_path_str.clone();
        let handle = thread::spawn(move || {
            let report_path = dir_clone.join(format!("{ticket_id}-report.json"));
            let attempt_id_output = Command::cargo_bin("orquestra-cli")
                .expect("find CLI binary")
                .current_dir(&dir_clone)
                .args(["--output", "json", "run", "status", &session_clone])
                .output()
                .expect("status");
            assert!(
                attempt_id_output.status.success(),
                "status failed: {}",
                String::from_utf8_lossy(&attempt_id_output.stderr)
            );
            let status_value: serde_json::Value =
                serde_json::from_slice(&attempt_id_output.stdout).expect("parse status");
            let attempt_id =
                status_value["session"]["ticket_states"][ticket_id]["dispatch_attempt_id"]
                    .as_str()
                    .expect("attempt id")
                    .to_string();
            std::fs::write(
                &report_path,
                format!(
                    r#"{{
  "sessionId": "{session_clone}",
  "ticketId": "{ticket_id}",
  "dispatchAttemptId": "{attempt_id}",
  "skillName": "local-harness",
  "score": 0.96,
  "summary": "Accepted",
  "evidence": [
    {{ "kind": "diff", "description": "local evidence", "path": null }}
  ]
}}"#
                ),
            )
            .expect("write report");

            let verify = Command::cargo_bin("orquestra-cli")
                .expect("find CLI binary")
                .current_dir(&dir_clone)
                .args([
                    "verify",
                    "ticket",
                    "--report",
                    &report_path.to_string_lossy(),
                    "--plan",
                    &plan_str,
                ])
                .output()
                .expect("verify");
            assert!(
                verify.status.success(),
                "verify failed: {}",
                String::from_utf8_lossy(&verify.stderr)
            );

            Command::cargo_bin("orquestra-cli")
                .expect("find CLI binary")
                .current_dir(&dir_clone)
                .args([
                    "run",
                    "complete-ticket",
                    &session_clone,
                    ticket_id,
                    "--output",
                    &format!("completed {ticket_id}"),
                    "--evidence",
                    "diff",
                ])
                .output()
                .expect("complete")
        });
        handles.push(handle);
    }

    let mut all_succeeded = true;
    for handle in handles {
        let output = handle.join().expect("thread join");
        if !output.status.success() {
            eprintln!(
                "concurrent step failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            all_succeeded = false;
        }
    }
    assert!(all_succeeded, "one or more concurrent steps failed");

    let status = command_at(&dir)
        .args(["--output", "json", "run", "status", &session_id])
        .output()
        .expect("status");
    let value: serde_json::Value = serde_json::from_slice(&status.stdout).expect("parse status");
    for ticket_id in ticket_ids {
        let state = &value["session"]["ticket_states"][ticket_id];
        assert_eq!(
            state["status"], "Completed",
            "ticket {ticket_id} expected Completed, got {state:?}"
        );
    }

    let events_log = std::fs::read_to_string(
        dir.path()
            .join(".orquestra")
            .join("events")
            .join(format!("{session_id}.jsonl")),
    )
    .expect("read events");
    let line_count = events_log.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        line_count >= 6,
        "expected ≥6 event lines (1 created + 1 dispatched + 4 completed), got {line_count}"
    );
    let completion_count = events_log
        .lines()
        .filter(|line| line.contains("\"ticket_completed\""))
        .count();
    assert_eq!(
        completion_count, 4,
        "expected 4 ticket_completed events without loss, got {completion_count}"
    );
}
