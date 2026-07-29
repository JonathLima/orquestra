use assert_cmd::Command;
use chrono::Utc;
use predicates::prelude::*;
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
            "orquestra-cli-model-test-{}-{}-{}",
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
            &fixture_path("model-routing-plan.json"),
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
    value["id"].as_str().expect("session id").to_string()
}

#[test]
fn test_model_catalog_lists_codex_tiers() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args(["--output", "json", "model", "catalog", "--host", "codex"])
        .output()
        .expect("run model catalog");

    assert!(
        output.status.success(),
        "catalog failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse catalog JSON");
    assert_eq!(value["host"], "codex");
    let models = value["models"].as_array().expect("models array");
    assert!(models.iter().any(|model| model["tier"] == "fast"));
    assert!(models.iter().any(|model| model["tier"] == "frontier"));
}

#[test]
fn test_model_recommendation_uses_fast_for_low_risk_ticket() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "--output",
            "json",
            "model",
            "recommend",
            "--ticket",
            &fixture_path("model-routing-plan.json"),
            "--host",
            "codex",
            "--ticket-id",
            "DOCS",
        ])
        .output()
        .expect("run model recommend");

    assert!(
        output.status.success(),
        "recommend failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse recommendation JSON");
    assert_eq!(value["recommendation"]["ticketId"], "DOCS");
    assert_eq!(value["recommendation"]["tier"], "fast");
    assert_eq!(value["recommendation"]["reasoningEffort"], "low");
}

#[test]
fn test_model_recommendation_uses_frontier_for_security_release_ticket() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "--output",
            "json",
            "model",
            "recommend",
            "--ticket",
            &fixture_path("model-routing-plan.json"),
            "--host",
            "codex",
            "--ticket-id",
            "SECURITY",
        ])
        .output()
        .expect("run model recommend");

    assert!(
        output.status.success(),
        "recommend failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse recommendation JSON");
    assert_eq!(value["recommendation"]["ticketId"], "SECURITY");
    assert_eq!(value["recommendation"]["tier"], "frontier");
    assert_eq!(value["recommendation"]["reasoningEffort"], "high");
    assert_eq!(value["recommendation"]["webRequired"], true);
}

#[test]
fn test_dispatch_manifest_contains_model_recommendation() {
    let dir = TestDir::new();
    let session_id = create_session(&dir);

    command_at(&dir)
        .args(["run", "dispatch", &session_id, "--host", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dispatched wave 1"));

    let manifest_path = dir
        .path()
        .join(".orquestra")
        .join("sessions")
        .join(&session_id)
        .join("tickets")
        .join("DOCS.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(manifest_path).expect("read manifest"))
            .expect("parse manifest");
    assert_eq!(manifest["modelRecommendation"]["host"], "codex");
    assert_eq!(manifest["modelRecommendation"]["tier"], "fast");
    assert_eq!(
        manifest["modelRecommendation"]["resolvedAt"],
        Utc::now().format("%Y-%m-%d").to_string()
    );
}

#[test]
fn test_model_explain_reads_persisted_dispatch_recommendation() {
    let dir = TestDir::new();
    let session_id = create_session(&dir);

    command_at(&dir)
        .args(["run", "dispatch", &session_id, "--host", "codex"])
        .assert()
        .success();

    let output = command_at(&dir)
        .args([
            "--output",
            "json",
            "model",
            "explain",
            "--session",
            &session_id,
            "--ticket-id",
            "DOCS",
        ])
        .output()
        .expect("run model explain");

    assert!(
        output.status.success(),
        "explain failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse explanation JSON");
    assert_eq!(value["recommendation"]["ticketId"], "DOCS");
    assert_eq!(value["recommendation"]["host"], "codex");
    assert_eq!(value["recommendation"]["tier"], "fast");
}
