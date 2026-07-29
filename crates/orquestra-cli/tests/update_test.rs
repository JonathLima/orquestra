use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn command_at(dir: &TempDir) -> Command {
    let mut command = Command::cargo_bin("orquestra-cli").expect("find CLI binary");
    command
        .current_dir(dir.path())
        .env("PATH", test_path(dir.path()))
        .env(
            "ORQUESTRA_UPDATE_CACHE_DIR",
            dir.path().join("update-cache"),
        );
    command
}

fn test_path(dir: &Path) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let separator = if cfg!(windows) { ";" } else { ":" };
    format!("{}{}{}", dir.join("bin").display(), separator, current)
}

fn write_fake_npm(dir: &TempDir, version: &str) {
    let bin = dir.path().join("bin");
    fs::create_dir_all(&bin).expect("create fake npm directory");
    if cfg!(windows) {
        fs::write(
            bin.join("npm.cmd"),
            format!("@echo off\r\necho \"{version}\"\r\n"),
        )
        .expect("write fake npm");
    } else {
        let path = bin.join("npm");
        fs::write(&path, format!("#!/bin/sh\necho '\"{version}\"'\n")).expect("write fake npm");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
        }
    }
}

fn run_check(dir: &TempDir) -> serde_json::Value {
    let output = command_at(dir)
        .args(["--output", "json", "update", "check"])
        .output()
        .expect("run update check");
    assert!(
        output.status.success(),
        "update check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse update check JSON")
}

#[test]
fn update_check_reports_available_version_and_reuses_fresh_cache() {
    let dir = tempfile::tempdir().expect("create update test directory");
    write_fake_npm(&dir, "0.2.0");

    let first = run_check(&dir);
    assert_eq!(first["status"], "available");
    assert_eq!(first["currentVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(first["latestVersion"], "0.2.0");
    assert_eq!(first["source"], "registry");

    write_fake_npm(&dir, "0.3.0");
    let cached = run_check(&dir);
    assert_eq!(cached["latestVersion"], "0.2.0");
    assert_eq!(cached["source"], "cache");
}

#[test]
fn update_check_can_be_disabled() {
    let dir = tempfile::tempdir().expect("create update test directory");
    let output = command_at(&dir)
        .env("ORQUESTRA_DISABLE_UPDATE_CHECK", "1")
        .args(["--output", "json", "update", "check"])
        .output()
        .expect("run disabled update check");
    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse disabled check JSON");
    assert_eq!(value["status"], "disabled");
}

#[test]
fn update_check_does_not_fail_when_npm_is_unavailable() {
    let dir = tempfile::tempdir().expect("create update test directory");
    let empty_path: PathBuf = dir.path().join("empty-bin");
    fs::create_dir(&empty_path).expect("create empty PATH");
    let output = command_at(&dir)
        .env("PATH", &empty_path)
        .args(["--output", "json", "update", "check"])
        .output()
        .expect("run unavailable npm update check");
    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse unavailable check JSON");
    assert_eq!(value["status"], "unknown");
}
