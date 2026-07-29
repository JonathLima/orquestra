use chrono::Utc;
use orquestra_core::error::OrquestraError;
use std::path::{Path, PathBuf};

use crate::types::*;

pub fn inventory_path() -> PathBuf {
    PathBuf::from(".orquestra/skills_inventory.json")
}

pub fn inventory_md_path() -> PathBuf {
    PathBuf::from(".orquestra/skills_inventory.md")
}

pub fn read_inventory() -> Result<Option<SkillInventory>, OrquestraError> {
    read_inventory_at(Path::new("."))
}

pub fn read_inventory_at(project_dir: &Path) -> Result<Option<SkillInventory>, OrquestraError> {
    let path = project_dir.join(inventory_path());
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| OrquestraError::from(format!("Cannot read inventory: {e}")))?;
    let inv: SkillInventory = serde_json::from_str(&content)
        .map_err(|e| OrquestraError::from(format!("Invalid inventory JSON: {e}")))?;
    Ok(Some(inv))
}

pub fn write_inventory(skills: &[SkillInfo], sources: &[ScanSource]) -> Result<(), OrquestraError> {
    write_inventory_at(Path::new("."), skills, sources)
}

pub fn write_inventory_at(
    project_dir: &Path,
    skills: &[SkillInfo],
    sources: &[ScanSource],
) -> Result<(), OrquestraError> {
    let dir = project_dir.join(".orquestra");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .map_err(|e| OrquestraError::from(format!("Cannot create .orquestra/: {e}")))?;
    }
    let inventory = SkillInventory {
        schema_version: 1,
        generated_at: Utc::now(),
        sources: sources.to_vec(),
        skills: skills.to_vec(),
    };
    let json = serde_json::to_string_pretty(&inventory)
        .map_err(|e| OrquestraError::from(format!("Cannot serialize inventory: {e}")))?;
    std::fs::write(project_dir.join(inventory_path()), &json)
        .map_err(|e| OrquestraError::from(format!("Cannot write inventory: {e}")))?;
    let md = render_markdown(&inventory);
    std::fs::write(project_dir.join(inventory_md_path()), &md)
        .map_err(|e| OrquestraError::from(format!("Cannot write inventory.md: {e}")))?;
    Ok(())
}

pub fn render_markdown(inventory: &SkillInventory) -> String {
    let mut out = String::new();
    out.push_str("# Skills Inventory\n\n");
    out.push_str(&format!(
        "Generated: {}\n\n",
        inventory.generated_at.format("%Y-%m-%d %H:%M:%S UTC")
    ));
    out.push_str(&format!("Schema version: {}\n\n", inventory.schema_version));
    out.push_str("## Sources\n\n");
    for src in &inventory.sources {
        out.push_str(&format!("- **{}**: `{}`\n", src.scope, src.path.display()));
    }
    out.push_str("\n## Skills\n\n");
    if inventory.skills.is_empty() {
        out.push_str("*(no skills found)*\n");
    }
    for skill in &inventory.skills {
        out.push_str(&format!("### {}\n", skill.name));
        out.push_str(&format!("- **ID:** {}\n", skill.id));
        out.push_str(&format!("- **Description:** {}\n", skill.description));
        if let Some(v) = &skill.version {
            out.push_str(&format!("- **Version:** {v}\n"));
        }
        out.push_str(&format!("- **Scope:** {}\n", skill.scope));
        out.push_str(&format!("- **Trust:** {:?}\n", skill.trust));
        out.push_str(&format!("- **Status:** {:?}\n", skill.status));
        out.push_str(&format!("- **Hash:** `{}`\n", skill.hash));
        out.push_str(&format!("- **Path:** `{}`\n", skill.source_path.display()));
        if !skill.capabilities.is_empty() {
            out.push_str(&format!(
                "- **Capabilities:** {}\n",
                skill.capabilities.join(", ")
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn sample_inventory() -> SkillInventory {
        SkillInventory {
            schema_version: 1,
            generated_at: Utc::now(),
            sources: vec![ScanSource {
                scope: "global".to_string(),
                path: PathBuf::from("~/.agents/skills"),
            }],
            skills: vec![SkillInfo {
                id: "test-skill".to_string(),
                name: "test-skill".to_string(),
                description: "A test".to_string(),
                version: Some("1.0".to_string()),
                scope: "global".to_string(),
                source_path: PathBuf::from("/tmp/skills/test-skill/SKILL.md"),
                hash: "sha256:abc".to_string(),
                trust: TrustLevel::UserGlobal,
                status: SkillStatus::Active,
                capabilities: vec![],
                metadata: HashMap::new(),
                provenance: Provenance::Local,
                inspected_at: Utc::now(),
            }],
        }
    }

    #[test]
    fn test_render_markdown_includes_name() {
        let inv = sample_inventory();
        let md = render_markdown(&inv);
        assert!(md.contains("test-skill"));
        assert!(md.contains("A test"));
        assert!(md.contains("sha256:abc"));
    }

    #[test]
    fn test_render_markdown_empty() {
        let inv = SkillInventory {
            schema_version: 1,
            generated_at: Utc::now(),
            sources: vec![],
            skills: vec![],
        };
        let md = render_markdown(&inv);
        assert!(md.contains("no skills found"));
    }

    #[test]
    fn test_inventory_round_trip() {
        let inv = sample_inventory();
        let json = serde_json::to_string_pretty(&inv).unwrap();
        let deserialized: SkillInventory = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.schema_version, 1);
        assert_eq!(deserialized.skills.len(), 1);
        assert_eq!(deserialized.skills[0].name, "test-skill");
    }
}
