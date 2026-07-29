use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fixture_path(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    p.to_string_lossy().to_string()
}

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let unique = format!(
            "orquestra-cli-run-test-{}-{}-{}",
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
    command.current_dir(dir.path());
    command
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
        .expect("run session create");
    assert!(
        output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse create output");
    value["id"]
        .as_str()
        .expect("create output contains ID")
        .to_string()
}

fn event_log_path(dir: &TestDir, session_id: &str) -> PathBuf {
    dir.path()
        .join(".orquestra")
        .join("events")
        .join(format!("{session_id}.jsonl"))
}

fn write_verification_report(
    dir: &TestDir,
    session_id: &str,
    ticket_id: &str,
    skill_name: &str,
    evidence_kind: &str,
) -> PathBuf {
    let status_output = command_at(dir)
        .args(["--output", "json", "run", "status", session_id])
        .output()
        .expect("run status for attempt id");
    assert!(
        status_output.status.success(),
        "status failed: {}",
        String::from_utf8_lossy(&status_output.stderr)
    );
    let status: serde_json::Value =
        serde_json::from_slice(&status_output.stdout).expect("parse status output");
    let attempt_id = status["session"]["ticket_states"][ticket_id]["dispatch_attempt_id"]
        .as_str()
        .expect("dispatch attempt id");
    let report = dir.path().join(format!("{ticket_id}-verification.json"));
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
    {{ "kind": "{evidence_kind}", "description": "evidence accepted", "path": null }}
  ]
}}"#
        ),
    )
    .expect("write verification report");
    report
}

fn verify_ticket(dir: &TestDir, report: &Path) {
    command_at(dir)
        .args([
            "verify",
            "ticket",
            "--report",
            &report.to_string_lossy(),
            "--plan",
            &fixture_path("valid-plan.json"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Passed: true"));
}

#[test]
fn test_run_create_success() {
    let dir = TestDir::new();
    let mut cmd = command_at(&dir);
    cmd.args(["run", "create", &fixture_path("valid-plan.json")]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Session created"));
}

#[test]
fn test_run_create_invalid_plan_fails() {
    let dir = TestDir::new();
    let mut cmd = command_at(&dir);
    cmd.args(["run", "create", &fixture_path("invalid-plan.json")]);
    cmd.assert().failure();
}

#[test]
fn test_run_status_not_found() {
    let dir = TestDir::new();
    let mut cmd = command_at(&dir);
    cmd.args(["run", "status", "00000000-0000-4000-8000-000000000000"]);
    cmd.assert().failure();
}

#[test]
fn test_session_list_after_create() {
    let dir = TestDir::new();
    let mut cmd = command_at(&dir);
    cmd.args(["run", "create", &fixture_path("valid-plan.json")]);
    cmd.assert().success();

    let mut cmd = command_at(&dir);
    cmd.args(["session", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Sessions"))
        .stdout(predicate::str::contains("Created"))
        .stdout(predicate::str::contains("\"Created\"").not());
}

#[test]
fn test_session_list_fails_for_corrupt_session() {
    let dir = TestDir::new();
    let session_id = "00000000-0000-4000-8000-000000000001";
    let session_dir = dir
        .path()
        .join(".orquestra")
        .join("sessions")
        .join(session_id);
    std::fs::create_dir_all(&session_dir).expect("create corrupt session directory");
    std::fs::write(session_dir.join("session.json"), "not-json")
        .expect("write corrupt session state");

    let mut cmd = command_at(&dir);
    cmd.args(["session", "list"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Cannot load session"));
}

#[test]
fn test_session_show_fails_for_corrupt_events() {
    let dir = TestDir::new();
    let session_id = create_session(&dir);
    std::fs::write(event_log_path(&dir, &session_id), "not-json\n")
        .expect("write corrupt event log");

    let mut cmd = command_at(&dir);
    cmd.args(["session", "show", &session_id]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Cannot read events"));
}

#[test]
fn test_short_event_timestamp_renders_without_panicking() {
    let dir = TestDir::new();
    let session_id = create_session(&dir);
    let event = serde_json::json!({
        "ts": "short",
        "session_id": session_id,
        "event": "short_timestamp",
        "data": {}
    });
    std::fs::write(event_log_path(&dir, &session_id), format!("{event}\n"))
        .expect("write short timestamp event");

    let mut events_cmd = command_at(&dir);
    events_cmd.args(["session", "events", &session_id]);
    events_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("short  short_timestamp"));

    let mut show_cmd = command_at(&dir);
    show_cmd.args(["session", "show", &session_id]);
    show_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("short  short_timestamp"));
}

#[test]
fn test_session_lifecycle_round_trip() {
    let dir = TestDir::new();
    let session_id = create_session(&dir);

    let mut start_cmd = command_at(&dir);
    start_cmd.args(["run", "start", &session_id]);
    start_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Running"));

    let status_output = command_at(&dir)
        .args(["--output", "json", "run", "status", &session_id])
        .output()
        .expect("run status");
    assert!(status_output.status.success());
    let status: serde_json::Value =
        serde_json::from_slice(&status_output.stdout).expect("parse status output");
    assert_eq!(status["session"]["status"], "Running");

    let mut dispatch_cmd = command_at(&dir);
    dispatch_cmd.args(["run", "dispatch", &session_id]);
    dispatch_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 1"));

    let report = write_verification_report(&dir, &session_id, "T1", "schema-skill", "diff");
    verify_ticket(&dir, &report);

    let mut complete_cmd = command_at(&dir);
    complete_cmd.args([
        "run",
        "complete-ticket",
        &session_id,
        "T1",
        "--output",
        "schema done",
        "--evidence",
        "diff",
    ]);
    complete_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Checkpoint"));

    let mut resume_cmd = command_at(&dir);
    resume_cmd.args(["session", "resume", &session_id]);
    resume_cmd
        .assert()
        .failure()
        .stderr(predicate::str::contains("checkpoint approval"));

    let mut approve_cmd = command_at(&dir);
    approve_cmd.args(["run", "approve-wave", &session_id, "--wave", "1"]);
    approve_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Running"));

    let mut events_cmd = command_at(&dir);
    events_cmd.args(["session", "events", &session_id]);
    events_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("session_created"))
        .stdout(predicate::str::contains("wave_dispatched"))
        .stdout(predicate::str::contains("ticket_completed"));

    let mut cancel_cmd = command_at(&dir);
    cancel_cmd.args(["run", "cancel", &session_id]);
    cancel_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Cancelled"));

    let export_output = command_at(&dir)
        .args(["run", "export", &session_id, "--format", "json"])
        .output()
        .expect("run export");
    assert!(export_output.status.success());
    let exported: serde_json::Value =
        serde_json::from_slice(&export_output.stdout).expect("parse export");
    assert_eq!(exported["id"], session_id);
    assert_eq!(exported["status"], "Cancelled");
}

#[test]
fn test_local_dispatch_checkpoint_approval_flow() {
    let dir = TestDir::new();
    let session_id = create_session(&dir);

    let mut dispatch_cmd = command_at(&dir);
    dispatch_cmd.args(["run", "dispatch", &session_id]);
    dispatch_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 1"));

    assert!(
        dir.path()
            .join(".orquestra")
            .join("sessions")
            .join(&session_id)
            .join("tickets")
            .join("T1.json")
            .exists()
    );

    let report = write_verification_report(&dir, &session_id, "T1", "schema-skill", "diff");
    verify_ticket(&dir, &report);

    let mut complete_cmd = command_at(&dir);
    complete_cmd.args([
        "run",
        "complete-ticket",
        &session_id,
        "T1",
        "--output",
        "schema done",
        "--evidence",
        "diff",
    ]);
    complete_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Status: Checkpoint"));

    let mut approve_cmd = command_at(&dir);
    approve_cmd.args(["run", "approve-wave", &session_id, "--wave", "1"]);
    approve_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Wave: 2/3"));

    let mut next_dispatch_cmd = command_at(&dir);
    next_dispatch_cmd.args(["run", "dispatch", &session_id]);
    next_dispatch_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 2"));
}
