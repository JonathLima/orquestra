use chrono::Utc;

use crate::research::ResearchTopic;
use crate::state::*;

pub fn generate_plan_draft(
    state: &InitState,
    research_topics: &[ResearchTopic],
    max_tickets: usize,
) -> PlanDraft {
    let intent = state
        .classification
        .as_ref()
        .map(|c| c.intent)
        .unwrap_or(ArtifactIntent::Build);

    let scope_size = state
        .classification
        .as_ref()
        .map(|c| c.scope)
        .unwrap_or(ArtifactScope::Medium);

    let ticket_count = match scope_size {
        ArtifactScope::Small => max_tickets.min(3),
        ArtifactScope::Medium => max_tickets.min(5),
        ArtifactScope::Large => max_tickets.min(8),
    };

    let tickets = generate_tickets(state, intent, ticket_count);
    let waves = generate_waves(&tickets);

    let research_validated: Vec<String> = research_topics
        .iter()
        .filter(|t| t.status == crate::research::ResearchStatus::Validated)
        .map(|t| t.topic.clone())
        .collect();

    let contradictions_open = state.contradictions_open_count();

    PlanDraft {
        id: format!(
            "plan-{}",
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        ),
        title: format!("{} — {}", intent.as_str(), state.idea),
        tickets: tickets.clone(),
        waves,
        skills_required: detect_skills_required(&tickets, &research_validated),
        skills_brain_required: Vec::new(),
        research_validated_topics: research_validated,
        contradictions_open,
        created_at: Utc::now(),
    }
}

fn generate_tickets(state: &InitState, intent: ArtifactIntent, count: usize) -> Vec<DraftTicket> {
    let reqs = &state.requirements.items;
    let req_chunks = if reqs.is_empty() {
        vec![vec![]; count]
    } else {
        let chunk_size = reqs.len().max(1).div_ceil(count);
        reqs.chunks(chunk_size.max(1)).map(|c| c.to_vec()).collect()
    };

    let tickets: Vec<DraftTicket> = intent_tickets(intent)
        .into_iter()
        .take(count)
        .enumerate()
        .map(|(i, base)| {
            let id = format!("T{}", i + 1);
            let chunk = req_chunks.get(i).cloned().unwrap_or_default();
            let ac: Vec<String> = chunk.iter().map(|r| r.text.clone()).collect();
            DraftTicket {
                id: id.clone(),
                title: base.title,
                objective: base.objective,
                acceptance_criteria: if ac.is_empty() { base.ac } else { ac },
                // ponytail: flat wave model — T1 root, all else blocked by T1
                // refine to granular dependencies when real DAG support is needed
                blocked_by: if i == 0 {
                    vec![]
                } else {
                    vec!["T1".to_string()]
                },
                preferred_capabilities: base.capabilities,
                assigned_skill: None,
                research_validated: false,
                wave: 0,
            }
        })
        .collect();

    tickets
}

fn generate_waves(tickets: &[DraftTicket]) -> Vec<DraftWave> {
    if tickets.is_empty() {
        return vec![];
    }
    if tickets.len() <= 3 {
        return vec![DraftWave {
            wave_number: 1,
            ticket_ids: tickets.iter().map(|t| t.id.clone()).collect(),
        }];
    }

    let mid = tickets.len() / 2;
    let wave1_ids: Vec<String> = tickets[..mid].iter().map(|t| t.id.clone()).collect();
    let wave2_ids: Vec<String> = tickets[mid..].iter().map(|t| t.id.clone()).collect();

    vec![
        DraftWave {
            wave_number: 1,
            ticket_ids: wave1_ids,
        },
        DraftWave {
            wave_number: 2,
            ticket_ids: wave2_ids,
        },
    ]
}

struct TicketBase {
    title: String,
    objective: String,
    ac: Vec<String>,
    capabilities: Vec<String>,
}

fn intent_tickets(intent: ArtifactIntent) -> Vec<TicketBase> {
    match intent {
        ArtifactIntent::Build => vec![
            TicketBase {
                title: "Define architecture & data model".into(),
                objective: "Design the system architecture and data structures".into(),
                ac: vec![
                    "Architecture documented".into(),
                    "Data model defined".into(),
                ],
                capabilities: vec!["architecture".into(), "data-modeling".into()],
            },
            TicketBase {
                title: "Implement core API layer".into(),
                objective: "Build the primary API endpoints and business logic".into(),
                ac: vec!["API designed".into(), "Core endpoints implemented".into()],
                capabilities: vec!["api-design".into(), "backend".into()],
            },
            TicketBase {
                title: "Build storage & persistence".into(),
                objective: "Implement data storage, migrations, and queries".into(),
                ac: vec![
                    "Database schema created".into(),
                    "Migrations working".into(),
                ],
                capabilities: vec!["database".into(), "data-persistence".into()],
            },
            TicketBase {
                title: "Add authentication & authorization".into(),
                objective: "Implement user auth, roles, and permission checks".into(),
                ac: vec![
                    "Auth flow working".into(),
                    "Role-based access enforced".into(),
                ],
                capabilities: vec!["auth".into(), "security".into()],
            },
            TicketBase {
                title: "Create tests & CI pipeline".into(),
                objective: "Add automated tests and continuous integration".into(),
                ac: vec!["Unit tests passing".into(), "CI pipeline green".into()],
                capabilities: vec!["testing".into(), "ci-cd".into()],
            },
            TicketBase {
                title: "Write deployment & operations guide".into(),
                objective: "Document deployment steps and operational runbooks".into(),
                ac: vec![
                    "Deployment guide written".into(),
                    "Runbook documented".into(),
                ],
                capabilities: vec!["devops".into(), "documentation".into()],
            },
            TicketBase {
                title: "Configure monitoring & observability".into(),
                objective: "Set up logging, metrics, and alerting".into(),
                ac: vec![
                    "Logging configured".into(),
                    "Metrics dashboard created".into(),
                ],
                capabilities: vec!["observability".into(), "monitoring".into()],
            },
            TicketBase {
                title: "Performance optimization & load testing".into(),
                objective: "Profile, optimize, and validate performance targets".into(),
                ac: vec![
                    "Load tests passing".into(),
                    "Performance targets met".into(),
                ],
                capabilities: vec!["performance".into(), "testing".into()],
            },
        ],
        ArtifactIntent::Migrate => vec![
            TicketBase {
                title: "Audit current system & map dependencies".into(),
                objective: "Catalog existing system components, dependencies, and data flows"
                    .into(),
                ac: vec![
                    "System inventory created".into(),
                    "Dependency map documented".into(),
                ],
                capabilities: vec!["audit".into(), "dependency-analysis".into()],
            },
            TicketBase {
                title: "Design target architecture".into(),
                objective: "Define the target state architecture and migration plan".into(),
                ac: vec![
                    "Target architecture documented".into(),
                    "Migration strategy approved".into(),
                ],
                capabilities: vec!["architecture".into(), "planning".into()],
            },
            TicketBase {
                title: "Build data migration pipeline".into(),
                objective: "Create scripts and processes for data transformation and migration"
                    .into(),
                ac: vec!["Migration scripts ready".into(), "Dry-run validated".into()],
                capabilities: vec!["data-migration".into(), "etl".into()],
            },
            TicketBase {
                title: "Implement adapter/compatibility layer".into(),
                objective: "Build backward-compatible interfaces for gradual migration".into(),
                ac: vec![
                    "Adapter layer working".into(),
                    "Backward compatibility verified".into(),
                ],
                capabilities: vec!["api-design".into(), "integration".into()],
            },
            TicketBase {
                title: "Execute phased rollout".into(),
                objective: "Roll out changes incrementally with rollback capability".into(),
                ac: vec![
                    "Phased rollout plan ready".into(),
                    "Rollback procedures tested".into(),
                ],
                capabilities: vec!["devops".into(), "release-engineering".into()],
            },
            TicketBase {
                title: "Validate & decommission legacy".into(),
                objective: "Verify migration completeness and retire old system".into(),
                ac: vec![
                    "Validation tests passing".into(),
                    "Legacy system decommissioned".into(),
                ],
                capabilities: vec!["testing".into(), "operations".into()],
            },
        ],
        ArtifactIntent::Audit => vec![
            TicketBase {
                title: "Scope & define audit criteria".into(),
                objective: "Define the audit scope, standards, and evaluation criteria".into(),
                ac: vec![
                    "Audit scope defined".into(),
                    "Evaluation criteria documented".into(),
                ],
                capabilities: vec!["audit".into(), "risk-analysis".into()],
            },
            TicketBase {
                title: "Review code quality & architecture".into(),
                objective: "Analyze codebase for quality, patterns, and architectural issues"
                    .into(),
                ac: vec![
                    "Code review completed".into(),
                    "Architecture assessment written".into(),
                ],
                capabilities: vec!["code-review".into(), "architecture".into()],
            },
            TicketBase {
                title: "Assess security & compliance".into(),
                objective: "Evaluate security posture and regulatory compliance".into(),
                ac: vec![
                    "Security assessment done".into(),
                    "Compliance gaps documented".into(),
                ],
                capabilities: vec!["security".into(), "compliance".into()],
            },
            TicketBase {
                title: "Check performance & reliability".into(),
                objective: "Measure performance metrics and reliability indicators".into(),
                ac: vec![
                    "Performance baseline captured".into(),
                    "Reliability assessment done".into(),
                ],
                capabilities: vec!["performance".into(), "reliability".into()],
            },
            TicketBase {
                title: "Compile findings & remediation plan".into(),
                objective: "Document all findings with severity and recommended fixes".into(),
                ac: vec![
                    "Findings report written".into(),
                    "Remediation plan created".into(),
                ],
                capabilities: vec!["documentation".into(), "planning".into()],
            },
        ],
        ArtifactIntent::Design => vec![
            TicketBase {
                title: "Define system context & stakeholders".into(),
                objective: "Map system boundaries, actors, and external dependencies".into(),
                ac: vec![
                    "Context diagram created".into(),
                    "Stakeholders identified".into(),
                ],
                capabilities: vec!["architecture".into(), "domain-modeling".into()],
            },
            TicketBase {
                title: "Design component architecture".into(),
                objective: "Define components, interfaces, and data flow between them".into(),
                ac: vec![
                    "Component diagram done".into(),
                    "Interfaces specified".into(),
                ],
                capabilities: vec!["architecture".into(), "api-design".into()],
            },
            TicketBase {
                title: "Model data & state".into(),
                objective: "Design data models, state machines, and persistence strategy".into(),
                ac: vec![
                    "Data model documented".into(),
                    "State machine defined".into(),
                ],
                capabilities: vec!["data-modeling".into(), "database".into()],
            },
        ],
        ArtifactIntent::Research => vec![
            TicketBase {
                title: "Survey existing solutions & literature".into(),
                objective: "Review existing approaches, tools, and prior work".into(),
                ac: vec![
                    "Literature review done".into(),
                    "Solution landscape mapped".into(),
                ],
                capabilities: vec!["research".into(), "analysis".into()],
            },
            TicketBase {
                title: "Evaluate & compare alternatives".into(),
                objective: "Compare candidate solutions against defined criteria".into(),
                ac: vec![
                    "Comparison matrix created".into(),
                    "Top candidates identified".into(),
                ],
                capabilities: vec!["analysis".into(), "decision-making".into()],
            },
            TicketBase {
                title: "Produce recommendation report".into(),
                objective: "Compile research findings into a structured recommendation".into(),
                ac: vec![
                    "Recommendation written".into(),
                    "Decision rationale documented".into(),
                ],
                capabilities: vec!["documentation".into(), "presentation".into()],
            },
        ],
        ArtifactIntent::Operate => vec![
            TicketBase {
                title: "Define deployment architecture".into(),
                objective: "Design the deployment topology, infrastructure, and networking".into(),
                ac: vec![
                    "Deployment topology designed".into(),
                    "Infrastructure defined".into(),
                ],
                capabilities: vec!["devops".into(), "infrastructure".into()],
            },
            TicketBase {
                title: "Set up CI/CD pipeline".into(),
                objective: "Configure build, test, and deployment automation".into(),
                ac: vec![
                    "CI pipeline running".into(),
                    "CD pipeline configured".into(),
                ],
                capabilities: vec!["ci-cd".into(), "automation".into()],
            },
            TicketBase {
                title: "Create monitoring & alerting".into(),
                objective: "Set up observability stack with dashboards and alerts".into(),
                ac: vec!["Monitoring configured".into(), "Alert rules created".into()],
                capabilities: vec!["observability".into(), "monitoring".into()],
            },
            TicketBase {
                title: "Write runbooks & incident response".into(),
                objective: "Document operational procedures and incident response plans".into(),
                ac: vec![
                    "Runbooks written".into(),
                    "Incident response plan documented".into(),
                ],
                capabilities: vec!["documentation".into(), "operations".into()],
            },
        ],
        ArtifactIntent::Onboard => vec![
            TicketBase {
                title: "Create project overview & setup guide".into(),
                objective: "Write the getting-started guide and development environment setup"
                    .into(),
                ac: vec!["Setup guide written".into(), "Quickstart verified".into()],
                capabilities: vec!["documentation".into(), "developer-experience".into()],
            },
            TicketBase {
                title: "Document architecture & key decisions".into(),
                objective: "Explain system architecture, key decisions, and design rationale"
                    .into(),
                ac: vec![
                    "Architecture overview written".into(),
                    "ADRs documented".into(),
                ],
                capabilities: vec!["architecture".into(), "documentation".into()],
            },
            TicketBase {
                title: "Write contribution guidelines".into(),
                objective: "Document how to contribute, review, and release".into(),
                ac: vec![
                    "Contributing guide written".into(),
                    "Release process documented".into(),
                ],
                capabilities: vec!["documentation".into(), "community".into()],
            },
        ],
        ArtifactIntent::Mixed => vec![
            TicketBase {
                title: "Research & discovery".into(),
                objective: "Investigate the problem space and identify constraints".into(),
                ac: vec!["Research completed".into(), "Constraints documented".into()],
                capabilities: vec!["research".into(), "analysis".into()],
            },
            TicketBase {
                title: "Design solution architecture".into(),
                objective: "Produce architecture and design documents".into(),
                ac: vec!["Architecture designed".into(), "Design reviewed".into()],
                capabilities: vec!["architecture".into(), "design".into()],
            },
            TicketBase {
                title: "Implement core functionality".into(),
                objective: "Build the primary implementation".into(),
                ac: vec!["Core implemented".into(), "Tests passing".into()],
                capabilities: vec!["backend".into(), "frontend".into()],
            },
            TicketBase {
                title: "Configure operations & monitoring".into(),
                objective: "Set up deployment, monitoring, and operational tooling".into(),
                ac: vec!["Deployment configured".into(), "Monitoring active".into()],
                capabilities: vec!["devops".into(), "observability".into()],
            },
        ],
    }
}

fn detect_skills_required(tickets: &[DraftTicket], validated: &[String]) -> Vec<String> {
    let mut skills: Vec<String> = tickets
        .iter()
        .flat_map(|t| t.preferred_capabilities.clone())
        .collect();
    skills.sort();
    skills.dedup();

    for topic in validated {
        let sanitized = topic.to_lowercase().replace(' ', "-");
        if !skills.contains(&sanitized) {
            skills.push(sanitized);
        }
    }

    skills
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research::{ResearchStatus, generate_brief};

    fn sample_state() -> InitState {
        let mut state = InitState::new(
            "test-id".into(),
            "opencode".into(),
            "Build a REST API for order management".into(),
        );
        let mut reqs: Vec<Requirement> = vec![
            "Handle 10k orders/minute".into(),
            "Support OAuth2 authentication".into(),
            "PostgreSQL for persistence".into(),
            "RESTful API design".into(),
            "Docker container deployment".into(),
        ]
        .into_iter()
        .map(|t| Requirement {
            id: uuid::Uuid::new_v4().to_string(),
            text: t,
            source: RequirementSource::User,
            answered: false,
        })
        .collect();
        let more: Vec<Requirement> = vec![
            "API versioning strategy".into(),
            "Rate limiting per client".into(),
        ]
        .into_iter()
        .map(|t| Requirement {
            id: uuid::Uuid::new_v4().to_string(),
            text: t,
            source: RequirementSource::User,
            answered: false,
        })
        .collect();
        reqs.extend(more);
        state.requirements.items = reqs;
        state.classification = Some(Classification {
            intent: ArtifactIntent::Build,
            scope: ArtifactScope::Medium,
            audience: Audience::Developer,
            confidence: 0.92,
            reasoning: "Keywords: build, REST, API".into(),
            classified_at: Utc::now(),
        });
        state
    }

    fn sample_topic(topic: &str) -> ResearchTopic {
        let dir = std::env::temp_dir();
        let mut t = generate_brief(&dir, "test-session", topic).expect("brief");
        t.status = ResearchStatus::Validated;
        t
    }

    #[test]
    fn generates_plan_draft_with_tickets() {
        let state = sample_state();
        let topics = vec![
            sample_topic("REST API patterns"),
            sample_topic("PostgreSQL performance"),
        ];
        let draft = generate_plan_draft(&state, &topics, 8);
        assert!(!draft.tickets.is_empty());
        assert!(!draft.waves.is_empty());
        assert!(draft.title.contains("Build"));
        assert_eq!(draft.research_validated_topics.len(), 2);
    }

    #[test]
    fn ticket_count_respects_max() {
        let state = sample_state();
        let draft = generate_plan_draft(&state, &[], 3);
        assert!(draft.tickets.len() <= 3);
    }

    #[test]
    fn waves_created_for_multi_ticket_plans() {
        let state = sample_state();
        let draft = generate_plan_draft(&state, &[], 8);
        assert!(!draft.waves.is_empty());
        assert_eq!(
            draft
                .waves
                .iter()
                .map(|w| w.ticket_ids.len())
                .sum::<usize>(),
            draft.tickets.len()
        );
    }

    #[test]
    fn skills_detected_from_ticket_capabilities() {
        let state = sample_state();
        let draft = generate_plan_draft(&state, &[], 8);
        assert!(draft.skills_required.contains(&"api-design".to_string()));
        assert!(draft.skills_required.contains(&"backend".to_string()));
    }

    #[test]
    fn research_validated_topics_included() {
        let state = sample_state();
        let topics = vec![sample_topic("OAuth2 security")];
        let draft = generate_plan_draft(&state, &topics, 5);
        assert!(
            draft
                .research_validated_topics
                .contains(&"OAuth2 security".to_string())
        );
    }

    #[test]
    fn contradictions_count_in_draft() {
        let mut state = sample_state();
        state.contradictions.push(Contradiction {
            id: "c1".into(),
            topic: "test".into(),
            claim_a: "A says X".into(),
            claim_b: "B says Y".into(),
            sources_a: vec!["srcA".into()],
            sources_b: vec!["srcB".into()],
            detected_at: Utc::now(),
            resolved: false,
            resolution: None,
        });
        let draft = generate_plan_draft(&state, &[], 5);
        assert_eq!(draft.contradictions_open, 1);
    }

    #[test]
    fn plan_draft_has_unique_id() {
        let state = sample_state();
        let d1 = generate_plan_draft(&state, &[], 5);
        let d2 = generate_plan_draft(&state, &[], 5);
        assert_ne!(d1.id, d2.id);
    }

    #[test]
    fn generates_different_tickets_per_intent() {
        let mut state = sample_state();
        state.classification = Some(Classification {
            intent: ArtifactIntent::Migrate,
            scope: ArtifactScope::Small,
            audience: Audience::Developer,
            confidence: 0.9,
            reasoning: "migration".into(),
            classified_at: Utc::now(),
        });
        let draft = generate_plan_draft(&state, &[], 5);
        assert!(draft.title.contains("migrate"));
        assert!(
            draft
                .skills_required
                .contains(&"data-migration".to_string())
                || draft.skills_required.contains(&"planning".to_string())
        );
    }

    #[test]
    fn skills_include_validated_topics() {
        let state = sample_state();
        let topics = vec![sample_topic("ci-cd pipeline")];
        let draft = generate_plan_draft(&state, &topics, 5);
        assert!(draft.skills_required.iter().any(|s| s.contains("ci-cd")));
    }

    #[test]
    fn mid_scope_produces_reasonable_tickets() {
        let state = sample_state();
        let draft = generate_plan_draft(&state, &[], 8);
        assert!(draft.tickets.len() >= 3);
    }
}
