use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

fn setup() -> PathBuf {
    let dir = std::env::temp_dir().join("orquestra-skills-cli-test");
    INIT.call_once(|| {
        let skill_dir = dir.join(".agents").join("skills").join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let content = r#"---
name: test-skill
description: A test skill for CLI integration
version: 1.0.0
capabilities:
  - backend
  - testing
---
# Test Skill
"#;
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    });
    dir
}

fn orquestra() -> Command {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    let dir = setup();
    cmd.env("HOME", &dir)
        .env("USERPROFILE", &dir)
        .current_dir(&dir);
    cmd
}

#[test]
fn test_skills_scan_success() {
    let mut cmd = orquestra();
    cmd.arg("skill").arg("scan");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Scanned"));
}

#[test]
fn test_skills_list_after_scan() {
    let mut cmd = orquestra();
    cmd.args(["skill", "scan"]);
    cmd.assert().success();

    let mut cmd = orquestra();
    cmd.arg("skill").arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("test-skill"));
}

#[test]
fn test_skills_info_found() {
    let mut cmd = orquestra();
    cmd.args(["skill", "scan"]);
    cmd.assert().success();

    let mut cmd = orquestra();
    cmd.args(["skill", "info", "test-skill"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("test-skill"));
}

#[test]
fn test_skills_info_not_found() {
    let mut cmd = orquestra();
    cmd.args(["skill", "scan"]);
    cmd.assert().success();

    let mut cmd = orquestra();
    cmd.args(["skill", "info", "nonexistent"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn test_skills_refresh_idempotent() {
    let mut cmd = orquestra();
    cmd.args(["skill", "scan"]);
    cmd.assert().success();

    let mut cmd = orquestra();
    cmd.args(["skill", "refresh"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No changes detected"));
}

#[test]
fn test_skills_match_plan_ticket() {
    let dir = setup();
    let ticket_file = dir.join("ticket-match.json");
    fs::write(
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
    .unwrap();

    let mut cmd = orquestra();
    cmd.args(["skill", "scan"]);
    cmd.assert().success();

    let mut cmd = orquestra();
    cmd.args(["skill", "match", "--ticket", &ticket_file.to_string_lossy()]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Selected: test-skill"));
}
