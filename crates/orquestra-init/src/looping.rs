use crate::error::InitError;
use crate::research::{ResearchStatus, ResearchTopic, assess_sources};
use crate::state::{Contradiction, InitPhase, InitState, MAX_ROUNDS, RankedSource, Round};
use orquestra_core::config::InitConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum TopicVerdict {
    Accept,
    ConfirmNeeded { reasons: Vec<String> },
    ReResearch { reasons: Vec<String> },
    ForceOverride { reasons: Vec<String> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvalReport {
    pub verdict: TopicVerdict,
    pub avg_score: f32,
    pub source_count: usize,
}

pub fn evaluate_topic(topic: &ResearchTopic, loop_count: u32, max_loops: u32) -> EvalReport {
    let mut config = InitConfig::default();
    config.research.max_research_loops = max_loops;
    evaluate_topic_with_config(topic, loop_count, &config)
}

pub fn evaluate_topic_with_config(
    topic: &ResearchTopic,
    loop_count: u32,
    config: &InitConfig,
) -> EvalReport {
    let assessment = assess_sources(&topic.sources, config);
    let avg = assessment.average_score;
    let source_count = assessment.counted_sources.len();
    let mut reasons = assessment.reasons(config);
    let contradictions = detect_source_contradictions(&assessment.counted_sources, &topic.topic);
    if !contradictions.is_empty() {
        reasons.push(format!("{} contradictions found", contradictions.len()));
    }

    let verdict = if reasons.is_empty() {
        TopicVerdict::Accept
    } else if config.research.allow_user_override
        && loop_count >= config.research.max_research_loops
    {
        TopicVerdict::ForceOverride { reasons }
    } else {
        TopicVerdict::ReResearch { reasons }
    };

    EvalReport {
        verdict,
        avg_score: avg,
        source_count,
    }
}

pub fn should_converge(
    topics: &[ResearchTopic],
    state: &InitState,
    min_rounds: u32,
    max_loops: u32,
) -> Result<bool, InitError> {
    let mut config = InitConfig::default();
    config.research.max_research_loops = max_loops;
    should_converge_with_config(topics, state, min_rounds, &config)
}

pub fn should_converge_with_config(
    topics: &[ResearchTopic],
    state: &InitState,
    min_rounds: u32,
    config: &InitConfig,
) -> Result<bool, InitError> {
    if state.round < min_rounds {
        return Ok(false);
    }

    if state.round > MAX_ROUNDS {
        return Err(format!("round {} exceeds max {}", state.round, MAX_ROUNDS).into());
    }

    if topics.is_empty() {
        return Ok(false);
    }

    for topic in topics {
        if topic.status != ResearchStatus::Validated {
            return Ok(false);
        }
        let report = evaluate_topic_with_config(topic, state.round, config);
        if report.verdict != TopicVerdict::Accept {
            return Ok(false);
        }
    }

    Ok(true)
}

pub fn enriched_query(topic_title: &str, round: u32, previous_queries: &[String]) -> String {
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    match round {
        0 => format!("{} {}", topic_title, date),
        1 => format!("{} {} detailed technical analysis", topic_title, date),
        _ if previous_queries.is_empty() => {
            format!("{} {} comprehensive authoritative guide", topic_title, date)
        }
        _ => format!(
            "{} {} comprehensive authoritative guide - avoiding common pitfalls",
            topic_title, date
        ),
    }
}

fn detect_source_contradictions(sources: &[RankedSource], topic: &str) -> Vec<Contradiction> {
    let mut result = Vec::new();
    for i in 0..sources.len() {
        for j in (i + 1)..sources.len() {
            let a = &sources[i];
            let b = &sources[j];
            if a.authority >= 0.6 && b.authority >= 0.6 && (a.agreement < 0.5 || b.agreement < 0.5)
            {
                let set_a: std::collections::HashSet<&str> =
                    a.claims.iter().map(|s| s.as_str()).collect();
                let set_b: std::collections::HashSet<&str> =
                    b.claims.iter().map(|s| s.as_str()).collect();
                let mismatched = set_a != set_b;
                if mismatched {
                    result.push(Contradiction {
                        id: format!("contra-{}-{}-{}", topic, i, j),
                        topic: topic.to_string(),
                        claim_a: a.claims.first().cloned().unwrap_or_default(),
                        claim_b: b.claims.first().cloned().unwrap_or_default(),
                        sources_a: vec![a.title.clone()],
                        sources_b: vec![b.title.clone()],
                        detected_at: chrono::Utc::now(),
                        resolved: false,
                        resolution: None,
                    });
                }
            }
        }
    }
    result
}

pub fn next_phase(
    eval_reports: &[(&String, EvalReport)],
    current_round: u32,
    max_loops: u32,
) -> InitPhase {
    let all_accepted = eval_reports
        .iter()
        .all(|(_, r)| r.verdict == TopicVerdict::Accept);
    let any_re_research = eval_reports
        .iter()
        .any(|(_, r)| matches!(r.verdict, TopicVerdict::ReResearch { .. }));
    let any_force_override = eval_reports
        .iter()
        .any(|(_, r)| matches!(r.verdict, TopicVerdict::ForceOverride { .. }));

    if all_accepted {
        return InitPhase::Converged {
            draft_id: format!("draft-r{}", current_round),
        };
    }

    let confirm_needed: Vec<String> = eval_reports
        .iter()
        .filter(|(_, r)| matches!(r.verdict, TopicVerdict::ConfirmNeeded { .. }))
        .map(|(topic, _)| (*topic).clone())
        .collect();

    if !confirm_needed.is_empty() {
        return InitPhase::Questioning {
            remaining: confirm_needed
                .into_iter()
                .map(|t| {
                    format!(
                        "Confirm: sources for '{}' are below threshold. Accept anyway?",
                        t
                    )
                })
                .collect(),
        };
    }

    if any_force_override {
        let override_topics: Vec<String> = eval_reports
            .iter()
            .filter(|(_, r)| matches!(r.verdict, TopicVerdict::ForceOverride { .. }))
            .map(|(topic, _)| (*topic).clone())
            .collect();
        return InitPhase::Questioning {
            remaining: override_topics
                .into_iter()
                .map(|t| {
                    format!(
                        "Override: research for '{}' failed after {} loops. Force continue?",
                        t, max_loops
                    )
                })
                .collect(),
        };
    }

    if any_re_research {
        let topics: Vec<String> = eval_reports
            .iter()
            .filter(|(_, r)| matches!(r.verdict, TopicVerdict::ReResearch { .. }))
            .map(|(topic, _)| (*topic).clone())
            .collect();
        return InitPhase::Researching { topics };
    }

    InitPhase::Converged {
        draft_id: format!("draft-r{}", current_round),
    }
}

pub fn add_round_with_eval(
    state: &mut InitState,
    eval_reports: &[(&String, EvalReport)],
    max_loops: u32,
) {
    state.increment_round();
    let phase = next_phase(eval_reports, state.round, max_loops);
    state.phase = phase;
    state.updated_at = chrono::Utc::now();

    let round = Round::new(state.round);
    state.rounds.push(round);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::generate_brief;
    use orquestra_core::config::InitConfig;

    fn make_source(
        title: &str,
        authority: f32,
        recency: f32,
        agreement: f32,
        claims: Vec<&str>,
    ) -> RankedSource {
        let url = if title.contains('.') || title.contains('/') {
            format!("https://{title}")
        } else {
            format!(
                "https://{}.source-{}.com",
                title.to_ascii_lowercase(),
                title.to_ascii_lowercase()
            )
        };
        let mut s = RankedSource {
            url,
            title: title.to_string(),
            authority,
            recency,
            relevance: 1.0,
            agreement,
            claims: claims.into_iter().map(|c| c.to_string()).collect(),
            score: 0.0,
            snippet: None,
            fetched_at: chrono::Utc::now(),
        };
        s.compute_score();
        s
    }

    fn make_topic(title: &str, sources: Vec<RankedSource>) -> ResearchTopic {
        let dir = std::env::temp_dir();
        let mut t = generate_brief(&dir, "test-session", title).expect("brief");
        t.sources = sources;
        t.status = ResearchStatus::Validated;
        t
    }

    fn test_config() -> InitConfig {
        let mut config = InitConfig::default();
        config.research.min_sources_per_topic = 4;
        config.research.min_reliability_score = 0.7;
        config.research.allow_user_override = true;
        config.require_primary_source = false;
        config
    }

    #[test]
    fn evaluate_accepts_high_score() {
        let sources = vec![
            make_source("A", 9.0, 1.0, 1.0, vec!["good"]),
            make_source("B", 8.0, 1.0, 1.0, vec!["good"]),
            make_source("C", 7.0, 0.9, 1.0, vec!["good"]),
            make_source("D", 9.0, 1.0, 1.0, vec!["good"]),
        ];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 0, &test_config());
        assert_eq!(report.verdict, TopicVerdict::Accept);
    }

    #[test]
    fn evaluate_reresearches_when_score_is_below_configured_gate() {
        let sources = vec![
            make_source("A", 5.0, 1.0, 1.0, vec!["okay"]),
            make_source("B", 5.0, 1.0, 1.0, vec!["okay"]),
            make_source("C", 5.0, 0.9, 1.0, vec!["okay"]),
            make_source("D", 5.0, 1.0, 1.0, vec!["okay"]),
        ];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 0, &test_config());
        assert!(matches!(report.verdict, TopicVerdict::ReResearch { .. }));
    }

    #[test]
    fn evaluate_reresearch_on_low_score() {
        let sources = vec![
            make_source("A", 2.0, 0.5, 0.5, vec!["bad"]),
            make_source("B", 2.0, 0.5, 0.5, vec!["bad"]),
            make_source("C", 2.0, 0.4, 0.5, vec!["bad"]),
            make_source("D", 2.0, 0.5, 0.5, vec!["bad"]),
        ];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 0, &test_config());
        assert!(matches!(report.verdict, TopicVerdict::ReResearch { .. }));
    }

    #[test]
    fn evaluate_force_override_after_max_loops() {
        let sources = vec![
            make_source("A", 2.0, 0.5, 0.5, vec!["bad"]),
            make_source("B", 2.0, 0.5, 0.5, vec!["bad"]),
            make_source("C", 2.0, 0.4, 0.5, vec!["bad"]),
            make_source("D", 2.0, 0.5, 0.5, vec!["bad"]),
        ];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 3, &test_config());
        assert!(matches!(report.verdict, TopicVerdict::ForceOverride { .. }));
    }

    #[test]
    fn evaluate_rejects_insufficient_sources() {
        let sources = vec![make_source("A", 9.0, 1.0, 1.0, vec!["good"])];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 0, &test_config());
        assert!(matches!(report.verdict, TopicVerdict::ReResearch { .. }));
    }

    #[test]
    fn evaluate_not_enough_sources_on_round_3_triggers_force() {
        let sources = vec![make_source("A", 9.0, 1.0, 1.0, vec!["good"])];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 3, &test_config());
        assert!(matches!(report.verdict, TopicVerdict::ForceOverride { .. }));
    }

    #[test]
    fn should_not_converge_before_min_rounds() {
        let sources = vec![
            make_source("A", 9.0, 1.0, 1.0, vec!["good"]),
            make_source("B", 8.0, 1.0, 1.0, vec!["good"]),
            make_source("C", 7.0, 0.9, 1.0, vec!["good"]),
            make_source("D", 9.0, 1.0, 1.0, vec!["good"]),
        ];
        let topic = make_topic("test", sources);
        let mut state = InitState::new("test-id".into(), "test-host".into(), "test idea".into());
        state.round = 1;
        assert!(!should_converge_with_config(&[topic], &state, 3, &test_config()).unwrap());
    }

    #[test]
    fn enriched_query_adds_detail_on_later_rounds() {
        let q0 = enriched_query("Rust async", 0, &[]);
        assert!(!q0.contains("comprehensive"));
        let q1 = enriched_query("Rust async", 1, &[q0]);
        assert!(q1.contains("detailed technical"));
    }

    #[test]
    fn next_phase_transitions_to_converged_when_all_accepted() {
        let t1 = "t1".to_string();
        let reports = vec![(
            &t1,
            EvalReport {
                verdict: TopicVerdict::Accept,
                avg_score: 8.0,
                source_count: 4,
            },
        )];
        let phase = next_phase(&reports, 1, 3);
        assert!(matches!(phase, InitPhase::Converged { .. }));
    }

    #[test]
    fn next_phase_transitions_to_questioning_on_confirm_needed() {
        let t1 = "t1".to_string();
        let reports = vec![(
            &t1,
            EvalReport {
                verdict: TopicVerdict::ConfirmNeeded {
                    reasons: vec!["low score".into()],
                },
                avg_score: 5.0,
                source_count: 4,
            },
        )];
        let phase = next_phase(&reports, 1, 3);
        assert!(matches!(phase, InitPhase::Questioning { .. }));
    }

    #[test]
    fn next_phase_transitions_to_researching_on_reresearch() {
        let t1 = "t1".to_string();
        let reports = vec![(
            &t1,
            EvalReport {
                verdict: TopicVerdict::ReResearch {
                    reasons: vec!["low score".into()],
                },
                avg_score: 2.0,
                source_count: 4,
            },
        )];
        let phase = next_phase(&reports, 1, 3);
        assert!(matches!(phase, InitPhase::Researching { .. }));
    }

    #[test]
    fn next_phase_force_override_goes_to_questioning() {
        let t1 = "t1".to_string();
        let reports = vec![(
            &t1,
            EvalReport {
                verdict: TopicVerdict::ForceOverride {
                    reasons: vec!["max loops".into()],
                },
                avg_score: 2.0,
                source_count: 4,
            },
        )];
        let phase = next_phase(&reports, 3, 3);
        assert!(matches!(phase, InitPhase::Questioning { .. }));
    }

    #[test]
    fn add_round_with_eval_appends_round() {
        let mut state = InitState::new("test-id".into(), "host".into(), "idea".into());
        let len_before = state.rounds.len();
        let t1 = "t1".to_string();
        let reports = vec![(
            &t1,
            EvalReport {
                verdict: TopicVerdict::Accept,
                avg_score: 8.0,
                source_count: 4,
            },
        )];
        add_round_with_eval(&mut state, &reports, 3);
        assert_eq!(state.rounds.len(), len_before + 1);
        assert_eq!(state.round, 1);
    }

    #[test]
    fn should_converge_false_on_empty_topics() {
        let state = InitState::new("test-id".into(), "host".into(), "idea".into());
        assert!(!should_converge_with_config(&[], &state, 1, &test_config()).unwrap());
    }

    #[test]
    fn should_converge_errors_on_excessive_rounds() {
        let topic = make_topic("test", vec![make_source("A", 9.0, 1.0, 1.0, vec!["g"])]);
        let mut state = InitState::new("test-id".into(), "host".into(), "idea".into());
        state.round = 101;
        let result = should_converge_with_config(&[topic], &state, 1, &test_config());
        assert!(result.is_err());
    }

    #[test]
    fn evaluate_avg_score_is_computed_correctly() {
        let sources = vec![
            make_source("A", 9.0, 1.0, 1.0, vec!["x"]),
            make_source("B", 8.0, 1.0, 1.0, vec!["x"]),
            make_source("C", 7.0, 1.0, 1.0, vec!["x"]),
            make_source("D", 6.0, 1.0, 1.0, vec!["x"]),
        ];
        let topic = make_topic("test", sources);
        let report = evaluate_topic_with_config(&topic, 0, &test_config());
        assert!((report.avg_score - 0.75).abs() < 0.1);
    }

    #[test]
    fn detect_contradiction_on_mismatched_claim_sets() {
        use crate::research::detect_contradictions;
        let a = make_source("A", 8.0, 1.0, 0.3, vec!["alpine is best"]);
        let b = make_source("B", 8.0, 1.0, 0.9, vec!["slim is best"]);
        let c = make_source("C", 7.0, 0.9, 0.9, vec!["alpine is best"]);
        let mut ab = a.clone();
        ab.claims = vec!["feature-x".into(), "feature-y".into()];
        let mut bb = b.clone();
        bb.claims = vec!["feature-x".into(), "feature-z".into()];
        let sources = vec![ab, bb, c];
        let contradictions = detect_contradictions(&sources);
        assert!(
            !contradictions.is_empty(),
            "should detect mismatch between A and B claims"
        );
    }

    #[test]
    fn loop_detector_uses_normalized_authority_threshold() {
        let a = make_source("a.example.com/a", 0.8, 1.0, 0.3, vec!["alpine is best"]);
        let b = make_source("b.example.net/b", 0.8, 1.0, 0.9, vec!["slim is best"]);
        let contradictions = detect_source_contradictions(&[a, b], "container base");
        assert_eq!(contradictions.len(), 1);
    }

    #[test]
    fn no_false_contradiction_on_identical_claim_sets() {
        use crate::research::detect_contradictions;
        let a = make_source("A", 8.0, 1.0, 0.9, vec!["feature-x", "feature-y"]);
        let b = make_source("B", 8.0, 1.0, 0.9, vec!["feature-x", "feature-y"]);
        let c = make_source("C", 8.0, 1.0, 0.9, vec!["feature-x", "feature-y"]);
        let d = make_source("D", 8.0, 1.0, 0.9, vec!["feature-x", "feature-y"]);
        let sources = vec![a, b, c, d];
        let contradictions = detect_contradictions(&sources);
        assert!(
            contradictions.is_empty(),
            "identical claim sets should not be contradictions"
        );
    }

    #[test]
    fn evaluate_topic_uses_configured_policy_instead_of_legacy_thresholds() {
        let sources = vec![
            make_source("a.example.com/a", 0.9, 1.0, 1.0, vec!["same"]),
            make_source("b.example.net/b", 0.9, 1.0, 1.0, vec!["same"]),
            make_source("c.example.org/c", 0.9, 1.0, 1.0, vec!["same"]),
        ];
        let topic = make_topic("test", sources);
        let mut config = InitConfig::default();
        config.research.min_sources_per_topic = 3;
        config.research.min_reliability_score = 0.85;
        config.require_primary_source = false;

        let report = evaluate_topic_with_config(&topic, 0, &config);
        assert_eq!(report.verdict, TopicVerdict::Accept);
        assert!((report.avg_score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn public_looping_functions_keep_legacy_signatures() {
        let _evaluate: fn(&ResearchTopic, u32, u32) -> EvalReport = evaluate_topic;
        let _converge: fn(&[ResearchTopic], &InitState, u32, u32) -> Result<bool, InitError> =
            should_converge;
    }
}
