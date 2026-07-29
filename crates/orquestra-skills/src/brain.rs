use chrono::Utc;
use orquestra_core::error::OrquestraError;
use orquestra_plan::Ticket;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::SkillInfo;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainPolicy {
    pub external_discovery_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrainCandidate {
    pub id: String,
    pub skill_name: String,
    pub ticket_id: String,
    pub status: BrainCandidateStatus,
    pub source_skill_id: String,
    pub created_at: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BrainCandidateStatus {
    Pending,
    Approved,
    Rejected,
}

pub fn brain_policy() -> BrainPolicy {
    BrainPolicy::default()
}

pub fn adapt_local_skill(
    project_dir: &Path,
    ticket: &Ticket,
    source_skill: &SkillInfo,
) -> Result<BrainCandidate, OrquestraError> {
    let created_at = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let skill_name = sanitize_identifier(&format!("brain-{}-{}", ticket.id, source_skill.name))?;
    let id = sanitize_identifier(&format!("{}-{}", skill_name, Utc::now().timestamp_millis()))?;
    let pending_dir = pending_dir(project_dir).join(&id);

    if pending_dir.exists() {
        return Err(OrquestraError::from(format!(
            "BRAIN candidate already exists: {id}"
        )));
    }

    let source = std::fs::read_to_string(&source_skill.source_path).map_err(|error| {
        OrquestraError::from(format!(
            "Cannot read source skill {}: {error}",
            source_skill.source_path.display()
        ))
    })?;
    std::fs::create_dir_all(&pending_dir)?;

    let adapted = format!(
        "---\nname: {skill_name}\ndescription: Project-local adaptation of {source_skill} for ticket {ticket_id}\n---\n\n# {skill_name}\n\nApply the inherited skill only to this ticket. Inspect project manifests and lockfiles first, then adapt commands, APIs, file locations, stack choices, and version-specific guidance to the versions actually present. Never modify the installed source skill.\n\n## Ticket Objective\n\n{objective}\n\n## Acceptance Criteria\n\n{acceptance_criteria}\n\n## Preferred Capabilities\n\n{capabilities}\n\n## Inherited Skill\n\nThe following source skill remains authoritative where it does not conflict with the ticket, the detected project stack, or the acceptance criteria.\n\n{source}\n",
        ticket_id = ticket.id,
        source_skill = source_skill.name,
        objective = ticket.objective,
        acceptance_criteria = ticket
            .acceptance_criteria
            .iter()
            .map(|criterion| format!("- {criterion}"))
            .collect::<Vec<_>>()
            .join("\n"),
        capabilities = ticket
            .preferred_capabilities
            .iter()
            .map(|capability| format!("- {capability}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    std::fs::write(pending_dir.join("SKILL.md"), adapted)?;

    let candidate = BrainCandidate {
        id,
        skill_name,
        ticket_id: ticket.id.clone(),
        status: BrainCandidateStatus::Pending,
        source_skill_id: source_skill.id.clone(),
        created_at,
        path: pending_dir,
    };
    write_candidate_metadata(&candidate)?;
    std::fs::write(
        candidate.path.join("REVIEW.md"),
        format!(
            "# Review {}\n\n- Ticket: `{}`\n- Source skill: `{}`\n- Status: pending\n",
            candidate.id, candidate.ticket_id, candidate.source_skill_id
        ),
    )?;

    Ok(candidate)
}

pub fn inspect_candidate(
    project_dir: &Path,
    candidate_id: &str,
) -> Result<BrainCandidate, OrquestraError> {
    validate_candidate_id(candidate_id)?;
    read_candidate_metadata(&pending_dir(project_dir).join(candidate_id))
}

pub fn approve_candidate(
    project_dir: &Path,
    candidate_id: &str,
) -> Result<BrainCandidate, OrquestraError> {
    validate_candidate_id(candidate_id)?;
    let source_dir = pending_dir(project_dir).join(candidate_id);
    let mut candidate = read_candidate_metadata(&source_dir)?;
    validate_candidate_id(&candidate.skill_name)?;
    if candidate.status != BrainCandidateStatus::Pending {
        return Err(OrquestraError::from(format!(
            "Candidate {candidate_id} is not pending"
        )));
    }
    let approved_dir = approved_skills_dir(project_dir).join(&candidate.skill_name);
    if approved_dir.exists() {
        return Err(OrquestraError::from(format!(
            "Approved skill already exists: {}",
            approved_dir.display()
        )));
    }
    std::fs::create_dir_all(approved_skills_dir(project_dir))?;
    std::fs::rename(&source_dir, &approved_dir)?;
    candidate.status = BrainCandidateStatus::Approved;
    candidate.path = approved_dir;
    write_candidate_metadata(&candidate)?;
    Ok(candidate)
}

pub fn reject_candidate(
    project_dir: &Path,
    candidate_id: &str,
) -> Result<BrainCandidate, OrquestraError> {
    validate_candidate_id(candidate_id)?;
    let mut candidate = read_candidate_metadata(&pending_dir(project_dir).join(candidate_id))?;
    if candidate.status != BrainCandidateStatus::Pending {
        return Err(OrquestraError::from(format!(
            "Candidate {candidate_id} is not pending"
        )));
    }
    candidate.status = BrainCandidateStatus::Rejected;
    write_candidate_metadata(&candidate)?;
    Ok(candidate)
}

pub fn external_discovery_disabled() -> OrquestraError {
    OrquestraError::from(
        "External BRAIN discovery is disabled by policy. Use local adaptation or approve a reviewed candidate.",
    )
}

fn pending_dir(project_dir: &Path) -> PathBuf {
    project_dir
        .join(".orquestra")
        .join("skills")
        .join("_pending")
}

fn approved_skills_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join("skills")
}

fn write_candidate_metadata(candidate: &BrainCandidate) -> Result<(), OrquestraError> {
    let json = serde_json::to_string_pretty(candidate)
        .map_err(|error| OrquestraError::from(format!("Cannot serialize candidate: {error}")))?;
    std::fs::write(candidate.path.join("PROVENANCE.json"), json)?;
    Ok(())
}

fn read_candidate_metadata(path: &Path) -> Result<BrainCandidate, OrquestraError> {
    let content = std::fs::read_to_string(path.join("PROVENANCE.json")).map_err(|error| {
        OrquestraError::from(format!(
            "Cannot read candidate metadata at {}: {error}",
            path.display()
        ))
    })?;
    let mut candidate: BrainCandidate = serde_json::from_str(&content)
        .map_err(|error| OrquestraError::from(format!("Invalid candidate metadata: {error}")))?;
    candidate.path = path.to_path_buf();
    Ok(candidate)
}

fn sanitize_identifier(raw: &str) -> Result<String, OrquestraError> {
    let value = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    validate_candidate_id(&value)?;
    Ok(value)
}

fn validate_candidate_id(candidate_id: &str) -> Result<(), OrquestraError> {
    if candidate_id.is_empty()
        || candidate_id == "."
        || candidate_id == ".."
        || candidate_id.contains(['/', '\\'])
        || !candidate_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(OrquestraError::from(format!(
            "Invalid BRAIN candidate id: {candidate_id}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Provenance, SkillStatus, TrustLevel};
    use std::collections::HashMap;

    fn source_skill(path: PathBuf) -> SkillInfo {
        SkillInfo {
            id: "source-skill".to_string(),
            name: "source-skill".to_string(),
            description: "Source".to_string(),
            version: None,
            scope: "global".to_string(),
            source_path: path,
            hash: "sha256:test".to_string(),
            trust: TrustLevel::UserGlobal,
            status: SkillStatus::Active,
            capabilities: vec!["rust".to_string()],
            metadata: HashMap::new(),
            provenance: Provenance::Local,
            inspected_at: Utc::now(),
        }
    }

    fn ticket() -> Ticket {
        Ticket {
            id: "T1".to_string(),
            title: "Build".to_string(),
            objective: "Build the local harness".to_string(),
            acceptance_criteria: vec!["done".to_string()],
            blocked_by: vec![],
            preferred_capabilities: vec!["rust".to_string()],
            assigned_skill: None,
            model_policy: None,
            model_recommendation: None,
            verification: Default::default(),
        }
    }

    #[test]
    fn local_adaptation_stays_project_local_and_pending() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_dir = dir.path().join("global-source");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, "# Source").expect("skill file");

        let candidate =
            adapt_local_skill(dir.path(), &ticket(), &source_skill(skill_file)).unwrap();

        assert_eq!(candidate.status, BrainCandidateStatus::Pending);
        assert!(candidate.path.starts_with(dir.path().join(".orquestra")));
        assert!(candidate.path.join("SKILL.md").exists());
        assert!(candidate.path.join("PROVENANCE.json").exists());
    }

    #[test]
    fn candidate_id_rejects_traversal() {
        let error = inspect_candidate(Path::new("."), "../escape").unwrap_err();
        assert!(error.to_string().contains("Invalid BRAIN candidate id"));
    }

    #[test]
    fn reject_candidate_ignores_tampered_metadata_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_dir = dir.path().join("global-source");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, "# Source").expect("skill file");
        let candidate =
            adapt_local_skill(dir.path(), &ticket(), &source_skill(skill_file)).unwrap();
        let escape_dir = dir.path().join("escape");
        std::fs::create_dir_all(&escape_dir).expect("escape dir");
        let metadata_path = candidate.path.join("PROVENANCE.json");
        let mut metadata: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
        metadata["path"] = serde_json::json!(escape_dir);
        std::fs::write(
            &metadata_path,
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .expect("tamper metadata");

        reject_candidate(dir.path(), &candidate.id).expect("reject candidate");

        assert!(!escape_dir.join("PROVENANCE.json").exists());
        assert!(candidate.path.join("PROVENANCE.json").exists());
    }

    #[test]
    fn approve_candidate_rejects_tampered_skill_name_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let skill_dir = dir.path().join("global-source");
        std::fs::create_dir_all(&skill_dir).expect("skill dir");
        let skill_file = skill_dir.join("SKILL.md");
        std::fs::write(&skill_file, "# Source").expect("skill file");
        let candidate =
            adapt_local_skill(dir.path(), &ticket(), &source_skill(skill_file)).unwrap();
        let metadata_path = candidate.path.join("PROVENANCE.json");
        let mut metadata: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&metadata_path).unwrap()).unwrap();
        metadata["skillName"] = serde_json::json!("../escape");
        std::fs::write(
            &metadata_path,
            serde_json::to_string_pretty(&metadata).unwrap(),
        )
        .expect("tamper metadata");

        let error = approve_candidate(dir.path(), &candidate.id)
            .expect_err("unsafe skill name must reject approval");

        assert!(error.to_string().contains("Invalid"));
    }
}
