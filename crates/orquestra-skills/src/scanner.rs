use chrono::Utc;
use orquestra_core::config::home_dir;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::warn;

use crate::frontmatter::parse_frontmatter;
use crate::types::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrainSkillProvenance {
    status: String,
    source_skill_id: String,
    created_at: String,
}

pub struct SkillScannerConfig {
    pub max_file_size: u64,
    pub max_dir_entries: usize,
}

impl Default for SkillScannerConfig {
    fn default() -> Self {
        Self {
            max_file_size: 1_048_576,
            max_dir_entries: 1000,
        }
    }
}

pub fn compute_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub fn scan_skill_dir(
    dir: &Path,
    scope: &str,
    trust: TrustLevel,
    config: &SkillScannerConfig,
) -> Vec<SkillInfo> {
    let mut skills = Vec::new();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return skills,
    };

    let mut count = 0;
    for entry in entries.filter_map(|e| e.ok()) {
        count += 1;
        if count > config.max_dir_entries {
            warn!(
                "Directory {} exceeds max entries, sampling stopped",
                dir.display()
            );
            break;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) == Some("_pending") {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        let skill_md = match std::fs::canonicalize(&skill_md) {
            Ok(c) => c,
            Err(_) => {
                warn!(
                    "Dangling symlink or inaccessible path: {}",
                    skill_md.display()
                );
                continue;
            }
        };
        let metadata = match std::fs::metadata(&skill_md) {
            Ok(m) => m,
            Err(e) => {
                warn!("Cannot read metadata for {}: {e}", skill_md.display());
                continue;
            }
        };
        if metadata.len() > config.max_file_size {
            warn!("SKILL.md too large ({} bytes), skipping", metadata.len());
            continue;
        }
        let content = match std::fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(e) => {
                warn!("Cannot read {}: {e}", skill_md.display());
                continue;
            }
        };
        let hash = compute_hash(content.as_bytes());
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                warn!("Invalid UTF-8 filename, skipping: {}", path.display());
                continue;
            }
        };

        let (description, version, capabilities, metadata_map, status) =
            match parse_frontmatter(&content) {
                Ok(Some(fm)) => (
                    fm.description,
                    fm.version,
                    fm.capabilities,
                    fm.metadata,
                    SkillStatus::Active,
                ),
                Ok(None) => (
                    String::new(),
                    None,
                    vec![],
                    HashMap::new(),
                    SkillStatus::Active,
                ),
                Err(e) => {
                    warn!("Invalid frontmatter in {}: {e}", skill_md.display());
                    (
                        String::new(),
                        None,
                        vec![],
                        HashMap::new(),
                        SkillStatus::Invalid,
                    )
                }
            };

        let (resolved_trust, resolved_provenance) =
            brain_provenance(&path).unwrap_or_else(|| (trust.clone(), Provenance::Local));

        skills.push(SkillInfo {
            id: name.clone(),
            name,
            description,
            version,
            scope: scope.to_string(),
            source_path: skill_md,
            hash,
            trust: resolved_trust,
            status,
            capabilities,
            metadata: metadata_map,
            provenance: resolved_provenance,
            inspected_at: Utc::now(),
        });
    }
    skills
}

fn brain_provenance(skill_dir: &Path) -> Option<(TrustLevel, Provenance)> {
    let content = std::fs::read_to_string(skill_dir.join("PROVENANCE.json")).ok()?;
    let provenance: BrainSkillProvenance = serde_json::from_str(&content).ok()?;
    let trust = match provenance.status.as_str() {
        "approved" => TrustLevel::BrainApproved,
        "pending" => TrustLevel::BrainPending,
        _ => return None,
    };
    Some((
        trust,
        Provenance::BrainAdapted {
            from: provenance.source_skill_id,
            retrieved_at: provenance.created_at,
        },
    ))
}

pub fn default_scan_sources() -> Vec<ScanSource> {
    let home = home_dir();
    vec![
        ScanSource {
            scope: "project".to_string(),
            path: PathBuf::from(".orquestra/skills"),
        },
        ScanSource {
            scope: "project".to_string(),
            path: PathBuf::from(".agents/skills"),
        },
        ScanSource {
            scope: "project".to_string(),
            path: PathBuf::from(".claude/skills"),
        },
        ScanSource {
            scope: "project".to_string(),
            path: PathBuf::from(".opencode/skills"),
        },
        ScanSource {
            scope: "global".to_string(),
            path: home.join(".agents/skills"),
        },
        ScanSource {
            scope: "global".to_string(),
            path: home.join(".config/opencode/skills"),
        },
        ScanSource {
            scope: "global".to_string(),
            path: home.join(".claude/skills"),
        },
    ]
}

pub fn scan_all(sources: &[ScanSource], config: &SkillScannerConfig) -> Vec<SkillInfo> {
    let mut all = Vec::new();
    for source in sources {
        let trust = match source.scope.as_str() {
            "global" => TrustLevel::UserGlobal,
            _ => TrustLevel::UserProject,
        };
        let skills = scan_skill_dir(&source.path, &source.scope, trust, config);
        all.extend(skills);
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_consistency() {
        let h1 = compute_hash(b"hello");
        let h2 = compute_hash(b"hello");
        assert_eq!(h1, h2);
        assert!(h1.starts_with("sha256:"));
    }

    #[test]
    fn test_compute_hash_different() {
        let h1 = compute_hash(b"hello");
        let h2 = compute_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_default_sources_has_entries() {
        let sources = default_scan_sources();
        assert!(!sources.is_empty());
        assert!(sources.iter().any(|s| s.scope == "project"));
        assert!(sources.iter().any(|s| s.scope == "global"));
    }

    #[test]
    fn approved_brain_skill_preserves_trust_and_provenance() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skills_dir = dir.path().join(".orquestra").join("skills");
        let skill_dir = skills_dir.join("brain-local-test");
        std::fs::create_dir_all(&skill_dir).expect("create skill dir");
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: brain-local-test\ndescription: Adapted\n---\n# Brain\n",
        )
        .expect("write skill");
        std::fs::write(
            skill_dir.join("PROVENANCE.json"),
            r#"{
  "id": "candidate-1",
  "skillName": "brain-local-test",
  "ticketId": "T1",
  "status": "approved",
  "sourceSkillId": "source-skill",
  "createdAt": "2026-07-27T00:00:00.000Z",
  "path": ".orquestra/skills/brain-local-test"
}"#,
        )
        .expect("write provenance");

        let skills = scan_skill_dir(
            &skills_dir,
            "project",
            TrustLevel::UserProject,
            &SkillScannerConfig::default(),
        );

        let skill = skills
            .iter()
            .find(|skill| skill.name == "brain-local-test")
            .expect("scan approved brain skill");
        assert_eq!(skill.trust, TrustLevel::BrainApproved);
        assert_eq!(
            skill.provenance,
            Provenance::BrainAdapted {
                from: "source-skill".to_string(),
                retrieved_at: "2026-07-27T00:00:00.000Z".to_string()
            }
        );
    }
}
