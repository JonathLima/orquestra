use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fixture_path(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
        .to_string_lossy()
        .to_string()
}

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let unique = format!(
            "orquestra-cli-research-test-{}-{}-{}",
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

    fn write_json(&self, name: &str, value: serde_json::Value) -> String {
        let path = self.path().join(name);
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&value).expect("serialize JSON"),
        )
        .expect("write JSON file");
        path.to_string_lossy().to_string()
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

fn create_started_session(dir: &TestDir) -> String {
    let output = command_at(dir)
        .args([
            "--output",
            "json",
            "run",
            "create",
            &fixture_path("research-plan.json"),
        ])
        .output()
        .expect("run create");
    assert!(
        output.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse create output");
    let session_id = value["id"].as_str().expect("session id").to_string();
    command_at(dir)
        .args(["run", "start", &session_id])
        .assert()
        .success();
    session_id
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

fn today_str() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn passing_research_report(session_id: Option<&str>) -> serde_json::Value {
    let today = today_str();
    let source = |url: &str, title: &str, publisher: &str| -> serde_json::Value {
        json!({
            "url": url,
            "title": title,
            "publisher": publisher,
            "sourceType": "primary",
            "trustLevel": "high",
            "retrievedAt": format!("{}T12:00:00Z", today),
            "claim": "Claim about ",
            "supportsClaim": true,
            "contentHash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        })
    };
    json!({
        "sessionId": session_id.unwrap_or("00000000-0000-4000-8000-000000000000"),
        "ticketId": "SECURITY",
        "currentDate": today,
        "generatedAt": format!("{}T12:00:00Z", today),
        "claims": [
            {
                "id": "claim-model-docs",
                "statement": "Codex model guidance must be checked against current official model documentation before release.",
                "requiredPrimary": true,
                "sources": [
                    source("https://platform.openai.com/docs/models", "OpenAI Models", "OpenAI"),
                    source("https://developers.openai.com/codex/cli", "Codex CLI", "OpenAI")
                ],
                "conflicts": [],
                "finalAssessment": "Supported by current official documentation.",
                "confidence": 0.96,
                "usedForDecision": true
            }
        ]
    })
}

#[test]
fn test_research_brief_marks_wie_cross_check_requirements() {
    let dir = TestDir::new();

    let output = command_at(&dir)
        .args([
            "--output",
            "json",
            "research",
            "brief",
            "--ticket",
            &fixture_path("research-plan.json"),
            "--ticket-id",
            "SECURITY",
            "--host",
            "antigravity",
        ])
        .output()
        .expect("run research brief");

    assert!(
        output.status.success(),
        "brief failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("parse brief");
    assert_eq!(value["ticketId"], "SECURITY");
    assert_eq!(value["currentDate"], today_str());
    assert_eq!(value["host"], "antigravity");
    assert_eq!(value["toolHints"]["webSearch"], "WIE web_search_advanced");
    assert_eq!(value["minimumSources"], 2);
    assert_eq!(value["primarySourceRequired"], true);
}

#[test]
fn test_research_validate_requires_primary_plus_cross_check() {
    let dir = TestDir::new();
    let report_path = dir.write_json("research-report.json", passing_research_report(None));

    let output = command_at(&dir)
        .args([
            "--output",
            "json",
            "research",
            "validate",
            "--report",
            &report_path,
        ])
        .output()
        .expect("run research validate");

    assert!(
        output.status.success(),
        "validate failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse validation");
    assert_eq!(value["valid"], true);
    assert_eq!(value["claimCount"], 1);
    assert_eq!(value["validatedClaims"], 1);
}

#[test]
fn test_research_validate_rejects_single_source_claim() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["claims"][0]["sources"]
        .as_array_mut()
        .expect("sources")
        .truncate(1);
    let report_path = dir.write_json("single-source.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("at least 2 supporting sources"));
}

#[test]
fn test_research_validate_rejects_stale_generated_at_date() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["generatedAt"] = json!("2026-07-26T23:59:00Z");
    let report_path = dir.write_json("stale-generated-at.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "generatedAt must start with currentDate",
        ));
}

#[test]
fn test_research_validate_rejects_private_ip_origin() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["claims"][0]["sources"][0]["url"] = json!("https://192.168.1.1/internal");
    let report_path = dir.write_json("private-ip.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("incomplete source"));
}

#[test]
fn test_research_validate_rejects_localhost_origin() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["claims"][0]["sources"][0]["url"] = json!("https://localhost/internal");
    let report_path = dir.write_json("localhost.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("incomplete source"));
}

#[test]
fn test_research_validate_rejects_loopback_ipv4() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["claims"][0]["sources"][0]["url"] = json!("https://127.0.0.1/loopback");
    let report_path = dir.write_json("loopback.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("incomplete source"));
}

#[test]
fn test_research_validate_rejects_duplicate_source_urls() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    let shared = "https://platform.openai.com/docs/models";
    report["claims"][0]["sources"][0]["url"] = json!(shared);
    report["claims"][0]["sources"][1]["url"] = json!(shared);
    let report_path = dir.write_json("duplicate-urls.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate supporting sources"));
}

#[test]
fn test_research_validate_rejects_unapproved_source_type() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["claims"][0]["sources"][0]["sourceType"] = json!("blog");
    let report_path = dir.write_json("bad-source-type.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("incomplete source"));
}

#[test]
fn test_research_validate_rejects_unapproved_trust_level() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["claims"][0]["sources"][0]["trustLevel"] = json!("speculative");
    let report_path = dir.write_json("bad-trust.json", report);

    command_at(&dir)
        .args(["research", "validate", "--report", &report_path])
        .assert()
        .failure()
        .stderr(predicate::str::contains("incomplete source"));
}

#[test]
fn test_research_store_rejects_report_without_session_id() {
    let dir = TestDir::new();
    let mut report = passing_research_report(None);
    report["sessionId"] = serde_json::Value::Null;
    let report_path = dir.write_json("no-session.json", report);

    command_at(&dir)
        .args(["research", "store", "--report", &report_path])
        .assert()
        .failure();
}

#[test]
fn test_research_store_writes_project_memory_and_research_index() {
    let dir = TestDir::new();
    let report_path = dir.write_json("research-report.json", passing_research_report(None));

    command_at(&dir)
        .args(["research", "store", "--report", &report_path])
        .assert()
        .success()
        .stdout(predicate::str::contains("Stored research report"));

    assert!(
        dir.path()
            .join(".orquestra")
            .join("research")
            .join("00000000-0000-4000-8000-000000000000")
            .join("SECURITY.json")
            .is_file()
    );
    assert!(dir.path().join(".orquestra/memory/facts.jsonl").is_file());
    assert!(
        std::fs::read_to_string(dir.path().join(".orquestra/memory/research-index.jsonl"))
            .expect("read research index")
            .contains("claim-model-docs")
    );
}

#[test]
fn test_web_required_ticket_cannot_complete_without_validated_research() {
    let dir = TestDir::new();
    let session_id = create_started_session(&dir);

    command_at(&dir)
        .args(["run", "dispatch", &session_id, "--host", "codex"])
        .assert()
        .success();
    command_at(&dir)
        .args([
            "run",
            "complete-ticket",
            &session_id,
            "SECURITY",
            "--output",
            "done",
            "--evidence",
            "test",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("validated research report"));
}

#[test]
fn test_web_required_ticket_completes_with_validated_research_and_verification() {
    let dir = TestDir::new();
    let session_id = create_started_session(&dir);

    command_at(&dir)
        .args(["run", "dispatch", &session_id, "--host", "codex"])
        .assert()
        .success();
    let attempt_id = dispatch_attempt_id(&dir, &session_id, "SECURITY");

    let report_path = dir.write_json(
        "research-report.json",
        passing_research_report(Some(&session_id)),
    );
    command_at(&dir)
        .args(["research", "store", "--report", &report_path])
        .assert()
        .success();

    let verification_path = dir.write_json(
        "verification.json",
        json!({
            "sessionId": session_id,
            "ticketId": "SECURITY",
            "dispatchAttemptId": attempt_id,
            "skillName": "security-review",
            "score": 0.99,
            "summary": "Research and tests passed.",
            "evidence": [
                { "kind": "test", "description": "cargo test passed", "path": null },
                { "kind": "research", "description": "validated cross-source report", "path": format!(".orquestra/research/{}/SECURITY.json", session_id) }
            ]
        }),
    );
    command_at(&dir)
        .args([
            "verify",
            "ticket",
            "--report",
            &verification_path,
            "--plan",
            &fixture_path("research-plan.json"),
        ])
        .assert()
        .success();

    command_at(&dir)
        .args([
            "run",
            "complete-ticket",
            &session_id,
            "SECURITY",
            "--output",
            "done",
            "--evidence",
            "test",
            "--evidence",
            "research",
        ])
        .assert()
        .success();
}
