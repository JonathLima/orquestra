use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, de};

pub const MAX_ROUNDS: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitState {
    pub id: String,
    pub host: String,
    pub idea: String,
    pub phase: InitPhase,
    pub round: u32,
    pub rounds: Vec<Round>,
    pub requirements: Requirements,
    pub contradictions: Vec<Contradiction>,
    pub plan_draft: Option<PlanDraft>,
    pub metrics: Metrics,
    pub classification: Option<Classification>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for InitState {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            host: String::new(),
            idea: String::new(),
            phase: InitPhase::Pending,
            round: 0,
            rounds: Vec::new(),
            requirements: Requirements::default(),
            contradictions: Vec::new(),
            plan_draft: None,
            metrics: Metrics::new(now),
            classification: None,
            created_at: now,
            updated_at: now,
        }
    }
}

impl InitState {
    pub fn new(id: String, host: String, idea: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            host,
            idea,
            phase: InitPhase::Pending,
            round: 0,
            rounds: Vec::new(),
            requirements: Requirements::default(),
            contradictions: Vec::new(),
            plan_draft: None,
            metrics: Metrics::new(now),
            classification: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn update_phase(&mut self, phase: InitPhase) {
        self.phase = phase;
        self.updated_at = Utc::now();
    }

    pub fn increment_round(&mut self) {
        self.round += 1;
        self.updated_at = Utc::now();
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.phase, InitPhase::Applied { .. } | InitPhase::Cancelled)
    }

    pub fn converged(&self) -> bool {
        matches!(
            self.phase,
            InitPhase::Converged { .. } | InitPhase::Applied { .. }
        )
    }

    pub fn answered_ratio(&self) -> f32 {
        let total = self.requirements.items.len();
        if total == 0 {
            return 0.0;
        }
        let answered = self
            .requirements
            .items
            .iter()
            .filter(|r| r.answered)
            .count();
        answered as f32 / total as f32
    }

    pub fn all_research_validated(&self, min_reliability_score: f32) -> bool {
        self.rounds.iter().all(|r| {
            r.research.iter().all(|e| {
                let score = e.average_score();
                score.is_some() && score.unwrap() >= min_reliability_score
            })
        })
    }

    pub fn contradictions_open(&self) -> Vec<&Contradiction> {
        self.contradictions.iter().filter(|c| !c.resolved).collect()
    }

    pub fn contradictions_open_count(&self) -> u32 {
        self.contradictions.iter().filter(|c| !c.resolved).count() as u32
    }

    pub fn record_tokens(&mut self, tokens_in: u32, tokens_out: u32) {
        self.metrics.record_round(self.round, tokens_in, tokens_out);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum InitPhase {
    Pending,
    Questioning { remaining: Vec<String> },
    Researching { topics: Vec<String> },
    Validating { sources: Vec<String> },
    Converged { draft_id: String },
    Applied { session_id: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Round {
    pub number: u32,
    pub questions: Vec<Question>,
    pub answers: Vec<Answer>,
    pub notes: Vec<String>,
    pub research: Vec<ResearchEntry>,
    pub sources_ranked: Vec<RankedSource>,
    pub convergence_delta: f32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub created_at: DateTime<Utc>,
}

impl Round {
    pub fn new(number: u32) -> Self {
        Self {
            number,
            questions: Vec::new(),
            answers: Vec::new(),
            notes: Vec::new(),
            research: Vec::new(),
            sources_ranked: Vec::new(),
            convergence_delta: 0.0,
            tokens_in: 0,
            tokens_out: 0,
            created_at: Utc::now(),
        }
    }

    pub fn answered(self) -> bool {
        self.answers.len() >= self.questions.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub id: String,
    pub text: String,
    pub category: QuestionCategory,
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionCategory {
    Problem,
    Constraint,
    Stakeholder,
    Technical,
    Security,
    Timeline,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Answer {
    pub question_id: String,
    pub text: String,
    pub confidence: AnswerConfidence,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnswerConfidence {
    Confirmed,
    Tentative,
    Override,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Requirements {
    pub items: Vec<Requirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub id: String,
    pub text: String,
    pub source: RequirementSource,
    pub answered: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequirementSource {
    User,
    Research,
    Inferred,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Contradiction {
    pub id: String,
    pub topic: String,
    pub claim_a: String,
    pub claim_b: String,
    pub sources_a: Vec<String>,
    pub sources_b: Vec<String>,
    pub detected_at: DateTime<Utc>,
    pub resolved: bool,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResearchEntry {
    pub id: String,
    pub topic: String,
    pub query: String,
    pub sources: Vec<RankedSource>,
    pub contradictions: Vec<String>,
    pub loops: u32,
    pub created_at: DateTime<Utc>,
    pub fetched_at: DateTime<Utc>,
}

impl ResearchEntry {
    pub fn average_score(&self) -> Option<f32> {
        if self.sources.is_empty() {
            return None;
        }
        let sum: f32 = self.sources.iter().map(|s| s.score).sum();
        Some(sum / self.sources.len() as f32)
    }

    pub fn has_min_sources(&self, min: usize) -> bool {
        self.sources.len() >= min
    }

    pub fn agreement_count(&self, min_score: f32) -> usize {
        self.sources.iter().filter(|s| s.score >= min_score).count()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RankedSource {
    pub url: String,
    pub title: String,
    #[serde(deserialize_with = "deserialize_legacy_score")]
    pub authority: f32,
    #[serde(deserialize_with = "deserialize_proportion")]
    pub recency: f32,
    #[serde(
        default = "default_relevance",
        deserialize_with = "deserialize_proportion"
    )]
    pub relevance: f32,
    #[serde(deserialize_with = "deserialize_proportion")]
    pub agreement: f32,
    #[serde(deserialize_with = "deserialize_legacy_score")]
    pub score: f32,
    pub claims: Vec<String>,
    #[serde(default)]
    pub snippet: Option<String>,
    pub fetched_at: DateTime<Utc>,
}

impl RankedSource {
    pub fn compute_score(&mut self) {
        self.authority = normalize_legacy_score(self.authority);
        self.recency = clamp_proportion(self.recency);
        self.relevance = clamp_proportion(self.relevance);
        self.agreement = clamp_proportion(self.agreement);
        self.score =
            (self.authority * self.recency * self.relevance * self.agreement).clamp(0.0, 1.0);
    }
}

fn default_relevance() -> f32 {
    1.0
}

fn normalize_legacy_score(value: f32) -> f32 {
    if !value.is_finite() {
        return 0.0;
    }
    let normalized = if value > 1.0 { value / 10.0 } else { value };
    normalized.clamp(0.0, 1.0)
}

fn clamp_proportion(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn deserialize_legacy_score<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    if value.is_finite() {
        Ok(normalize_legacy_score(value))
    } else {
        Err(de::Error::custom("RankedSource score must be finite"))
    }
}

fn deserialize_proportion<'de, D>(deserializer: D) -> Result<f32, D::Error>
where
    D: Deserializer<'de>,
{
    let value = f32::deserialize(deserializer)?;
    if value.is_finite() {
        Ok(clamp_proportion(value))
    } else {
        Err(de::Error::custom("RankedSource proportion must be finite"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub current_round: u32,
    pub total_tokens_in: u32,
    pub total_tokens_out: u32,
    pub per_round: Vec<RoundMetrics>,
    pub sources_consulted: u32,
    pub sources_ranked: u32,
    pub contradictions_found: u32,
    pub contradictions_resolved: u32,
    pub started_at: DateTime<Utc>,
    pub last_updated_at: DateTime<Utc>,
}

impl Metrics {
    pub fn new(started_at: DateTime<Utc>) -> Self {
        Self {
            current_round: 0,
            total_tokens_in: 0,
            total_tokens_out: 0,
            per_round: Vec::new(),
            sources_consulted: 0,
            sources_ranked: 0,
            contradictions_found: 0,
            contradictions_resolved: 0,
            started_at,
            last_updated_at: started_at,
        }
    }

    pub fn record_round(&mut self, round: u32, tokens_in: u32, tokens_out: u32) {
        self.current_round = round;
        self.total_tokens_in += tokens_in;
        self.total_tokens_out += tokens_out;

        if let Some(existing) = self
            .per_round
            .iter_mut()
            .find(|r: &&mut RoundMetrics| r.round == round)
        {
            existing.tokens_in += tokens_in;
            existing.tokens_out += tokens_out;
        } else {
            self.per_round.push(RoundMetrics {
                round,
                tokens_in,
                tokens_out,
                questions_asked: 0,
                answers_recorded: 0,
                sources_ranked: 0,
                timestamp: Utc::now(),
            });
        }

        self.last_updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RoundMetrics {
    pub round: u32,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub questions_asked: u32,
    pub answers_recorded: u32,
    pub sources_ranked: u32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Classification {
    pub intent: ArtifactIntent,
    pub scope: ArtifactScope,
    pub audience: Audience,
    pub confidence: f32,
    pub reasoning: String,
    pub classified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Hash, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactIntent {
    Design,
    Build,
    Migrate,
    Audit,
    Research,
    Operate,
    Onboard,
    Mixed,
}

impl ArtifactIntent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Design => "design",
            Self::Build => "build",
            Self::Migrate => "migrate",
            Self::Audit => "audit",
            Self::Research => "research",
            Self::Operate => "operate",
            Self::Onboard => "onboard",
            Self::Mixed => "mixed",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactScope {
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    Developer,
    Stakeholder,
    Operations,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlanDraft {
    pub id: String,
    pub title: String,
    pub tickets: Vec<DraftTicket>,
    pub waves: Vec<DraftWave>,
    pub skills_required: Vec<String>,
    pub skills_brain_required: Vec<String>,
    pub research_validated_topics: Vec<String>,
    pub contradictions_open: u32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DraftTicket {
    pub id: String,
    pub title: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub blocked_by: Vec<String>,
    pub preferred_capabilities: Vec<String>,
    pub assigned_skill: Option<String>,
    pub research_validated: bool,
    pub wave: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DraftWave {
    pub wave_number: u32,
    pub ticket_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_state_default_is_pending() {
        let state = InitState::default();
        assert_eq!(state.phase, InitPhase::Pending);
        assert_eq!(state.round, 0);
        assert!(state.rounds.is_empty());
    }

    #[test]
    fn init_state_new_sets_fields() {
        let state = InitState::new(
            "id-1".to_string(),
            "opencode".to_string(),
            "build a test".to_string(),
        );
        assert_eq!(state.id, "id-1");
        assert_eq!(state.host, "opencode");
        assert_eq!(state.idea, "build a test");
    }

    #[test]
    fn answered_ratio_zero_when_no_requirements() {
        let state = InitState::default();
        assert_eq!(state.answered_ratio(), 0.0);
    }

    #[test]
    fn answered_ratio_half_when_half_answered() {
        let mut state = InitState::default();
        state.requirements.items = vec![
            Requirement {
                id: "R1".to_string(),
                text: "need X".to_string(),
                source: RequirementSource::User,
                answered: true,
            },
            Requirement {
                id: "R2".to_string(),
                text: "need Y".to_string(),
                source: RequirementSource::User,
                answered: false,
            },
        ];
        assert!((state.answered_ratio() - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn terminal_phase_is_applied() {
        let terminal = InitState {
            phase: InitPhase::Applied {
                session_id: "s1".to_string(),
            },
            ..InitState::default()
        };
        assert!(terminal.is_terminal());
        assert!(terminal.converged());
    }

    #[test]
    fn non_terminal_phase_not_terminal() {
        let q = InitState {
            phase: InitPhase::Questioning {
                remaining: vec!["Q1".to_string()],
            },
            ..InitState::default()
        };
        assert!(!q.is_terminal());
        assert!(!q.converged());
    }

    #[test]
    fn converged_phase_is_converged() {
        let c = InitState {
            phase: InitPhase::Converged {
                draft_id: "d1".to_string(),
            },
            ..InitState::default()
        };
        assert!(c.converged());
        assert!(!c.is_terminal());
    }

    #[test]
    fn contradictions_open_count() {
        let mut state = InitState::default();
        let now = Utc::now();
        state.contradictions = vec![
            Contradiction {
                id: "C1".to_string(),
                topic: "t".to_string(),
                claim_a: "a".to_string(),
                claim_b: "b".to_string(),
                sources_a: vec![],
                sources_b: vec![],
                detected_at: now,
                resolved: false,
                resolution: None,
            },
            Contradiction {
                id: "C2".to_string(),
                topic: "t2".to_string(),
                claim_a: "a2".to_string(),
                claim_b: "b2".to_string(),
                sources_a: vec![],
                sources_b: vec![],
                detected_at: now,
                resolved: true,
                resolution: Some("resolved".to_string()),
            },
        ];
        assert_eq!(state.contradictions_open_count(), 1);
    }

    #[test]
    fn increment_round_increases_round() {
        let mut state = InitState::default();
        state.increment_round();
        assert_eq!(state.round, 1);
        state.increment_round();
        assert_eq!(state.round, 2);
    }

    #[test]
    fn research_entry_average_score_none_when_empty() {
        let entry = ResearchEntry {
            id: "R1".to_string(),
            topic: "test".to_string(),
            query: "test query".to_string(),
            sources: vec![],
            contradictions: vec![],
            loops: 0,
            created_at: Utc::now(),
            fetched_at: Utc::now(),
        };
        assert!(entry.average_score().is_none());
    }

    #[test]
    fn research_entry_average_score_correct() {
        let entry = ResearchEntry {
            id: "R1".to_string(),
            topic: "test".to_string(),
            query: "test query".to_string(),
            sources: vec![
                RankedSource {
                    url: "a".to_string(),
                    title: "A".to_string(),
                    authority: 0.8,
                    recency: 1.0,
                    relevance: 1.0,
                    agreement: 0.9,
                    score: 0.72,
                    claims: vec![],
                    snippet: None,
                    fetched_at: Utc::now(),
                },
                RankedSource {
                    url: "b".to_string(),
                    title: "B".to_string(),
                    authority: 0.7,
                    recency: 0.8,
                    relevance: 1.0,
                    agreement: 1.0,
                    score: 0.56,
                    claims: vec![],
                    snippet: None,
                    fetched_at: Utc::now(),
                },
            ],
            contradictions: vec![],
            loops: 0,
            created_at: Utc::now(),
            fetched_at: Utc::now(),
        };
        let avg = entry.average_score().unwrap();
        assert!((avg - 0.64).abs() < 1e-5);
    }

    #[test]
    fn research_entry_has_min_sources() {
        let entry = ResearchEntry {
            id: "R1".to_string(),
            topic: "test".to_string(),
            query: "q".to_string(),
            sources: vec![
                RankedSource {
                    url: "a".to_string(),
                    title: "A".to_string(),
                    authority: 1.0,
                    recency: 1.0,
                    relevance: 1.0,
                    agreement: 1.0,
                    score: 1.0,
                    claims: vec![],
                    snippet: None,
                    fetched_at: Utc::now(),
                },
                RankedSource {
                    url: "b".to_string(),
                    title: "B".to_string(),
                    authority: 1.0,
                    recency: 1.0,
                    relevance: 1.0,
                    agreement: 1.0,
                    score: 1.0,
                    claims: vec![],
                    snippet: None,
                    fetched_at: Utc::now(),
                },
            ],
            contradictions: vec![],
            loops: 0,
            created_at: Utc::now(),
            fetched_at: Utc::now(),
        };
        assert!(entry.has_min_sources(2));
        assert!(!entry.has_min_sources(3));
    }

    #[test]
    fn ranked_source_compute_score_normalizes_legacy_values_and_clamps() {
        let mut source = RankedSource {
            url: "x".to_string(),
            title: "X".to_string(),
            authority: 8.0,
            recency: 1.5,
            relevance: 1.0,
            agreement: -0.5,
            score: 0.0,
            claims: vec![],
            snippet: None,
            fetched_at: Utc::now(),
        };
        source.compute_score();
        assert!((source.authority - 0.8).abs() < f32::EPSILON);
        assert_eq!(source.recency, 1.0);
        assert_eq!(source.agreement, 0.0);
        assert_eq!(source.score, 0.0);
    }

    #[test]
    fn ranked_source_deserializes_legacy_scores_to_unit_interval() {
        let json = format!(
            r#"{{
                "url":"https://example.com",
                "title":"Legacy",
                "authority":8.0,
                "recency":1.0,
                "agreement":0.9,
                "score":7.2,
                "claims":[],
                "fetchedAt":"{}"
            }}"#,
            Utc::now().to_rfc3339()
        );
        let source: RankedSource = serde_json::from_str(&json).expect("legacy source");
        assert!((source.authority - 0.8).abs() < f32::EPSILON);
        assert!((source.score - 0.72).abs() < f32::EPSILON);
    }

    #[test]
    fn metrics_record_round_tracks_tokens() {
        let now = Utc::now();
        let mut metrics = Metrics::new(now);
        metrics.record_round(1, 1000, 200);
        assert_eq!(metrics.total_tokens_in, 1000);
        assert_eq!(metrics.total_tokens_out, 200);
        assert_eq!(metrics.current_round, 1);

        metrics.record_round(2, 2000, 400);
        assert_eq!(metrics.total_tokens_in, 3000);
        assert_eq!(metrics.total_tokens_out, 600);
        assert_eq!(metrics.current_round, 2);
    }

    #[test]
    fn metrics_update_existing_round() {
        let now = Utc::now();
        let mut metrics = Metrics::new(now);
        metrics.record_round(1, 1000, 200);
        metrics.record_round(1, 500, 100);
        assert_eq!(metrics.total_tokens_in, 1500);
        assert_eq!(metrics.total_tokens_out, 300);
        assert_eq!(metrics.per_round.len(), 1);
    }

    #[test]
    fn round_new_is_empty() {
        let r = Round::new(1);
        assert_eq!(r.number, 1);
        assert!(r.questions.is_empty());
        assert!(r.answers.is_empty());
        assert!(r.notes.is_empty());
    }

    #[test]
    fn artifact_intent_as_str() {
        assert_eq!(ArtifactIntent::Design.as_str(), "design");
        assert_eq!(ArtifactIntent::Build.as_str(), "build");
        assert_eq!(ArtifactIntent::Mixed.as_str(), "mixed");
    }

    #[test]
    fn plan_draft_defaults() {
        let draft = PlanDraft {
            id: "d1".to_string(),
            title: "Test Plan".to_string(),
            tickets: vec![],
            waves: vec![],
            skills_required: vec![],
            skills_brain_required: vec![],
            research_validated_topics: vec![],
            contradictions_open: 0,
            created_at: Utc::now(),
        };
        assert!(!draft.id.is_empty());
    }

    #[test]
    fn init_state_serde_roundtrip() {
        let state = InitState::new(
            "test-id".to_string(),
            "opencode".to_string(),
            "test idea".to_string(),
        );
        let json = serde_json::to_string(&state).expect("serialize");
        let deserialized: InitState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.id, state.id);
        assert_eq!(deserialized.host, state.host);
        assert_eq!(deserialized.idea, state.idea);
    }

    #[test]
    fn init_phase_serde_tagged_roundtrip() {
        let phase = InitPhase::Questioning {
            remaining: vec!["Q1".to_string(), "Q2".to_string()],
        };
        let json = serde_json::to_string(&phase).expect("serialize");
        let deserialized: InitPhase = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(phase, deserialized);
    }
}
