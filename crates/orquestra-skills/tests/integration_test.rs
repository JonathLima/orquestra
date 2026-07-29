use std::fs;
use std::path::PathBuf;
use std::sync::Once;

static INIT: Once = Once::new();

fn test_dir() -> PathBuf {
    let dir = std::env::temp_dir()
        .join("orquestra-skills-int-test")
        .join("skills");
    INIT.call_once(|| {
        let _ = fs::create_dir_all(&dir);
        let skill_dir = dir.join("test-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        let content = r#"---
name: test-skill
description: A skill for integration tests
version: 1.0.0
---
# Test Skill

Hello from integration tests.
"#;
        fs::write(skill_dir.join("SKILL.md"), content).unwrap();
    });
    dir
}

fn cleanup() {
    let _ = fs::remove_dir_all(".orquestra");
}

#[test]
fn test_scan_parses_frontmatter_from_real_file() {
    let dir = test_dir();
    let config = orquestra_skills::SkillScannerConfig::default();
    let skills = orquestra_skills::scan_skill_dir(
        &dir,
        "test",
        orquestra_skills::TrustLevel::UserGlobal,
        &config,
    );
    assert!(!skills.is_empty(), "should find test-skill");
    let skill = skills.iter().find(|s| s.name == "test-skill").unwrap();
    assert_eq!(skill.name, "test-skill");
    assert_eq!(skill.description, "A skill for integration tests");
    assert_eq!(skill.version.as_deref(), Some("1.0.0"));
    assert_eq!(skill.scope, "test");
    assert!(skill.hash.starts_with("sha256:"));
}

#[test]
fn test_full_scan_write_read_cycle() {
    let _dir = test_dir();
    let config = orquestra_skills::SkillScannerConfig::default();
    let sources = orquestra_skills::default_scan_sources();
    let skills = orquestra_skills::scan_all(&sources, &config);
    orquestra_skills::write_inventory(&skills, &sources).unwrap();
    let inv = orquestra_skills::read_inventory().unwrap().unwrap();
    assert_eq!(inv.schema_version, 1);
    assert!(inv.sources.len() >= 3);
    cleanup();
}

#[test]
fn test_render_markdown_from_scan() {
    let _dir = test_dir();
    let config = orquestra_skills::SkillScannerConfig::default();
    let sources = orquestra_skills::default_scan_sources();
    let skills = orquestra_skills::scan_all(&sources, &config);
    let sources = orquestra_skills::default_scan_sources();
    let inv = orquestra_skills::SkillInventory {
        schema_version: 1,
        generated_at: chrono::Utc::now(),
        sources,
        skills,
    };
    let md = orquestra_skills::render_markdown(&inv);
    assert!(md.contains("# Skills Inventory"));
    assert!(md.contains("## Sources"));
    assert!(md.contains("## Skills"));
}
