#![allow(dead_code)]

pub mod artifact;
pub mod classifier;
pub mod convergence;
pub mod error;
pub mod looping;
pub mod research;
pub mod state;
pub mod storage;

pub use artifact::build_artifacts;
pub use classifier::{
    HeuristicClassifier, RefinementRequest, RefinementResponse, merge_classifications,
};
pub use convergence::generate_plan_draft;
pub use error::InitError;
pub use looping::{EvalReport, TopicVerdict};
pub use research::{
    ResearchStatus, ResearchTopic, authority_for_url, generate_brief, generate_query, list_topics,
    load_research_topic, normalize_claim, save_research_topic, store_results,
    store_results_with_config, validate, validate_with_config,
};
pub use state::{
    Answer, AnswerConfidence, ArtifactIntent, ArtifactScope, Audience, Classification,
    Contradiction, DraftTicket, DraftWave, InitPhase, InitState, Metrics, PlanDraft, Question,
    QuestionCategory, RankedSource, Requirement, RequirementSource, ResearchEntry, Round,
    RoundMetrics,
};
pub use storage::{
    append_event, ensure_init_dirs, events_file, list_sessions, load_state, metrics_file,
    read_json, save_metrics_json, save_state, session_dir, state_file, validate_init_id,
};

use chrono::Utc;
use orquestra_core::config::InitConfig;
use serde::{Deserialize, Serialize};

const ISO_FMT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

pub fn iso_now() -> String {
    Utc::now().format(ISO_FMT).to_string()
}

pub fn today_date() -> String {
    Utc::now().format("%Y-%m-%d").to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConvergenceAssessment {
    pub confidence: f32,
    pub classification_confidence: f32,
    pub research_confidence: f32,
    pub requirements_confidence: f32,
    pub blockers: Vec<String>,
}

pub fn assess_convergence(
    state: &InitState,
    topics: &[ResearchTopic],
    config: &InitConfig,
) -> ConvergenceAssessment {
    let mut blockers = Vec::new();
    let classification_confidence = state
        .classification
        .as_ref()
        .map(|classification| classification.confidence.clamp(0.0, 1.0))
        .unwrap_or(0.0);
    if state.classification.is_none() {
        blockers.push("classification is missing".to_string());
    } else if classification_confidence < config.min_confidence {
        blockers.push(format!(
            "classification confidence {:.2} < {:.2}",
            classification_confidence, config.min_confidence
        ));
    }

    let mut research_confidence = 0.0_f32;
    if topics.is_empty() {
        blockers.push("research is missing".to_string());
    } else {
        research_confidence = 1.0;
        for topic in topics {
            if topic.status != ResearchStatus::Validated {
                blockers.push(format!("research topic '{}' is not validated", topic.topic));
                research_confidence = 0.0;
                continue;
            }
            let report = looping::evaluate_topic_with_config(topic, state.round, config);
            research_confidence = research_confidence.min(report.avg_score);
            if report.verdict != TopicVerdict::Accept {
                blockers.push(format!(
                    "research topic '{}' has not converged: {:?}",
                    topic.topic, report.verdict
                ));
            }
        }
    }

    let requirements_confidence = if state.requirements.items.is_empty() {
        1.0
    } else {
        let supported = state
            .requirements
            .items
            .iter()
            .filter(|requirement| {
                requirement.answered
                    || matches!(
                        requirement.source,
                        RequirementSource::User | RequirementSource::Research
                    )
            })
            .count();
        supported as f32 / state.requirements.items.len() as f32
    };
    if requirements_confidence < 1.0 {
        blockers.push("one or more inferred requirements need confirmation".to_string());
    }

    let open_contradictions = state.contradictions_open_count() as usize;
    let contradictions_confidence = if open_contradictions <= config.max_contradictions {
        1.0
    } else {
        blockers.push(format!(
            "{open_contradictions} open contradiction(s) exceed the configured maximum {}",
            config.max_contradictions
        ));
        0.0
    };
    if state.round < config.min_rounds {
        blockers.push(format!(
            "discovery round {} < configured minimum {}",
            state.round, config.min_rounds
        ));
    }

    let confidence = classification_confidence
        .min(research_confidence)
        .min(requirements_confidence)
        .min(contradictions_confidence);
    if confidence < config.min_confidence
        && !blockers
            .iter()
            .any(|blocker| blocker.contains("confidence"))
    {
        blockers.push(format!(
            "composite confidence {:.2} < {:.2}",
            confidence, config.min_confidence
        ));
    }

    ConvergenceAssessment {
        confidence,
        classification_confidence,
        research_confidence,
        requirements_confidence,
        blockers,
    }
}

pub fn create_init_session(
    project_dir: &std::path::Path,
    host: &str,
    idea: Option<&str>,
) -> Result<InitState, InitError> {
    storage::ensure_init_dirs(project_dir)?;

    let id = uuid::Uuid::new_v4().to_string();
    let idea_text = idea.unwrap_or("").to_string();
    let state = InitState::new(id, host.to_string(), idea_text.clone());

    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        &state.id,
        "init_created",
        &serde_json::json!({
            "host": host,
            "idea": idea_text,
        }),
    )?;

    Ok(state)
}

pub fn answer_question(
    project_dir: &std::path::Path,
    session_id: &str,
    question_id: &str,
    answer_text: &str,
) -> Result<InitState, InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;

    if state.is_terminal() {
        return Err(InitError::InvalidTransition(format!(
            "Cannot answer in terminal phase {:?}",
            state.phase
        )));
    }

    let answer = Answer {
        question_id: question_id.to_string(),
        text: answer_text.to_string(),
        confidence: state::AnswerConfidence::Confirmed,
        created_at: Utc::now(),
    };

    if state.rounds.is_empty() {
        state.increment_round();
        state.rounds.push(Round::new(state.round));
    }

    if let Some(round) = state.rounds.last_mut() {
        round.answers.push(answer);
    }

    state.updated_at = Utc::now();
    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        session_id,
        "question_answered",
        &serde_json::json!({
            "question_id": question_id,
        }),
    )?;

    Ok(state)
}

pub fn add_note(
    project_dir: &std::path::Path,
    session_id: &str,
    note_text: &str,
) -> Result<InitState, InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;

    if state.is_terminal() {
        return Err(InitError::InvalidTransition(format!(
            "Cannot add note in terminal phase {:?}",
            state.phase
        )));
    }

    if let Some(round) = state.rounds.last_mut() {
        round.notes.push(note_text.to_string());
    }

    state.updated_at = Utc::now();
    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        session_id,
        "note_added",
        &serde_json::json!({
            "note_length": note_text.len(),
        }),
    )?;

    Ok(state)
}

pub fn cancel_init(
    project_dir: &std::path::Path,
    session_id: &str,
) -> Result<InitState, InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;

    if state.is_terminal() {
        return Err(InitError::InvalidTransition(format!(
            "Cannot cancel init in terminal phase {:?}",
            state.phase
        )));
    }

    state.phase = InitPhase::Cancelled;
    state.updated_at = Utc::now();

    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        session_id,
        "init_cancelled",
        &serde_json::json!({"reason": "user_requested"}),
    )?;

    Ok(state)
}

pub fn record_tokens(
    project_dir: &std::path::Path,
    session_id: &str,
    tokens_in: u32,
    tokens_out: u32,
) -> Result<InitState, InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;
    state.record_tokens(tokens_in, tokens_out);
    state.updated_at = Utc::now();
    storage::save_state(project_dir, &state)?;
    let metrics_json = serde_json::to_value(&state.metrics)?;
    storage::save_metrics_json(project_dir, session_id, &metrics_json)?;
    Ok(state)
}

pub fn classify_init(
    project_dir: &std::path::Path,
    session_id: &str,
    refinement_json: Option<&str>,
) -> Result<InitState, InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;

    if state.is_terminal() {
        return Err(InitError::InvalidTransition(format!(
            "Cannot classify in terminal phase {:?}",
            state.phase
        )));
    }

    let heuristic = HeuristicClassifier::classify(&state.idea, &state.requirements);

    let classification = if let Some(json) = refinement_json {
        let refinement: RefinementResponse = serde_json::from_str(json)
            .map_err(|e| InitError::from(format!("Invalid refinement response JSON: {e}")))?;
        merge_classifications(&heuristic, Some(&refinement))
    } else {
        heuristic
    };

    state.classification = Some(classification);
    state.updated_at = Utc::now();
    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        session_id,
        "classified",
        &serde_json::json!({
            "intent": format!("{:?}", state.classification.as_ref().unwrap().intent),
            "confidence": state.classification.as_ref().unwrap().confidence,
        }),
    )?;

    Ok(state)
}

pub fn add_requirement(
    project_dir: &std::path::Path,
    session_id: &str,
    text: &str,
    source: &str,
) -> Result<InitState, InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;

    if state.is_terminal() {
        return Err(InitError::InvalidTransition(format!(
            "Cannot add requirement in terminal phase {:?}",
            state.phase
        )));
    }

    let source_enum = match source {
        "research" => RequirementSource::Research,
        "inferred" => RequirementSource::Inferred,
        _ => RequirementSource::User,
    };

    let req = Requirement {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.to_string(),
        source: source_enum,
        answered: false,
    };

    state.requirements.items.push(req);
    state.updated_at = Utc::now();
    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        session_id,
        "requirement_added",
        &serde_json::json!({
            "text_length": text.len(),
            "source": source,
        }),
    )?;

    Ok(state)
}

pub fn evaluate_session(
    project_dir: &std::path::Path,
    session_id: &str,
    max_loops: u32,
) -> Result<(InitState, Vec<(String, EvalReport)>), InitError> {
    let mut config = orquestra_core::config::InitConfig::default();
    config.research.max_research_loops = max_loops;
    evaluate_session_with_config(project_dir, session_id, &config)
}

pub fn evaluate_session_with_config(
    project_dir: &std::path::Path,
    session_id: &str,
    config: &orquestra_core::config::InitConfig,
) -> Result<(InitState, Vec<(String, EvalReport)>), InitError> {
    let mut state = storage::load_state(project_dir, session_id)?;

    if state.is_terminal() {
        return Err(InitError::InvalidTransition(format!(
            "Cannot evaluate in terminal phase {:?}",
            state.phase
        )));
    }

    let topics = crate::research::list_topics(project_dir, session_id)?;
    let evaluable: Vec<_> = topics
        .into_iter()
        .filter(|t| t.status == crate::research::ResearchStatus::Completed)
        .collect();
    if evaluable.is_empty() {
        return Err(InitError::Research(
            "No completed research topics to evaluate. Run 'init store-research' first.".into(),
        ));
    }

    let eval_pairs: Vec<(String, EvalReport)> = evaluable
        .iter()
        .map(|t| {
            let report = crate::looping::evaluate_topic_with_config(t, state.round, config);
            (t.topic.clone(), report)
        })
        .collect();

    let eval_refs: Vec<(&String, EvalReport)> = eval_pairs
        .iter()
        .map(|(topic, report)| (topic, report.clone()))
        .collect();

    crate::looping::add_round_with_eval(&mut state, &eval_refs, config.research.max_research_loops);

    for (topic, report) in evaluable
        .iter()
        .zip(eval_pairs.iter().map(|(_, report)| report))
    {
        if report.verdict == TopicVerdict::Accept {
            crate::research::validate_with_config(project_dir, session_id, &topic.id, config)?;
        }
    }
    let topics = crate::research::list_topics(project_dir, session_id)?;
    let assessment = assess_convergence(&state, &topics, config);
    if assessment.blockers.is_empty() {
        state.phase = InitPhase::Converged {
            draft_id: format!("draft-r{}", state.round),
        };
    } else if matches!(state.phase, InitPhase::Converged { .. }) {
        state.phase = InitPhase::Questioning {
            remaining: assessment.blockers.clone(),
        };
    }

    storage::save_state(project_dir, &state)?;
    storage::append_event(
        project_dir,
        session_id,
        "evaluated",
        &serde_json::json!({
            "round": state.round,
            "phase": format!("{:?}", state.phase),
            "topics": eval_pairs.len(),
            "confidence": assessment.confidence,
            "blockers": assessment.blockers,
        }),
    )?;

    Ok((state, eval_pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    #[test]
    fn create_init_session_persists_state() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test idea"))
            .expect("create init session");
        assert_eq!(state.host, "opencode");
        assert_eq!(state.idea, "test idea");
        assert_eq!(state.phase, InitPhase::Pending);
        assert!(
            storage::state_file(dir.path(), &state.id)
                .expect("state file path")
                .exists()
        );
    }

    #[test]
    fn create_init_session_without_idea() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", None).expect("create init session");
        assert!(state.idea.is_empty());
    }

    #[test]
    fn answer_question_updates_state() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        let updated =
            answer_question(dir.path(), &state.id, "Q1", "This is my answer").expect("answer");
        assert_eq!(updated.round, 1);
        assert_eq!(updated.rounds.len(), 1);
        assert_eq!(updated.rounds[0].answers.len(), 1);
        assert_eq!(updated.rounds[0].answers[0].text, "This is my answer");
    }

    #[test]
    fn answer_question_rejected_in_terminal_phase() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        cancel_init(dir.path(), &state.id).expect("cancel");
        let err =
            answer_question(dir.path(), &state.id, "Q1", "won't work").expect_err("should reject");
        assert!(matches!(err, InitError::InvalidTransition(_)));
    }

    #[test]
    fn add_note_appends_to_round() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        answer_question(dir.path(), &state.id, "Q1", "answer")
            .expect("answer first to create round");
        let updated = add_note(dir.path(), &state.id, "Important note").expect("add note");
        assert!(!updated.rounds.is_empty());
        assert!(
            updated.rounds[0]
                .notes
                .contains(&"Important note".to_string())
        );
    }

    #[test]
    fn cancel_init_ends_session() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        let cancelled = cancel_init(dir.path(), &state.id).expect("cancel");
        assert!(cancelled.is_terminal());
    }

    #[test]
    fn record_tokens_updates_metrics() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        let updated = record_tokens(dir.path(), &state.id, 1500, 400).expect("record tokens");
        assert_eq!(updated.metrics.total_tokens_in, 1500);
        assert_eq!(updated.metrics.total_tokens_out, 400);
    }

    #[test]
    fn multiple_tokens_records_accumulate() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        let updated = record_tokens(dir.path(), &state.id, 1000, 200).expect("first");
        assert_eq!(updated.metrics.total_tokens_in, 1000);
        let updated = record_tokens(dir.path(), &state.id, 500, 100).expect("second");
        assert_eq!(updated.metrics.total_tokens_in, 1500);
    }

    #[test]
    fn iso_now_format() {
        let s = iso_now();
        assert!(s.len() >= 20);
        assert!(s.contains('T'));
    }

    #[test]
    fn today_date_format() {
        let s = today_date();
        assert_eq!(s.len(), 10);
        assert_eq!(s.chars().filter(|&c| c == '-').count(), 2);
    }

    #[test]
    fn classify_init_stores_classification() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("migrate Express to Fastify"))
            .expect("create");
        let updated = classify_init(dir.path(), &state.id, None).expect("classify");
        let c = updated.classification.expect("classification present");
        assert_eq!(c.intent, ArtifactIntent::Migrate);
        assert!(c.confidence > 0.0);
    }

    #[test]
    fn classify_init_rejected_in_terminal_phase() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        cancel_init(dir.path(), &state.id).expect("cancel");
        let err = classify_init(dir.path(), &state.id, None).expect_err("should reject");
        assert!(matches!(err, InitError::InvalidTransition(_)));
    }

    #[test]
    fn classify_init_with_refinement_overrides_heuristic() {
        // Use an idea with a single low-weight keyword so heuristic confidence < 0.95
        let dir = tmp_dir();
        let state =
            create_init_session(dir.path(), "opencode", Some("system check")).expect("create");
        let refinement = r#"{"intent":"audit","scope":"large","audience":"stakeholder","confidence":0.95,"reasoning":"LLM says audit"}"#;
        let updated = classify_init(dir.path(), &state.id, Some(refinement))
            .expect("classify with refinement");
        let c = updated.classification.expect("classification present");
        assert_eq!(c.intent, ArtifactIntent::Audit);
        assert_eq!(c.confidence, 0.95);
    }

    #[test]
    fn add_requirement_appends_to_state() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test idea")).expect("create");
        let updated = add_requirement(dir.path(), &state.id, "must handle 10k req/s", "user")
            .expect("add requirement");
        assert_eq!(updated.requirements.items.len(), 1);
        assert_eq!(updated.requirements.items[0].text, "must handle 10k req/s");
        assert_eq!(
            updated.requirements.items[0].source,
            RequirementSource::User
        );
    }

    #[test]
    fn add_requirement_from_research_source() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test idea")).expect("create");
        let updated = add_requirement(dir.path(), &state.id, "use PostgreSQL 16", "research")
            .expect("add requirement");
        assert_eq!(
            updated.requirements.items[0].source,
            RequirementSource::Research
        );
    }

    #[test]
    fn add_requirement_rejected_in_terminal_phase() {
        let dir = tmp_dir();
        let state = create_init_session(dir.path(), "opencode", Some("test")).expect("create");
        cancel_init(dir.path(), &state.id).expect("cancel");
        let err = add_requirement(dir.path(), &state.id, "won't work", "user")
            .expect_err("should reject");
        assert!(matches!(err, InitError::InvalidTransition(_)));
    }

    #[test]
    fn list_sessions_includes_created() {
        let dir = tmp_dir();
        let s1 = create_init_session(dir.path(), "h1", Some("idea1")).expect("create 1");
        let s2 = create_init_session(dir.path(), "h2", Some("idea2")).expect("create 2");
        let sessions = list_sessions(dir.path()).expect("list");
        assert!(sessions.contains(&s1.id));
        assert!(sessions.contains(&s2.id));
    }
}
