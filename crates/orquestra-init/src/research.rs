use std::path::Path;

use crate::error::InitError;
use crate::state::RankedSource;
use crate::storage;
use orquestra_core::config::InitConfig;
use orquestra_core::security::{host_resolves_to_non_public_ip, public_http_url_host};
use url::{Host, Url};

const MIN_REGISTRABLE_DOMAINS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchStatus {
    Pending,
    InProgress,
    Completed,
    Validated,
    Failed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchTopic {
    pub id: String,
    pub topic: String,
    pub query: String,
    pub status: ResearchStatus,
    pub sources: Vec<RankedSource>,
    pub brief_issued_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub loops: u32,
    pub contradictions: Vec<String>,
    pub average_score: Option<f32>,
}

pub fn generate_brief(
    project_dir: &Path,
    session_id: &str,
    topic: &str,
) -> Result<ResearchTopic, InitError> {
    let today = crate::today_date();
    let query = format!("{} {today}", topic.trim());
    let id = format!(
        "research-{}-{}",
        sanitize_topic(topic),
        uuid::Uuid::new_v4()
            .to_string()
            .chars()
            .take(8)
            .collect::<String>()
    );

    let brief = ResearchTopic {
        id: id.clone(),
        topic: topic.to_string(),
        query,
        status: ResearchStatus::Pending,
        sources: Vec::new(),
        brief_issued_at: Some(chrono::Utc::now()),
        completed_at: None,
        loops: 0,
        contradictions: Vec::new(),
        average_score: None,
    };

    save_research_topic(project_dir, session_id, &brief)?;

    Ok(brief)
}

pub fn store_results(
    project_dir: &Path,
    session_id: &str,
    topic_id: &str,
    sources: Vec<RankedSource>,
) -> Result<ResearchTopic, InitError> {
    store_results_with_config(
        project_dir,
        session_id,
        topic_id,
        sources,
        &InitConfig::default(),
    )
}

pub fn store_results_with_config(
    project_dir: &Path,
    session_id: &str,
    topic_id: &str,
    sources: Vec<RankedSource>,
    config: &InitConfig,
) -> Result<ResearchTopic, InitError> {
    let mut brief = load_research_topic(project_dir, session_id, topic_id)?;

    let mut seen_urls: std::collections::HashSet<String> = brief
        .sources
        .iter()
        .map(|source| canonical_url_key(&source.url))
        .collect();
    for mut source in sources {
        source.claims = source
            .claims
            .into_iter()
            .map(|claim| normalize_claim(&claim))
            .filter(|claim| !claim.is_empty())
            .collect();
        if seen_urls.insert(canonical_url_key(&source.url)) {
            brief.sources.push(source);
        }
    }
    for source in &mut brief.sources {
        source.authority = authority_for_url(&source.url);
    }
    apply_claim_agreement(&mut brief.sources, config);
    brief.status = ResearchStatus::Completed;
    brief.completed_at = Some(chrono::Utc::now());
    brief.average_score = average_score(&brief.sources);

    save_research_topic(project_dir, session_id, &brief)?;

    Ok(brief)
}

pub fn validate(
    project_dir: &Path,
    session_id: &str,
    topic_id: &str,
) -> Result<ResearchTopic, InitError> {
    validate_with_config(project_dir, session_id, topic_id, &InitConfig::default())
}

pub fn validate_with_config(
    project_dir: &Path,
    session_id: &str,
    topic_id: &str,
    config: &InitConfig,
) -> Result<ResearchTopic, InitError> {
    let mut brief = load_research_topic(project_dir, session_id, topic_id)?;

    if brief.status != ResearchStatus::Completed {
        return Err(InitError::Research(format!(
            "Cannot validate topic {}: status is {:?}, expected Completed",
            topic_id, brief.status
        )));
    }

    let assessment = assess_sources(&brief.sources, config);
    let contradictions = detect_contradictions(&assessment.counted_sources);
    brief.contradictions = contradictions;
    brief.average_score = Some(assessment.average_score);

    let mut reasons = assessment.reasons(config);

    if reasons.is_empty() && brief.contradictions.is_empty() {
        brief.status = ResearchStatus::Validated;
    } else {
        if !brief.contradictions.is_empty() {
            reasons.push(format!("{} contradictions", brief.contradictions.len()));
        }
        brief.status = ResearchStatus::Failed;
        return Err(InitError::Research(format!(
            "Validation failed: {}",
            reasons.join("; ")
        )));
    }

    save_research_topic(project_dir, session_id, &brief)?;
    Ok(brief)
}

pub fn load_research_topic(
    project_dir: &Path,
    session_id: &str,
    topic_id: &str,
) -> Result<ResearchTopic, InitError> {
    storage::validate_init_id(session_id)?;
    let path = research_topic_file(project_dir, session_id, topic_id)?;
    if !path.exists() {
        return Err(InitError::Research(format!(
            "Research topic not found: {topic_id}"
        )));
    }
    storage::read_json(&path)
}

pub fn save_research_topic(
    project_dir: &Path,
    session_id: &str,
    topic: &ResearchTopic,
) -> Result<(), InitError> {
    let path = research_topic_file(project_dir, session_id, &topic.id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(topic)?;
    storage::atomic_write(&path, json.as_bytes())?;
    Ok(())
}

pub fn list_topics(project_dir: &Path, session_id: &str) -> Result<Vec<ResearchTopic>, InitError> {
    storage::validate_init_id(session_id)?;
    let dir = research_dir(project_dir, session_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut topics: Vec<ResearchTopic> = std::fs::read_dir(&dir)?
        .filter_map(|entry| entry.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .filter_map(|e| storage::read_json::<ResearchTopic>(&e.path()).ok())
        .collect();
    topics.sort_by_key(|a| a.brief_issued_at);
    Ok(topics)
}

fn research_dir(project_dir: &Path, session_id: &str) -> std::path::PathBuf {
    project_dir
        .join(".orquestra")
        .join("init")
        .join(session_id)
        .join("research")
}

fn research_topic_file(
    project_dir: &Path,
    session_id: &str,
    topic_id: &str,
) -> Result<std::path::PathBuf, InitError> {
    if topic_id.contains('/') || topic_id.contains('\\') {
        return Err(InitError::Research(format!(
            "Invalid research topic ID: {topic_id}"
        )));
    }
    Ok(research_dir(project_dir, session_id).join(format!("{topic_id}.json")))
}

fn average_score(sources: &[RankedSource]) -> Option<f32> {
    if sources.is_empty() {
        return None;
    }
    let sum: f32 = sources.iter().map(|s| s.score).sum();
    Some(sum / sources.len() as f32)
}

#[derive(Debug)]
pub(crate) struct SourceAssessment {
    pub counted_sources: Vec<RankedSource>,
    pub average_score: f32,
    pub registrable_domain_count: usize,
    pub has_primary_source: bool,
    pub confirmed_claim_count: usize,
}

impl SourceAssessment {
    pub fn reasons(&self, config: &InitConfig) -> Vec<String> {
        let mut reasons = Vec::new();
        let required_sources = config.research.min_sources_per_topic;
        if self.counted_sources.len() < required_sources {
            reasons.push(format!(
                "only {} valid unique URLs, need {}",
                self.counted_sources.len(),
                required_sources
            ));
        }
        if self.registrable_domain_count < MIN_REGISTRABLE_DOMAINS {
            reasons.push(format!(
                "only {} registrable domains, need {}",
                self.registrable_domain_count, MIN_REGISTRABLE_DOMAINS
            ));
        }
        if config.require_primary_source && !self.has_primary_source {
            reasons.push("no primary or official source".to_string());
        }
        if self.confirmed_claim_count == 0 {
            reasons.push(format!(
                "no claim corroborated by {} independent sources",
                config.research.min_agreement_for_confirmed
            ));
        }
        if self.average_score < config.research.min_reliability_score {
            reasons.push(format!(
                "average score {:.2} < {:.2}",
                self.average_score, config.research.min_reliability_score
            ));
        }
        reasons
    }
}

pub(crate) fn assess_sources(sources: &[RankedSource], config: &InitConfig) -> SourceAssessment {
    let mut seen_urls = std::collections::HashSet::new();
    let mut registrable_domains = std::collections::HashSet::new();
    let mut counted_sources = Vec::new();
    let mut has_primary_source = false;

    for source in sources {
        let Some((canonical_url, host, registrable_domain)) = valid_source_identity(&source.url)
        else {
            continue;
        };
        if !seen_urls.insert(canonical_url) {
            continue;
        }

        registrable_domains.insert(registrable_domain);
        has_primary_source |= is_primary_or_official_host(&host);
        counted_sources.push(source.clone());
    }

    let confirmed_claim_count = apply_claim_agreement(&mut counted_sources, config);
    let average_score = average_score(&counted_sources).unwrap_or(0.0);
    SourceAssessment {
        counted_sources,
        average_score,
        registrable_domain_count: registrable_domains.len(),
        has_primary_source,
        confirmed_claim_count,
    }
}

fn apply_claim_agreement(sources: &mut [RankedSource], config: &InitConfig) -> usize {
    let mut claim_domains: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();

    for source in sources.iter_mut() {
        source.claims = source
            .claims
            .iter()
            .map(|claim| normalize_claim(claim))
            .filter(|claim| !claim.is_empty())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let Some((_, _, domain)) = valid_source_identity(&source.url) else {
            continue;
        };
        for claim in &source.claims {
            claim_domains
                .entry(claim.clone())
                .or_default()
                .insert(domain.clone());
        }
    }

    let required = config.research.min_agreement_for_confirmed.max(1);
    let confirmed_claims: std::collections::HashSet<String> = claim_domains
        .into_iter()
        .filter_map(|(claim, domains)| (domains.len() >= required).then_some(claim))
        .collect();

    for source in sources {
        source.agreement = if source.claims.is_empty() {
            0.0
        } else {
            let confirmed = source
                .claims
                .iter()
                .filter(|claim| confirmed_claims.contains(*claim))
                .count();
            confirmed as f32 / source.claims.len() as f32
        };
        source.compute_score();
    }

    confirmed_claims.len()
}

pub fn normalize_claim(claim: &str) -> String {
    claim
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn valid_source_identity(raw_url: &str) -> Option<(String, String, String)> {
    let mut url = Url::parse(raw_url).ok()?;
    let host = public_http_url_host(raw_url)?;
    if host_resolves_to_non_public_ip(&host) {
        return None;
    }
    let registrable_domain = registrable_domain(&host)?;
    url.set_fragment(None);
    let canonical_url = url.to_string();
    Some((canonical_url, host, registrable_domain))
}

fn canonical_url_key(raw_url: &str) -> String {
    let Ok(mut url) = Url::parse(raw_url) else {
        return raw_url.trim().to_string();
    };
    url.set_fragment(None);
    url.to_string()
}

fn registrable_domain(host: &str) -> Option<String> {
    let suffix = psl::suffix(host.as_bytes())?;
    if !suffix.is_known() || suffix.typ().is_none() {
        return None;
    }
    let domain = psl::domain(host.as_bytes())?;
    std::str::from_utf8(domain.as_bytes())
        .ok()
        .map(|domain| domain.trim_end_matches('.').to_string())
}

fn is_primary_or_official_host(host: &str) -> bool {
    PRIMARY_OR_OFFICIAL_DOMAINS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
        || host.ends_with(".gov")
        || host.ends_with(".gov.br")
}

pub fn authority_for_url(raw_url: &str) -> f32 {
    let Some(host) = Url::parse(raw_url).ok().and_then(|url| match url.host()? {
        Host::Domain(domain) => Some(domain.to_ascii_lowercase()),
        Host::Ipv4(_) | Host::Ipv6(_) => None,
    }) else {
        return 0.0;
    };

    if is_primary_or_official_host(&host) {
        1.0
    } else if TRUSTED_SECONDARY_DOMAINS
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
    {
        0.9
    } else {
        0.6
    }
}

const PRIMARY_OR_OFFICIAL_DOMAINS: &[&str] = &[
    "acm.org",
    "amazon.com",
    "arxiv.org",
    "cloud.google.com",
    "developer.mozilla.org",
    "docs.docker.com",
    "docs.github.com",
    "docs.rs",
    "expressjs.com",
    "ietf.org",
    "kubernetes.io",
    "learn.microsoft.com",
    "nodejs.org",
    "react.dev",
    "rust-lang.org",
    "w3.org",
];

const TRUSTED_SECONDARY_DOMAINS: &[&str] = &["github.com", "stackoverflow.com", "wikipedia.org"];

pub(crate) fn detect_contradictions(sources: &[RankedSource]) -> Vec<String> {
    let mut contradictions = Vec::new();

    if sources.len() < 2 {
        return contradictions;
    }

    for i in 0..sources.len() {
        for j in (i + 1)..sources.len() {
            let a = &sources[i];
            let b = &sources[j];

            if a.authority < 0.6 || b.authority < 0.6 {
                continue;
            }

            let set_a: std::collections::HashSet<&str> =
                a.claims.iter().map(|s| s.as_str()).collect();
            let set_b: std::collections::HashSet<&str> =
                b.claims.iter().map(|s| s.as_str()).collect();
            let disagreement = a.agreement < 0.5 || b.agreement < 0.5;
            let mismatched_claims = set_a != set_b;

            if disagreement && mismatched_claims {
                contradictions.push(format!(
                    "{} contradicts {} on topic (agreement={}, sources differ in claims)",
                    a.title,
                    b.title,
                    a.agreement.min(b.agreement)
                ));
            }
        }
    }

    contradictions
}

pub fn generate_query(topic: &str, host: &str) -> String {
    let today = crate::today_date();
    format!("{topic} best practices {today} {host}")
}

fn sanitize_topic(topic: &str) -> String {
    topic
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
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use orquestra_core::config::InitConfig;

    fn tmp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn sample_source(
        url: &str,
        title: &str,
        auth: f32,
        rec: f32,
        agree: f32,
        claims: Vec<&str>,
    ) -> RankedSource {
        RankedSource {
            url: url.to_string(),
            title: title.to_string(),
            authority: auth,
            recency: rec,
            relevance: 1.0,
            agreement: agree,
            score: 0.0,
            claims: claims.into_iter().map(String::from).collect(),
            snippet: None,
            fetched_at: Utc::now(),
        }
    }

    #[test]
    fn generate_brief_creates_topic() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief =
            generate_brief(dir.path(), sid, "Node.js testing 2026").expect("generate brief");
        assert_eq!(brief.status, ResearchStatus::Pending);
        assert!(brief.query.contains("2026"));
        assert!(brief.brief_issued_at.is_some());
    }

    #[test]
    fn store_results_scores_sources() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "test topic").expect("generate");
        let sources = vec![
            sample_source("https://a.com", "Source A", 8.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://b.com", "Source B", 7.0, 0.9, 1.0, vec!["same"]),
            sample_source("https://c.com", "Source C", 6.0, 0.8, 0.9, vec!["same"]),
            sample_source("https://d.com", "Source D", 9.0, 0.9, 1.0, vec!["same"]),
        ];

        let completed = store_results(dir.path(), sid, &brief.id, sources).expect("store results");
        assert_eq!(completed.status, ResearchStatus::Completed);
        assert!(completed.completed_at.is_some());
        assert!(completed.average_score.is_some());
        assert!(completed.sources.iter().all(|s| s.score > 0.0));
    }

    #[test]
    fn store_results_ignores_spoofed_json_authority() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "spoofed authority").expect("brief");
        let mut config = InitConfig::default();
        config.research.min_agreement_for_confirmed = 1;

        let completed = store_results_with_config(
            dir.path(),
            &state.id,
            &brief.id,
            vec![sample_source(
                "https://evil.example.com/payload",
                "Spoofed nodejs.org",
                1.0,
                1.0,
                1.0,
                vec!["canonical claim"],
            )],
            &config,
        )
        .expect("store");

        assert!((completed.sources[0].authority - 0.6).abs() < f32::EPSILON);
        assert!((completed.sources[0].score - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn store_results_accumulates_and_deduplicates_across_research_calls() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "accumulation").expect("brief");

        store_results(
            dir.path(),
            &state.id,
            &brief.id,
            vec![
                sample_source("https://example.com/a", "A", 1.0, 1.0, 1.0, vec!["same"]),
                sample_source("https://example.net/b", "B", 1.0, 1.0, 1.0, vec!["same"]),
            ],
        )
        .expect("first batch");
        let completed = store_results(
            dir.path(),
            &state.id,
            &brief.id,
            vec![
                sample_source(
                    "https://example.com/a",
                    "Duplicate A",
                    1.0,
                    1.0,
                    1.0,
                    vec!["same"],
                ),
                sample_source("https://example.org/c", "C", 1.0, 1.0, 1.0, vec!["same"]),
            ],
        )
        .expect("second batch");

        assert_eq!(completed.sources.len(), 3);
        assert_eq!(
            completed
                .sources
                .iter()
                .filter(|source| source.url == "https://example.com/a")
                .count(),
            1
        );
    }

    #[test]
    fn store_results_confirms_claims_only_across_independent_domains() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "agreement").expect("brief");
        let mut config = InitConfig::default();
        config.research.min_agreement_for_confirmed = 2;

        let first = store_results_with_config(
            dir.path(),
            &state.id,
            &brief.id,
            vec![
                sample_source(
                    "https://nodejs.org/a",
                    "A",
                    1.0,
                    1.0,
                    1.0,
                    vec!["Use async I/O"],
                ),
                sample_source(
                    "https://docs.nodejs.org/b",
                    "B",
                    1.0,
                    1.0,
                    1.0,
                    vec!["  use   ASYNC i/o  "],
                ),
            ],
            &config,
        )
        .expect("same-domain batch");
        assert!(first.sources.iter().all(|source| source.agreement == 0.0));

        let completed = store_results_with_config(
            dir.path(),
            &state.id,
            &brief.id,
            vec![sample_source(
                "https://example.net/c",
                "C",
                1.0,
                1.0,
                1.0,
                vec!["use async i/o"],
            )],
            &config,
        )
        .expect("independent corroboration");
        assert!(
            completed
                .sources
                .iter()
                .all(|source| source.agreement == 1.0)
        );
        assert!(
            completed
                .sources
                .iter()
                .all(|source| source.claims == vec!["use async i/o"])
        );
    }

    #[test]
    fn validate_requires_five_sources_by_default() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "test").expect("generate");
        let sources = vec![sample_source(
            "https://a.com",
            "A",
            9.0,
            1.0,
            1.0,
            vec!["c1"],
        )];

        store_results(dir.path(), sid, &brief.id, sources).expect("store");

        let err = validate_with_config(dir.path(), sid, &brief.id, &InitConfig::default())
            .expect_err("should fail with only 1 source");
        assert!(err.to_string().contains("only"));
    }

    #[test]
    fn validate_passes_with_five_diverse_reliable_sources() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "test topic").expect("generate");
        let sources = vec![
            sample_source(
                "https://nodejs.org/api/http.html",
                "A",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://rust-lang.org/learn",
                "B",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://docs.rs/http/latest/http/",
                "C",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://ietf.org/standards/",
                "D",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source("https://w3.org/TR/fetch/", "E", 1.0, 1.0, 1.0, vec!["same"]),
        ];

        store_results(dir.path(), sid, &brief.id, sources).expect("store");
        let validated = validate_with_config(dir.path(), sid, &brief.id, &InitConfig::default())
            .expect("validate should pass");
        assert_eq!(validated.status, ResearchStatus::Validated);
    }

    #[test]
    fn validate_fails_on_contradiction() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "test").expect("generate");
        let sources = vec![
            sample_source(
                "https://nodejs.org/a",
                "A",
                1.0,
                1.0,
                0.3,
                vec!["alpine is best"],
            ),
            sample_source("https://b.com/b", "B", 1.0, 1.0, 0.9, vec!["slim is best"]),
            sample_source(
                "https://c.net/c",
                "C",
                1.0,
                1.0,
                0.9,
                vec!["distroless is best"],
            ),
            sample_source("https://d.org/d", "D", 1.0, 1.0, 1.0, vec!["slim is best"]),
            sample_source("https://e.dev/e", "E", 1.0, 1.0, 1.0, vec!["slim is best"]),
        ];

        store_results(dir.path(), sid, &brief.id, sources).expect("store");
        let err = validate_with_config(dir.path(), sid, &brief.id, &InitConfig::default())
            .expect_err("should fail on contradiction");
        assert!(err.to_string().contains("contradiction"));
    }

    #[test]
    fn generate_query_includes_date() {
        let query = generate_query("Jest testing", "opencode");
        assert!(query.contains("2026"));
        assert!(query.contains("Jest testing"));
    }

    #[test]
    fn list_topics_returns_saved() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        generate_brief(dir.path(), sid, "topic 1").expect("t1");
        generate_brief(dir.path(), sid, "topic 2").expect("t2");

        let topics = list_topics(dir.path(), sid).expect("list");
        assert_eq!(topics.len(), 2);
    }

    #[test]
    fn validate_rejects_not_completed() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "test").expect("generate");
        let err = validate_with_config(dir.path(), sid, &brief.id, &InitConfig::default())
            .expect_err("should fail, not completed yet");
        assert!(err.to_string().contains("Pending"));
    }

    #[test]
    fn sanitize_topic_creates_valid_id() {
        let result = sanitize_topic("Node.js Testing 2026!");
        assert!(!result.contains('!'));
        assert!(!result.contains('.'));
        assert_eq!(result, "node-js-testing-2026");
    }

    #[test]
    fn avg_score_none_on_empty() {
        assert!(average_score(&[]).is_none());
    }

    #[test]
    fn avg_score_single() {
        let mut s = sample_source("https://x.com", "X", 8.0, 1.0, 1.0, vec!["c"]);
        s.compute_score();
        let avg = average_score(&[s]);
        assert!(avg.is_some());
        assert!((avg.unwrap() - 0.8).abs() < 0.01);
    }

    #[test]
    fn research_persistence_roundtrip() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "test roundtrip").expect("generate");
        let loaded = load_research_topic(dir.path(), sid, &brief.id).expect("load");
        assert_eq!(loaded.id, brief.id);
        assert_eq!(loaded.topic, "test roundtrip");
    }

    #[test]
    fn source_score_computed_on_store() {
        let dir = tmp_dir();
        crate::create_init_session(dir.path(), "opencode", Some("test")).expect("create session");
        let sessions = storage::list_sessions(dir.path()).expect("list");
        let sid = &sessions[0];

        let brief = generate_brief(dir.path(), sid, "score test").expect("generate");
        let sources = vec![sample_source(
            "https://x.com",
            "X",
            8.0,
            0.9,
            1.0,
            vec!["a"],
        )];
        let mut config = InitConfig::default();
        config.research.min_agreement_for_confirmed = 1;
        let completed =
            store_results_with_config(dir.path(), sid, &brief.id, sources, &config).expect("store");
        assert!((completed.sources[0].score - 0.54).abs() < 0.01);
    }

    #[test]
    fn validate_uses_configured_source_and_reliability_policy() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "configurable policy").expect("brief");
        let sources = vec![
            sample_source(
                "https://github.com/example/one",
                "A",
                0.9,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://stackoverflow.com/questions/1/two",
                "B",
                0.9,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://wikipedia.org/wiki/Three",
                "C",
                0.9,
                1.0,
                1.0,
                vec!["same"],
            ),
        ];
        store_results(dir.path(), &state.id, &brief.id, sources).expect("store");

        let mut config = InitConfig::default();
        config.research.min_sources_per_topic = 3;
        config.research.min_reliability_score = 0.85;
        config.require_primary_source = false;

        let validated = validate_with_config(dir.path(), &state.id, &brief.id, &config)
            .expect("configured policy");
        assert_eq!(validated.status, ResearchStatus::Validated);
    }

    #[test]
    fn validate_rejects_duplicate_or_invalid_urls() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "unique URLs").expect("brief");
        let sources = vec![
            sample_source(
                "https://nodejs.org/api/http.html",
                "Primary",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source("https://example.com/a", "A", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://example.net/b", "B", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://example.org/c", "C", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source(
                "https://example.org/c",
                "Duplicate",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source("not a URL", "Invalid", 1.0, 1.0, 1.0, vec!["same"]),
        ];
        store_results(dir.path(), &state.id, &brief.id, sources).expect("store");

        let err = validate_with_config(dir.path(), &state.id, &brief.id, &InitConfig::default())
            .expect_err("duplicates and invalid URLs do not count");
        assert!(err.to_string().contains("valid unique URLs"));
    }

    #[test]
    fn validate_requires_three_registrable_domains() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "domain diversity").expect("brief");
        let sources = vec![
            sample_source(
                "https://nodejs.org/api/http.html",
                "Primary",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://docs.nodejs.org/a",
                "A",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source(
                "https://blog.nodejs.org/b",
                "B",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
            sample_source("https://example.com/c", "C", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source(
                "https://docs.example.com/d",
                "D",
                1.0,
                1.0,
                1.0,
                vec!["same"],
            ),
        ];
        store_results(dir.path(), &state.id, &brief.id, sources).expect("store");

        let err = validate_with_config(dir.path(), &state.id, &brief.id, &InitConfig::default())
            .expect_err("subdomains must not inflate diversity");
        assert!(err.to_string().contains("registrable domains"));
    }

    #[test]
    fn registrable_domain_uses_public_suffix_list_for_co_za() {
        assert_eq!(
            registrable_domain("docs.example.co.za").as_deref(),
            Some("example.co.za")
        );
    }

    #[test]
    fn registrable_domain_rejects_unknown_or_reserved_suffixes() {
        for host in ["example.invalid", "service.local", "service.internal"] {
            assert_eq!(registrable_domain(host), None, "{host} must be rejected");
        }
    }

    #[test]
    fn public_validate_keeps_legacy_signature() {
        let _validate: fn(&std::path::Path, &str, &str) -> Result<ResearchTopic, InitError> =
            validate;
    }

    #[test]
    fn validate_requires_primary_source_when_configured() {
        let dir = tmp_dir();
        let state =
            crate::create_init_session(dir.path(), "opencode", Some("test")).expect("session");
        let brief = generate_brief(dir.path(), &state.id, "primary source").expect("brief");
        let sources = vec![
            sample_source("https://example.com/a", "A", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://example.net/b", "B", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://example.org/c", "C", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://community.dev/d", "D", 1.0, 1.0, 1.0, vec!["same"]),
            sample_source("https://articles.test/e", "E", 1.0, 1.0, 1.0, vec!["same"]),
        ];
        store_results(dir.path(), &state.id, &brief.id, sources).expect("store");

        let err = validate_with_config(dir.path(), &state.id, &brief.id, &InitConfig::default())
            .expect_err("primary source is mandatory");
        assert!(err.to_string().contains("primary or official source"));
    }
}
