use crate::{RuntimeError, storage};
use chrono::Utc;
use orquestra_core::security::{
    host_resolves_to_non_public_ip, public_https_url_host, redact_secrets,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchSource {
    pub url: String,
    pub title: String,
    pub publisher: String,
    pub source_type: String,
    pub trust_level: String,
    pub retrieved_at: String,
    pub claim: String,
    pub supports_claim: bool,
    #[serde(default)]
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchClaim {
    pub id: String,
    pub statement: String,
    #[serde(default)]
    pub required_primary: bool,
    #[serde(default)]
    pub sources: Vec<ResearchSource>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    pub final_assessment: String,
    pub confidence: f64,
    pub used_for_decision: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchReport {
    pub session_id: Option<String>,
    pub ticket_id: String,
    pub current_date: String,
    pub generated_at: String,
    #[serde(default)]
    pub claims: Vec<ResearchClaim>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchValidation {
    pub valid: bool,
    pub ticket_id: String,
    pub claim_count: usize,
    pub validated_claims: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResearchIndex {
    session_id: Option<String>,
    ticket_id: String,
    current_date: String,
    claim_ids: Vec<String>,
    stored_at: String,
}

pub fn research_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join("research")
}

pub fn memory_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".orquestra").join("memory")
}

pub fn research_report_file(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
) -> Result<PathBuf, RuntimeError> {
    storage::validate_session_id(session_id)?;
    storage::validate_ticket_id(ticket_id)?;
    Ok(research_dir(project_dir)
        .join(session_id)
        .join(format!("{ticket_id}.json")))
}

pub fn validate_research_report(report: &ResearchReport) -> ResearchValidation {
    let mut errors = Vec::new();
    match report.session_id.as_deref() {
        Some(session_id) if storage::validate_session_id(session_id).is_ok() => {}
        _ => errors.push("sessionId must be a UUID v4 session identifier".to_string()),
    }
    if storage::validate_ticket_id(&report.ticket_id).is_err() {
        errors.push("ticketId must be a safe filename".to_string());
    }
    let current_utc_date = Utc::now().date_naive().to_string();
    if report.current_date != current_utc_date {
        errors.push(format!(
            "currentDate must equal the current UTC date {current_utc_date}"
        ));
    }
    if report.claims.is_empty() {
        errors.push("research report must contain at least one claim".to_string());
    }
    if !report.generated_at.starts_with(&report.current_date) {
        errors.push("generatedAt must start with currentDate".to_string());
    }

    let mut validated_claims = 0;
    let mut seen_claim_ids = BTreeSet::new();
    for claim in &report.claims {
        if claim.id.trim().is_empty() {
            errors.push("claim id is required".to_string());
        } else if !seen_claim_ids.insert(claim.id.clone()) {
            errors.push(format!("duplicate claim id: {}", claim.id));
        }
        if !claim.used_for_decision {
            errors.push(format!("claim {} is not marked usedForDecision", claim.id));
        }
        if !claim.confidence.is_finite() || !(0.0..=1.0).contains(&claim.confidence) {
            errors.push(format!("claim {} confidence must be 0.0..=1.0", claim.id));
        }
        if !claim.conflicts.is_empty() {
            errors.push(format!(
                "claim {} has unresolved conflicts: {}",
                claim.id,
                claim.conflicts.join("; ")
            ));
        }
        let supporting_sources = claim
            .sources
            .iter()
            .filter(|source| source.supports_claim)
            .collect::<Vec<_>>();
        if claim.final_assessment.trim().is_empty() {
            errors.push(format!("claim {} finalAssessment is required", claim.id));
        }
        if supporting_sources.len() < 2 {
            errors.push(format!(
                "claim {} requires at least 2 supporting sources",
                claim.id
            ));
        }
        if claim.required_primary
            && !supporting_sources
                .iter()
                .any(|source| source.source_type.eq_ignore_ascii_case("primary"))
        {
            errors.push(format!("claim {} requires a primary source", claim.id));
        }
        let mut source_urls = BTreeSet::new();
        let mut source_authorities = BTreeSet::new();
        for source in supporting_sources {
            if !source.retrieved_at.starts_with(&report.current_date) {
                errors.push(format!(
                    "claim {} source {} was not retrieved on currentDate {}",
                    claim.id, source.url, report.current_date
                ));
            }
            if source.title.trim().is_empty()
                || source.publisher.trim().is_empty()
                || source.claim.trim().is_empty()
                || !is_allowed_source_type(&source.source_type)
                || !is_allowed_trust_level(&source.trust_level)
                || !is_public_https_url(&source.url)
            {
                errors.push(format!("claim {} has an incomplete source", claim.id));
            }
            if !source_urls.insert(source.url.as_str()) {
                errors.push(format!(
                    "claim {} contains duplicate supporting sources",
                    claim.id
                ));
            }
            match public_https_host(&source.url) {
                Some(host) => {
                    if !check_trusted_domain(&source.url) {
                        errors.push(format!(
                            "claim {} source {} is not from a trusted domain",
                            claim.id, source.url
                        ));
                    }
                    if reject_resolved_private_ip(&host) {
                        errors.push(format!(
                            "claim {} source {} resolves to a private IP",
                            claim.id, source.url
                        ));
                    }
                    if !source_authorities.insert(host) {
                        errors.push(format!(
                            "claim {} must use supporting sources from distinct authorities",
                            claim.id
                        ));
                    }
                }
                None => errors.push(format!("claim {} has an incomplete source", claim.id)),
            }
            if source.content_hash.is_none() {
                errors.push(format!(
                    "claim {} source {} is missing contentHash",
                    claim.id, source.url
                ));
            }
        }
        let claim_valid = errors.is_empty()
            || !errors
                .iter()
                .any(|error| error.contains(&format!("claim {}", claim.id)));
        if claim_valid {
            validated_claims += 1;
        }
    }

    ResearchValidation {
        valid: errors.is_empty(),
        ticket_id: report.ticket_id.clone(),
        claim_count: report.claims.len(),
        validated_claims,
        errors,
    }
}

pub fn save_research_report(
    project_dir: &Path,
    report: &ResearchReport,
) -> Result<ResearchValidation, RuntimeError> {
    let _lock = storage::acquire_named_lock(project_dir, "research-memory")?;
    let validation = validate_research_report(report);
    if !validation.valid {
        return Err(RuntimeError::Other(validation.errors.join("; ")));
    }

    let mut redacted = report.clone();
    for claim in &mut redacted.claims {
        claim.statement = redact_secrets(&claim.statement);
        claim.final_assessment = redact_secrets(&claim.final_assessment);
        for source in &mut claim.sources {
            source.title = redact_secrets(&source.title);
            source.publisher = redact_secrets(&source.publisher);
            source.claim = redact_secrets(&source.claim);
        }
    }

    storage::atomic_write_json(
        &research_report_file(
            project_dir,
            report.session_id.as_deref().ok_or_else(|| {
                RuntimeError::Other("sessionId must be present before storing research".to_string())
            })?,
            &report.ticket_id,
        )?,
        &redacted,
    )?;
    append_memory_facts(project_dir, &redacted)?;
    write_research_index(project_dir, &redacted)?;
    Ok(validation)
}

pub fn load_research_report(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
) -> Result<ResearchReport, RuntimeError> {
    storage::read_json(&research_report_file(project_dir, session_id, ticket_id)?)
}

pub fn list_research_reports(
    project_dir: &Path,
    session_id: &str,
) -> Result<Vec<ResearchReport>, RuntimeError> {
    storage::validate_session_id(session_id)?;
    let dir = research_dir(project_dir).join(session_id);
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
        if let Ok(report) = load_research_report(project_dir, session_id, ticket_id) {
            reports.push(report);
        }
    }
    reports.sort_by(|a, b| a.ticket_id.cmp(&b.ticket_id));
    Ok(reports)
}

fn append_memory_facts(project_dir: &Path, report: &ResearchReport) -> Result<(), RuntimeError> {
    let path = memory_dir(project_dir).join("facts.jsonl");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for claim in &report.claims {
        let source_urls = claim
            .sources
            .iter()
            .filter(|source| source.supports_claim)
            .map(|source| source.url.clone())
            .collect::<Vec<_>>();
        let fact = serde_json::json!({
            "ticketId": report.ticket_id,
            "sessionId": report.session_id,
            "claimId": claim.id,
            "statement": claim.statement,
            "confidence": claim.confidence,
            "currentDate": report.current_date,
            "sourceUrls": source_urls
        });
        writeln!(file, "{}", serde_json::to_string(&fact)?)?;
    }
    Ok(())
}

fn write_research_index(project_dir: &Path, report: &ResearchReport) -> Result<(), RuntimeError> {
    let index = ResearchIndex {
        session_id: report.session_id.clone(),
        ticket_id: report.ticket_id.clone(),
        current_date: report.current_date.clone(),
        claim_ids: report.claims.iter().map(|claim| claim.id.clone()).collect(),
        stored_at: crate::iso_now(),
    };
    let path = memory_dir(project_dir).join("research-index.jsonl");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    storage::ensure_no_symlink_ancestors(&path)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{}", serde_json::to_string(&index)?)?;
    file.sync_data()?;
    Ok(())
}

fn is_public_https_url(value: &str) -> bool {
    public_https_host(value).is_some()
}

fn public_https_host(value: &str) -> Option<String> {
    public_https_url_host(value)
}

const TRUSTED_PRIMARY_DOMAINS: &[&str] = &[
    "wikipedia.org",
    "github.com",
    "docs.rs",
    "crates.io",
    "rust-lang.org",
    "npmjs.com",
    "docs.npmjs.com",
    "sigstore.dev",
    "github.community",
    "opencode.ai",
    "anthropic.com",
    "openai.com",
    "developers.google.com",
    "platform.openai.com",
    "docs.anthropic.com",
];

fn reject_resolved_private_ip(host: &str) -> bool {
    host_resolves_to_non_public_ip(host)
}

fn check_trusted_domain(url: &str) -> bool {
    let Some(host) = public_https_host(url) else {
        return false;
    };
    TRUSTED_PRIMARY_DOMAINS
        .iter()
        .any(|trusted| host == *trusted || host.ends_with(&format!(".{trusted}")))
}

fn is_allowed_source_type(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "primary" | "official" | "user" | "community" | "secondary"
    )
}

fn is_allowed_trust_level(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "high" | "medium"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(url: &str) -> ResearchSource {
        ResearchSource {
            url: url.to_string(),
            title: "Official documentation".to_string(),
            publisher: "Example".to_string(),
            source_type: "primary".to_string(),
            trust_level: "high".to_string(),
            retrieved_at: "1970-01-01T12:00:00Z".to_string(),
            claim: "Supported claim".to_string(),
            supports_claim: true,
            content_hash: Some(
                "abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
            ),
        }
    }

    #[test]
    fn research_requires_a_session_and_current_utc_date() {
        let report = ResearchReport {
            session_id: None,
            ticket_id: "T1".to_string(),
            current_date: "1970-01-01".to_string(),
            generated_at: "1970-01-01T12:00:00Z".to_string(),
            claims: vec![ResearchClaim {
                id: "C1".to_string(),
                statement: "A supported fact".to_string(),
                required_primary: true,
                sources: vec![
                    source("https://wikipedia.org/page-a"),
                    source("https://github.com/repo-b"),
                ],
                conflicts: vec![],
                final_assessment: "Validated".to_string(),
                confidence: 0.9,
                used_for_decision: true,
            }],
        };

        let validation = validate_research_report(&report);

        assert!(!validation.valid);
        assert!(
            validation
                .errors
                .iter()
                .any(|error| error.contains("sessionId"))
        );
        assert!(
            validation
                .errors
                .iter()
                .any(|error| error.contains("current UTC date"))
        );
    }

    #[test]
    fn list_research_reports_returns_empty_for_no_reports() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session_id = uuid::Uuid::new_v4().to_string();

        let reports =
            list_research_reports(dir.path(), &session_id).expect("list research reports");
        assert!(reports.is_empty());
    }

    #[test]
    fn list_research_reports_returns_saved_reports() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session_id = uuid::Uuid::new_v4().to_string();
        let report_dir = research_dir(dir.path()).join(&session_id);
        std::fs::create_dir_all(&report_dir).expect("create research session dir");

        let report = ResearchReport {
            session_id: Some(session_id.clone()),
            ticket_id: "T1".to_string(),
            current_date: "2026-07-28".to_string(),
            generated_at: "2026-07-28T12:00:00Z".to_string(),
            claims: vec![],
        };
        storage::atomic_write_json(&report_dir.join("T1.json"), &report)
            .expect("write research report");

        let report2 = ResearchReport {
            session_id: Some(session_id.clone()),
            ticket_id: "T2".to_string(),
            current_date: "2026-07-28".to_string(),
            generated_at: "2026-07-28T12:00:00Z".to_string(),
            claims: vec![],
        };
        storage::atomic_write_json(&report_dir.join("T2.json"), &report2)
            .expect("write research report");

        let reports =
            list_research_reports(dir.path(), &session_id).expect("list research reports");
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].ticket_id, "T1");
        assert_eq!(reports[1].ticket_id, "T2");
    }

    #[test]
    fn reject_resolved_private_ip_rejects_loopback() {
        assert!(reject_resolved_private_ip("127.0.0.1"));
    }

    #[test]
    fn reject_resolved_private_ip_rejects_private_range() {
        assert!(reject_resolved_private_ip("10.0.0.1"));
        assert!(reject_resolved_private_ip("192.168.1.1"));
        assert!(reject_resolved_private_ip("172.16.0.1"));
    }

    #[test]
    fn reject_resolved_private_ip_allows_public() {
        assert!(!reject_resolved_private_ip("github.com"));
        assert!(!reject_resolved_private_ip("wikipedia.org"));
    }
}
