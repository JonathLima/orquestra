use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;

fn fixture_path(name: &str) -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    p.to_string_lossy().to_string()
}

#[test]
fn test_plan_validate_valid() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["plan", "validate", &fixture_path("valid-plan.json")]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("valid"));
}

#[test]
fn test_plan_validate_invalid() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["plan", "validate", &fixture_path("invalid-plan.json")]);
    cmd.assert().failure();
}

#[test]
fn test_plan_waves_shows_waves() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["plan", "waves", &fixture_path("valid-plan.json")]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Wave"));
}

#[test]
fn test_plan_explain_shows_title() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["plan", "explain", &fixture_path("valid-plan.json")]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Build Authentication"));
}

#[test]
fn test_plan_export_json() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args([
        "plan",
        "export",
        &fixture_path("valid-plan.json"),
        "--format",
        "json",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("schemaVersion"));
}

#[test]
fn test_plan_export_md() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args([
        "plan",
        "export",
        &fixture_path("valid-plan.json"),
        "--format",
        "md",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("# Plan: Build Authentication"));
}

#[test]
fn test_plan_validate_cycle() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["plan", "validate", &fixture_path("cycle-plan.json")]);
    cmd.assert()
        .failure()
        .stdout(predicate::str::contains("cycle_detected"));
}
