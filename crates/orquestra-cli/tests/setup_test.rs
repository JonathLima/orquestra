use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new(name: &str) -> Self {
        let unique = format!(
            "orquestra-{}-{}-{}-{}",
            name,
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before the Unix epoch")
                .as_nanos(),
            TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed),
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("create isolated test directory");
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

fn assert_no_global_writes(paths: &[(PathBuf, SystemTime)]) {
    for (p, before) in paths {
        if p.exists() {
            let modified = p.metadata().ok().and_then(|m| m.modified().ok());
            if let Some(t) = modified {
                assert!(
                    t <= *before || (t.duration_since(*before).unwrap_or_default().as_secs() < 2),
                    "path was modified during setup: {}",
                    p.display()
                );
            }
        }
    }
}

fn snapshot_global_paths() -> Vec<(PathBuf, SystemTime)> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default();
    let paths = vec![
        home.join(".agents"),
        home.join(".orquestra"),
        home.join(".claude").join("skills"),
        home.join(".config").join("opencode").join("skills"),
    ];
    paths
        .into_iter()
        .map(|p| {
            let mtime = p
                .exists()
                .then(|| p.metadata().ok().and_then(|m| m.modified().ok()))
                .flatten()
                .unwrap_or(UNIX_EPOCH);
            (p, mtime)
        })
        .collect()
}

fn dry_run_lists_exact_skill(output: &str, skill_name: &str) -> bool {
    let expected_detail = format!("({skill_name} SKILL.md)");
    output
        .lines()
        .any(|line| line.contains("COPY") && line.contains(&expected_detail))
}

#[test]
fn test_setup_dry_run_shows_plan() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--dry-run", "--host", "opencode"]);
    let output = cmd.output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    for skill_name in [
        "orquestra-orchestrator",
        "orquestra-planner",
        "orquestra-router",
        "orquestra-verifier",
        "orquestra-grill",
        "orquestra-grill-with-docs",
    ] {
        assert!(
            dry_run_lists_exact_skill(&stdout, skill_name),
            "dry-run did not list an exact copy for {skill_name}"
        );
    }
}

#[test]
fn test_setup_dry_run_includes_official_grill_skills_for_every_host() {
    for host in ["codex", "claude-code", "opencode", "antigravity"] {
        let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
        cmd.args(["setup", "--dry-run", "--host", host]);
        let output = cmd.output().unwrap();
        assert!(output.status.success(), "dry-run failed for {host}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            dry_run_lists_exact_skill(&stdout, "orquestra-grill"),
            "{host} did not list the standard grill skill"
        );
        assert!(
            dry_run_lists_exact_skill(&stdout, "orquestra-grill-with-docs"),
            "{host} did not list the document-aware grill skill"
        );
    }
}

#[test]
fn test_dry_run_exact_match_does_not_confuse_the_two_grill_skills() {
    let with_docs_only =
        "COPY /skills/orquestra-grill-with-docs/SKILL.md (orquestra-grill-with-docs SKILL.md)";
    assert!(!dry_run_lists_exact_skill(
        with_docs_only,
        "orquestra-grill"
    ));
    assert!(dry_run_lists_exact_skill(
        with_docs_only,
        "orquestra-grill-with-docs"
    ));
}

#[test]
fn test_setup_dry_run_json_output() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args([
        "--output",
        "json",
        "setup",
        "--dry-run",
        "--host",
        "opencode",
    ]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"host\""));
}

#[test]
fn test_setup_requires_host_flag() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--dry-run"]);
    cmd.assert().failure();
}

#[test]
fn test_setup_invalid_host() {
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--dry-run", "--host", "nonexistent"]);
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Unknown host"));
}

#[test]
fn test_setup_succeeds_in_clean_dir() {
    let dir = TestDir::new("setup-clean");
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--host", "opencode"])
        .current_dir(dir.path());
    cmd.assert().success();
}

#[test]
fn test_setup_installs_skills_locally() {
    let dir = TestDir::new("setup-skills");
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--host", "opencode"])
        .current_dir(dir.path());
    cmd.assert().success();
    let installed = dir
        .path()
        .join(".opencode")
        .join("skills")
        .join("orquestra-orchestrator")
        .join("SKILL.md");
    assert!(
        installed.exists(),
        "expected skill installed at: {}",
        installed.display()
    );
}

#[test]
fn test_setup_copies_official_grill_skills_for_every_host() {
    for (host, skills_dir) in [
        ("codex", [".agents", "skills"]),
        ("claude-code", [".claude", "skills"]),
        ("opencode", [".opencode", "skills"]),
        ("antigravity", [".agents", "skills"]),
    ] {
        let dir = TestDir::new(&format!("setup-{host}-official-grills"));
        let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
        cmd.args(["setup", "--host", host]).current_dir(dir.path());
        cmd.assert().success();

        for skill_name in ["orquestra-grill", "orquestra-grill-with-docs"] {
            let installed = dir
                .path()
                .join(skills_dir[0])
                .join(skills_dir[1])
                .join(skill_name)
                .join("SKILL.md");
            assert!(
                installed.is_file(),
                "{host} did not install {}",
                installed.display()
            );
        }
    }
}

#[test]
fn test_setup_skills_are_discoverable_in_the_real_inventory() {
    let dir = TestDir::new("setup-scan-official-grills");
    let mut setup = Command::cargo_bin("orquestra-cli").unwrap();
    setup
        .args(["setup", "--host", "opencode"])
        .current_dir(dir.path());
    setup.assert().success();

    let mut scan = Command::cargo_bin("orquestra-cli").unwrap();
    scan.args(["skill", "scan"]).current_dir(dir.path());
    scan.assert().success();

    let inventory_path = dir.path().join(".orquestra/skills_inventory.json");
    let inventory: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&inventory_path).expect("read generated inventory"),
    )
    .expect("parse generated inventory");
    let names: Vec<&str> = inventory["skills"]
        .as_array()
        .expect("inventory skills array")
        .iter()
        .filter_map(|skill| skill["name"].as_str())
        .collect();

    assert!(names.contains(&"orquestra-grill"));
    assert!(names.contains(&"orquestra-grill-with-docs"));
}

#[test]
fn test_setup_does_not_write_global_agents() {
    let before = snapshot_global_paths();
    let dir = TestDir::new("setup-no-global");
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--host", "opencode"])
        .current_dir(dir.path());
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "setup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_no_global_writes(&before);
}

#[test]
fn test_setup_does_not_touch_existing_global_skills() {
    let before = snapshot_global_paths();
    let dir = TestDir::new("setup-no-global-skills");
    let mut cmd = Command::cargo_bin("orquestra-cli").unwrap();
    cmd.args(["setup", "--host", "codex"])
        .current_dir(dir.path());
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "setup with codex host failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_no_global_writes(&before);
}
