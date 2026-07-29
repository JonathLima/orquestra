use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_adapter_list_shows_adapters() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.arg("adapter").arg("list");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("codex"))
        .stdout(predicate::str::contains("opencode"))
        .stdout(predicate::str::contains("claude-code"))
        .stdout(predicate::str::contains("antigravity"));
}

#[test]
fn test_adapter_list_json() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["--output", "json", "adapter", "list"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"adapters\""));
}

#[test]
fn test_adapter_detect_runs_without_panic() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.arg("adapter").arg("detect");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("codex"));
}

#[test]
fn test_adapter_inspect_opencode() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["adapter", "inspect", "opencode"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("spawnSubagent"))
        .stdout(predicate::str::contains("opencode"));
}

#[test]
fn test_adapter_inspect_json() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["--output", "json", "adapter", "inspect", "opencode"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"name\""));
}

#[test]
fn test_adapter_inspect_invalid_host() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["adapter", "inspect", "nonexistent"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown adapter"));
}
