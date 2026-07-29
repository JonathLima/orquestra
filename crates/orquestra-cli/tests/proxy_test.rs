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
            "orquestra-proxy-test-{}-{}-{}",
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
    command.current_dir(dir.path()).env("PATH", test_path(dir));
    command
}

fn test_path(dir: &TestDir) -> String {
    let current = std::env::var("PATH").unwrap_or_default();
    let bin = dir.path().join("bin");
    let separator = if cfg!(windows) { ";" } else { ":" };
    format!("{}{}{}", bin.display(), separator, current)
}

fn write_fake_opencode(dir: &TestDir) {
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).expect("create bin dir");
    if cfg!(windows) {
        std::fs::write(
            bin.join("opencode.cmd"),
            "@echo off\r\nif \"%1\"==\"--version\" (echo fake-opencode 2.0& exit /b 0)\r\nif \"%1\"==\"/orquestra-orchestrator:build\" (echo simulating orchestrator flow& exit /b 0)\r\nif \"%1\"==\"fail\" (echo intentional failure >&2& exit /b 42)\r\necho fake-opencode:%*\r\nexit /b 0\r\n",
        )
        .expect("write fake opencode");
    } else {
        let path = bin.join("opencode");
        std::fs::write(
            &path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo fake-opencode 2.0; exit 0; fi\nif [ \"$1\" = \"/orquestra-orchestrator:build\" ]; then echo simulating orchestrator flow; exit 0; fi\nif [ \"$1\" = \"fail\" ]; then echo intentional failure >&2; exit 42; fi\necho \"fake-opencode:$*\"\n",
        )
        .expect("write fake opencode");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
    }
}

fn write_fake_codex(dir: &TestDir) {
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).expect("create bin dir");
    if cfg!(windows) {
        std::fs::write(
            bin.join("codex.cmd"),
            "@echo off\r\nif \"%1\"==\"--version\" (echo fake-codex 1.0& exit /b 0)\r\necho fake-codex:%*\r\nexit /b 0\r\n",
        )
        .expect("write fake codex");
    } else {
        let path = bin.join("codex");
        std::fs::write(
            &path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo fake-codex 1.0; exit 0; fi\necho \"fake-codex:$*\"\n",
        )
        .expect("write fake codex");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&path, permissions).unwrap();
        }
    }
}

fn disable_proxy_in_project_config(dir: &TestDir) {
    let config_dir = dir.path().join(".orquestra");
    std::fs::create_dir_all(&config_dir).expect("create .orquestra config dir");
    std::fs::write(
        config_dir.join("config.toml"),
        "[security]\nallow_proxy = false\n",
    )
    .expect("write project config");
}

#[test]
fn test_proxy_doctor_reports_fake_host() {
    let dir = TestDir::new();
    write_fake_codex(&dir);

    command_at(&dir)
        .args(["proxy", "doctor", "codex"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Proxy host: codex"))
        .stdout(predicate::str::contains("Detected: true"));
}

#[test]
fn test_proxy_forwards_arguments_to_fake_host() {
    let dir = TestDir::new();
    write_fake_codex(&dir);

    command_at(&dir)
        .args(["proxy", "codex", "--", "hello", "world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("fake-codex:hello world"));
}

#[test]
fn test_proxy_honors_project_security_policy() {
    let dir = TestDir::new();
    write_fake_codex(&dir);
    disable_proxy_in_project_config(&dir);

    command_at(&dir)
        .args(["proxy", "codex", "--", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Proxy execution is disabled by policy",
        ));
}

#[test]
fn test_proxy_propagates_child_exit_code() {
    let dir = TestDir::new();
    write_fake_opencode(&dir);

    command_at(&dir)
        .args(["proxy", "opencode", "--", "fail"])
        .assert()
        .failure()
        .code(42);
}

#[test]
fn test_proxy_doctor_works_for_any_host() {
    let dir = TestDir::new();
    write_fake_opencode(&dir);

    command_at(&dir)
        .args(["proxy", "doctor", "opencode"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Proxy host: opencode"))
        .stdout(predicate::str::contains("Detected: true"))
        .stdout(predicate::str::contains("Policy: enabled"));
}

#[test]
fn test_proxy_forwards_orchestrator_trigger() {
    let dir = TestDir::new();
    write_fake_opencode(&dir);

    command_at(&dir)
        .args(["proxy", "opencode", "--", "/orquestra-orchestrator:build"])
        .assert()
        .success()
        .stdout(predicate::str::contains("simulating orchestrator flow"));
}

#[test]
fn test_project_security_policy_overrides_proxy_env_allow() {
    let dir = TestDir::new();
    write_fake_codex(&dir);
    disable_proxy_in_project_config(&dir);

    let mut command = command_at(&dir);
    command.env("ORQUESTRA_SECURITY_ALLOW_PROXY", "true");
    command
        .args(["proxy", "codex", "--", "hello"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Proxy execution is disabled by policy",
        ));
}
