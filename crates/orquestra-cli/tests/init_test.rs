use assert_cmd::Command;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        let unique = format!(
            "orquestra-cli-init-test-{}-{}-{}",
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

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, content).expect("write test file");
        path
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

fn extract_session_id(output: &str) -> Option<&str> {
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("Init session started: ") {
            return Some(rest.trim());
        }
    }
    None
}

#[test]
fn test_init_start_creates_session() {
    let dir = TestDir::new();
    let mut cmd = command_at(&dir);
    cmd.args([
        "init",
        "start",
        "--host",
        "opencode",
        "--idea",
        "Build a CLI tool",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("Init session started:"));
}

#[test]
fn test_init_list_shows_sessions() {
    let dir = TestDir::new();
    command_at(&dir)
        .args(["init", "start", "--host", "opencode", "--idea", "test idea"])
        .assert()
        .success();
    command_at(&dir)
        .args(["init", "list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("test idea"));
}

#[test]
fn test_init_plan_blocks_before_convergence() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Refactor legacy Express API",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    command_at(&dir)
        .args([
            "init",
            "plan",
            "--session-id",
            session_id,
            "--max-tickets",
            "4",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains(
            "Cannot generate plan before convergence",
        ));
}

#[test]
fn test_init_classify_detects_intent() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Modernizar API Express legada",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    command_at(&dir)
        .args(["init", "classify", "--session-id", session_id])
        .assert()
        .success()
        .stdout(predicates::str::contains("Migrate"));
}

#[test]
fn test_init_add_requirement_appends_to_state() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Build something",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    command_at(&dir)
        .args([
            "init",
            "add-requirement",
            "--session-id",
            session_id,
            "--text",
            "Handle 10k requests/min",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Total: 1 requirements"));
}

#[test]
fn test_init_classify_with_refinement_response() {
    let dir = TestDir::new();
    // Use a low-confidence idea so refinement (conf 0.95) overrides the heuristic
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "look at the current state",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let refinement = r#"{"intent":"migrate","scope":"medium","audience":"developer","confidence":0.95,"reasoning":"LLM says it's a migration project"}"#;
    command_at(&dir)
        .args([
            "init",
            "classify",
            "--session-id",
            session_id,
            "--refinement-response",
            refinement,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Migrate"));
}

#[test]
fn test_init_cancel_ends_session() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Cancel test",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    command_at(&dir)
        .args(["init", "cancel", "--session-id", session_id])
        .assert()
        .success();
}

fn sample_source(url: &str, title: &str) -> String {
    format!(
        r#"{{"url":"{}","title":"{}","authority":1.0,"recency":1.0,"agreement":1.0,"claims":["claim"],"score":0.0,"fetchedAt":"2026-07-28T00:00:00Z"}}"#,
        url, title
    )
}

#[test]
fn test_init_research_creates_topic() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "build a pipeline",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "Rust async patterns",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Research started:"));
}

#[test]
fn test_init_store_results_scores_sources() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "test store",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "test topic",
        ])
        .output()
        .expect("research");
    let research_out = String::from_utf8_lossy(&research.stdout);
    let topic_id = research_out
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic ID line");
    let sources_json = format!(
        "[{},{},{},{},{}]",
        sample_source("https://nodejs.org/api/http.html", "Primary"),
        sample_source("https://a.com", "A"),
        sample_source("https://b.com", "B"),
        sample_source("https://c.com", "C"),
        sample_source("https://d.com", "D"),
    );
    command_at(&dir)
        .args([
            "init",
            "store-research",
            "--session-id",
            session_id,
            "--topic-id",
            topic_id,
            "--sources-json",
            &sources_json,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Avg score:"));
}

#[test]
fn test_init_store_research_rejects_empty_sources() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "test reject",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "empty test",
        ])
        .output()
        .expect("research");
    let research_out = String::from_utf8_lossy(&research.stdout);
    let topic_id = research_out
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic ID line");
    command_at(&dir)
        .args([
            "init",
            "store-research",
            "--session-id",
            session_id,
            "--topic-id",
            topic_id,
            "--sources-json",
            "[]",
        ])
        .assert()
        .failure()
        .stderr(predicates::str::contains("At least one source"));
}

#[test]
fn test_init_evaluate_requires_all_convergence_gates() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args(["init", "start", "--host", "opencode", "--idea", "test eval"])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "test",
        ])
        .output()
        .expect("research");
    let research_out = String::from_utf8_lossy(&research.stdout);
    let topic_id = research_out
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic ID line");
    let sources_json = format!(
        "[{},{},{},{},{}]",
        sample_source("https://nodejs.org/api/http.html", "Primary"),
        sample_source("https://rust-lang.org/learn", "Rust"),
        sample_source("https://docs.rs/http/latest/http/", "Docs.rs"),
        sample_source("https://w3.org/TR/fetch/", "W3C"),
        sample_source("https://ietf.org/standards/", "IETF"),
    );
    command_at(&dir)
        .args([
            "init",
            "store-research",
            "--session-id",
            session_id,
            "--topic-id",
            topic_id,
            "--sources-json",
            &sources_json,
        ])
        .assert()
        .success();
    command_at(&dir)
        .args(["init", "evaluate", "--session-id", session_id])
        .assert()
        .success()
        .stdout(predicates::str::contains("Questioning"))
        .stdout(predicates::str::contains("classification is missing"))
        .stdout(predicates::str::contains(
            "discovery round 1 < configured minimum 3",
        ));
}

#[test]
fn test_init_evaluate_fails_on_cancelled_session() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "cancel eval",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    command_at(&dir)
        .args(["init", "cancel", "--session-id", session_id])
        .assert()
        .success();
    command_at(&dir)
        .args(["init", "evaluate", "--session-id", session_id])
        .assert()
        .failure()
        .stderr(predicates::str::contains("terminal phase"));
}

#[test]
fn test_init_request_research_emits_delegation_envelope() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Research delegation",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "database choice real-time chat",
        ])
        .output()
        .expect("research");
    let topic_id = String::from_utf8_lossy(&research.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic ID")
        .to_string();

    command_at(&dir)
        .args([
            "init",
            "request-research",
            "--session-id",
            session_id,
            "--topic-id",
            &topic_id,
            "--host",
            "opencode",
            "--max-sources",
            "4",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Research delegation for topic"))
        .stdout(predicates::str::contains("webSearch: WebSearch tool"))
        .stdout(predicates::str::contains("store-research"))
        .stdout(predicates::str::contains("markdown-file"));
}

#[test]
fn test_init_request_research_uses_configured_minimum_sources() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Research defaults",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "default source count",
        ])
        .output()
        .expect("research");
    let research_stdout = String::from_utf8_lossy(&research.stdout);
    let topic_id = research_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic ID");
    std::fs::write(
        dir.path().join(".orquestra").join("config.toml"),
        "[init.research]\nmin_sources_per_topic = 7\n",
    )
    .expect("project config");

    command_at(&dir)
        .args([
            "init",
            "request-research",
            "--session-id",
            session_id,
            "--topic-id",
            topic_id,
            "--host",
            "opencode",
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Max sources: 7"));
}

#[test]
fn test_init_store_research_uses_configured_claim_agreement() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Configured agreement",
        ])
        .output()
        .expect("start");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let session_id = extract_session_id(&stdout).expect("session ID");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "agreement config",
        ])
        .output()
        .expect("research");
    let research_stdout = String::from_utf8_lossy(&research.stdout);
    let topic_id = research_stdout
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic ID");
    std::fs::write(
        dir.path().join(".orquestra").join("config.toml"),
        "[init.research]\nmin_agreement_for_confirmed = 3\n",
    )
    .expect("project config");
    let sources_json = format!(
        "[{},{}]",
        sample_source("https://nodejs.org/a", "A"),
        sample_source("https://example.net/b", "B"),
    );

    command_at(&dir)
        .args([
            "init",
            "store-research",
            "--session-id",
            session_id,
            "--topic-id",
            topic_id,
            "--sources-json",
            &sources_json,
        ])
        .assert()
        .success();

    let topic_path = dir
        .path()
        .join(".orquestra")
        .join("init")
        .join(session_id)
        .join("research")
        .join(format!("{topic_id}.json"));
    let topic: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(topic_path).expect("topic"))
            .expect("topic JSON");
    assert!(
        topic["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .all(|source| source["agreement"] == 0.0)
    );
}

#[test]
fn test_init_store_research_parses_markdown_file() {
    let dir = TestDir::new();
    let output = command_at(&dir)
        .args([
            "init",
            "start",
            "--host",
            "opencode",
            "--idea",
            "Markdown parse",
        ])
        .output()
        .expect("start");
    let stdout1 = String::from_utf8_lossy(&output.stdout).into_owned();
    let session_id = extract_session_id(&stdout1).expect("sid");
    let research = command_at(&dir)
        .args([
            "init",
            "research",
            "--session-id",
            session_id,
            "--topic",
            "markdown topic",
        ])
        .output()
        .expect("research");
    let topic_id = String::from_utf8_lossy(&research.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Research started: "))
        .expect("topic id")
        .to_string();

    let md = dir.write(
        "wie-response.md",
        "## Search Results — 4 results\nQuery: `markdown topic 2026`\n\n### 1. A\nURL: https://a.com\nSource: 🔗 docs.rs  👉  Score: 80/100\nClaim: normalized claim\nSnippet: A snippet\n\n### 2. B\nURL: https://b.com\nSource: 🔗 docs.rs  👉  Score: 80/100\nClaim: normalized claim\nSnippet: B snippet\n\n### 3. C\nURL: https://c.com\nSource: 🔗 docs.rs  👉  Score: 80/100\nClaim: normalized claim\nSnippet: C snippet\n\n### 4. D\nURL: https://d.com\nSource: 🔗 docs.rs  👉  Score: 80/100\nClaim: normalized claim\nSnippet: D snippet\n",
    );
    let md_path = md.to_string_lossy().to_string();

    command_at(&dir)
        .args([
            "init",
            "store-research",
            "--session-id",
            session_id,
            "--topic-id",
            &topic_id,
            "--markdown-file",
            &md_path,
        ])
        .assert()
        .success()
        .stdout(predicates::str::contains("Status: Completed"))
        .stdout(predicates::str::contains("Sources: 4"));
}
