use crate::{RuntimeError, storage};
use orquestra_config::{OrquestraConfig, profile};
use orquestra_core::security::redact_secrets;
use orquestra_plan::VerificationPolicy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub kind: String,
    pub description: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub session_id: String,
    pub ticket_id: String,
    #[serde(default)]
    pub dispatch_attempt_id: Option<String>,
    pub skill_name: String,
    pub score: f64,
    pub summary: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerificationOutcome {
    pub ticket_id: String,
    pub passed: bool,
    pub score: f64,
    pub minimum_score: f64,
    pub missing_evidence: Vec<String>,
    pub reasons: Vec<String>,
}

pub fn verification_dir(project_dir: &Path, session_id: &str) -> Result<PathBuf, RuntimeError> {
    storage::validate_session_id(session_id)?;
    Ok(project_dir
        .join(".orquestra")
        .join("verification")
        .join(session_id))
}

pub fn verification_report_file(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
) -> Result<PathBuf, RuntimeError> {
    storage::validate_ticket_id(ticket_id)?;
    Ok(verification_dir(project_dir, session_id)?.join(format!("{ticket_id}.json")))
}

pub fn save_verification_report(
    project_dir: &Path,
    report: &VerificationReport,
) -> Result<(), RuntimeError> {
    storage::validate_session_id(&report.session_id)?;
    storage::validate_ticket_id(&report.ticket_id)?;
    validate_report_shape(report)?;
    let mut redacted = report.clone();
    redacted.summary = redact_secrets(&redacted.summary);
    for evidence in &mut redacted.evidence {
        evidence.description = redact_secrets(&evidence.description);
        evidence.path = evidence.path.as_deref().map(redact_secrets);
    }
    storage::atomic_write_json(
        &verification_report_file(project_dir, &report.session_id, &report.ticket_id)?,
        &redacted,
    )
}

pub fn load_verification_report(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
) -> Result<VerificationReport, RuntimeError> {
    storage::read_json(&verification_report_file(
        project_dir,
        session_id,
        ticket_id,
    )?)
}

pub fn evaluate_report(
    policy: &VerificationPolicy,
    report: &VerificationReport,
) -> Result<VerificationOutcome, RuntimeError> {
    validate_score(policy.minimum_score)?;
    validate_report_shape(report)?;

    let present = report
        .evidence
        .iter()
        .map(|item| item.kind.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let missing_evidence = policy
        .required_evidence
        .iter()
        .filter(|kind| !present.contains(&kind.to_ascii_lowercase()))
        .cloned()
        .collect::<Vec<_>>();

    let mut reasons = Vec::new();
    if report.evidence.is_empty() {
        reasons.push("verification report must include evidence".to_string());
    }
    if report.score < policy.minimum_score {
        reasons.push(format!(
            "score {} is below minimum {}",
            report.score, policy.minimum_score
        ));
    }
    if !missing_evidence.is_empty() {
        reasons.push(format!(
            "missing required evidence: {}",
            missing_evidence.join(", ")
        ));
    }

    Ok(VerificationOutcome {
        ticket_id: report.ticket_id.clone(),
        passed: reasons.is_empty(),
        score: report.score,
        minimum_score: policy.minimum_score,
        missing_evidence,
        reasons,
    })
}

pub fn verify_with_profile(
    project_dir: &Path,
    ticket_dir: &Path,
    profile_name: &str,
) -> Result<orquestra_config::ProfileResult, RuntimeError> {
    let config_path = project_dir.join(".orquestra").join("config.toml");
    let config = OrquestraConfig::load(&config_path)
        .map_err(|e| RuntimeError::Other(format!("failed to load config: {e}")))?;
    let profile = config.get_profile(profile_name).ok_or_else(|| {
        RuntimeError::Other(format!("verification profile '{profile_name}' not found"))
    })?;
    let result = profile::execute_profile(profile, ticket_dir)?;
    let expected = profile.expected_exit_code.unwrap_or(0);
    if result.exit_code != expected {
        return Err(RuntimeError::Other(format!(
            "profile '{}' exited with {} (expected {})",
            profile_name, result.exit_code, expected
        )));
    }
    Ok(result)
}

pub fn list_verification_reports(
    project_dir: &Path,
    session_id: &str,
) -> Result<Vec<VerificationReport>, RuntimeError> {
    let dir = verification_dir(project_dir, session_id)?;
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut reports = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if !matches!(path.extension().and_then(|e| e.to_str()), Some("json")) {
            continue;
        }
        let Some(ticket_id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if let Ok(report) = load_verification_report(project_dir, session_id, ticket_id) {
            reports.push(report);
        }
    }
    reports.sort_by(|a, b| a.ticket_id.cmp(&b.ticket_id));
    Ok(reports)
}

fn validate_score(score: f64) -> Result<(), RuntimeError> {
    if !score.is_finite() || !(0.0..=1.0).contains(&score) {
        return Err(RuntimeError::Other(format!(
            "verification score must be between 0.0 and 1.0, got {score}"
        )));
    }
    Ok(())
}

fn validate_report_shape(report: &VerificationReport) -> Result<(), RuntimeError> {
    validate_score(report.score)?;
    if report.skill_name.trim().is_empty() {
        return Err(RuntimeError::Other(
            "verification skillName is required".to_string(),
        ));
    }
    if report.summary.trim().is_empty() {
        return Err(RuntimeError::Other(
            "verification summary is required".to_string(),
        ));
    }
    for evidence in &report.evidence {
        validate_evidence(evidence)?;
    }
    Ok(())
}

fn validate_evidence(evidence: &Evidence) -> Result<(), RuntimeError> {
    let kind = evidence.kind.trim();
    if kind.is_empty()
        || !kind
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        return Err(RuntimeError::Other(format!(
            "verification evidence kind is invalid: {}",
            evidence.kind
        )));
    }
    if evidence.description.trim().is_empty() {
        return Err(RuntimeError::Other(format!(
            "verification evidence {kind} requires a description"
        )));
    }
    if evidence
        .description
        .to_ascii_lowercase()
        .contains("assumed")
    {
        return Err(RuntimeError::Other(format!(
            "verification evidence {kind} cannot be assumption-based"
        )));
    }
    if let Some(path) = &evidence.path {
        storage::validate_relative_path(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use orquestra_plan::VerificationPolicy;

    fn report() -> VerificationReport {
        VerificationReport {
            session_id: uuid::Uuid::new_v4().to_string(),
            ticket_id: "T1".to_string(),
            dispatch_attempt_id: Some(uuid::Uuid::new_v4().to_string()),
            skill_name: "verifier".to_string(),
            score: 0.97,
            summary: "ok".to_string(),
            evidence: vec![Evidence {
                kind: "test".to_string(),
                description: "cargo test".to_string(),
                path: None,
            }],
        }
    }

    #[test]
    fn passing_report_meets_score_and_evidence() {
        let policy = VerificationPolicy {
            minimum_score: 0.95,
            required_evidence: vec!["test".to_string()],
        };

        let outcome = evaluate_report(&policy, &report()).unwrap();

        assert!(outcome.passed);
    }

    #[test]
    fn missing_evidence_fails() {
        let policy = VerificationPolicy {
            minimum_score: 0.95,
            required_evidence: vec!["test".to_string(), "review".to_string()],
        };

        let outcome = evaluate_report(&policy, &report()).unwrap();

        assert!(!outcome.passed);
        assert_eq!(outcome.missing_evidence, vec!["review".to_string()]);
    }

    #[test]
    fn saving_report_rejects_unsafe_ticket_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut report = report();
        report.ticket_id = "../escape".to_string();

        let error = save_verification_report(dir.path(), &report).unwrap_err();

        assert!(error.to_string().contains("Invalid ticket ID"));
    }

    #[test]
    fn evaluation_rejects_empty_evidence_even_with_high_score() {
        let mut report = report();
        report.evidence.clear();
        let policy = VerificationPolicy::default();

        let outcome = evaluate_report(&policy, &report).expect("evaluate report");

        assert!(!outcome.passed);
        assert!(
            outcome
                .reasons
                .iter()
                .any(|reason| reason.contains("must include evidence"))
        );
    }

    #[test]
    fn saving_report_rejects_unsafe_evidence_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut report = report();
        report.evidence[0].path = Some("../secret.txt".to_string());

        let error = save_verification_report(dir.path(), &report).unwrap_err();

        assert!(error.to_string().contains("unsafe component"));
    }

    #[test]
    fn list_verification_reports_returns_saved_reports() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session_id = uuid::Uuid::new_v4().to_string();

        let reports = list_verification_reports(dir.path(), &session_id)
            .expect("list reports for empty session");
        assert!(reports.is_empty());

        let mut report = report();
        report.session_id = session_id.clone();
        report.ticket_id = "T1".to_string();
        save_verification_report(dir.path(), &report).expect("save T1 report");

        report.ticket_id = "T2".to_string();
        save_verification_report(dir.path(), &report).expect("save T2 report");

        let reports = list_verification_reports(dir.path(), &session_id).expect("list reports");
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].ticket_id, "T1");
        assert_eq!(reports[1].ticket_id, "T2");
    }
}
