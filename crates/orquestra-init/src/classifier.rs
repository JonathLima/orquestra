use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::state::{ArtifactIntent, ArtifactScope, Audience, Classification, Requirements};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefinementRequest {
    pub idea: String,
    pub heuristic_intent: ArtifactIntent,
    pub heuristic_scope: ArtifactScope,
    pub heuristic_audience: Audience,
    pub heuristic_confidence: f32,
    pub requirements_count: usize,
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefinementResponse {
    pub intent: ArtifactIntent,
    pub scope: ArtifactScope,
    pub audience: Audience,
    pub confidence: f32,
    pub reasoning: String,
}

pub fn merge_classifications(
    heuristic: &Classification,
    refinement: Option<&RefinementResponse>,
) -> Classification {
    let Some(r) = refinement else {
        return heuristic.clone();
    };

    let intent = if r.confidence > heuristic.confidence {
        r.intent
    } else {
        heuristic.intent
    };
    let scope = if r.confidence > heuristic.confidence {
        r.scope
    } else {
        heuristic.scope
    };
    let audience = if r.confidence > heuristic.confidence {
        r.audience
    } else {
        heuristic.audience
    };
    let confidence = heuristic.confidence.max(r.confidence);
    let reasoning = format!(
        "heuristic: {}; refinement: {}",
        heuristic.reasoning, r.reasoning
    );

    Classification {
        intent,
        scope,
        audience,
        confidence,
        reasoning,
        classified_at: chrono::Utc::now(),
    }
}

pub struct HeuristicClassifier;

impl HeuristicClassifier {
    pub fn classify(idea: &str, requirements: &Requirements) -> Classification {
        let (intent, intent_conf, intent_reason) = Self::detect_intent(idea);
        let scope = Self::detect_scope(requirements);
        let audience = Self::detect_audience(idea);

        let confidence = intent_conf.clamp(0.0, 1.0);

        Classification {
            intent,
            scope,
            audience,
            confidence,
            reasoning: intent_reason,
            classified_at: chrono::Utc::now(),
        }
    }

    fn detect_intent(idea: &str) -> (ArtifactIntent, f32, String) {
        let lower = idea.to_lowercase();
        let signals = Self::intent_keywords();
        let mut results: Vec<(ArtifactIntent, f32, Vec<String>)> = Vec::new();

        for (intent, keywords) in &signals {
            let mut score = 0.0f32;
            let mut matched = Vec::new();
            for (keyword, weight) in keywords {
                if lower.contains(*keyword) {
                    score += weight;
                    matched.push(keyword.to_string());
                }
            }
            if score > 0.0 {
                results.push((*intent, score, matched));
            }
        }

        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((intent, score, matched)) = results.first() {
            let reason = format!(
                "intent={:?} matched keywords: {} (score={:.2})",
                intent,
                matched.join(", "),
                score
            );

            if results.len() > 1 {
                let second = &results[1];
                let ratio = second.1 / score.max(0.01);
                if ratio > 0.6 {
                    return (
                        ArtifactIntent::Mixed,
                        score.min(0.92),
                        format!(
                            "mixed intent: {:?}({:.2}) and {:?}({:.2})",
                            intent, score, second.0, second.1
                        ),
                    );
                }
            }

            let confidence = (score / 3.0).min(1.0);
            (*intent, confidence, reason)
        } else {
            (
                ArtifactIntent::Mixed,
                0.3,
                "no clear intent detected".to_string(),
            )
        }
    }

    fn intent_keywords() -> HashMap<ArtifactIntent, Vec<(&'static str, f32)>> {
        let mut map: HashMap<ArtifactIntent, Vec<(&'static str, f32)>> = HashMap::new();

        map.insert(
            ArtifactIntent::Design,
            vec![
                ("design", 2.0),
                ("architect", 2.0),
                ("blueprint", 2.0),
                ("architecture", 1.5),
                ("component", 1.0),
                ("system", 0.8),
                ("model", 0.8),
                ("schema", 1.0),
                ("diagram", 1.5),
                ("pipeline", 0.8),
                ("projetar", 2.0),
                ("projeto", 1.5),
                ("arquitetura", 2.0),
                ("componente", 1.0),
                ("fluxo", 0.8),
            ],
        );

        map.insert(
            ArtifactIntent::Build,
            vec![
                ("build", 2.0),
                ("implement", 2.0),
                ("create", 1.5),
                ("develop", 1.5),
                ("feature", 1.0),
                ("code", 1.0),
                ("function", 1.0),
                ("endpoint", 1.0),
                ("api", 0.8),
                ("implementar", 2.0),
                ("criar", 1.5),
                ("construir", 1.5),
                ("desenvolver", 1.5),
                ("funcionalidade", 1.0),
                ("endpoint", 1.0),
            ],
        );

        map.insert(
            ArtifactIntent::Migrate,
            vec![
                ("migrate", 2.0),
                ("modernize", 2.0),
                ("upgrade", 2.0),
                ("refactor", 2.0),
                ("port", 1.5),
                ("convert", 1.5),
                ("legacy", 1.5),
                ("move", 0.8),
                ("migrar", 2.0),
                ("modernizar", 2.0),
                ("modernização", 2.0),
                ("legado", 1.5),
                ("legada", 1.5),
                ("refatorar", 2.0),
                ("refatoração", 2.0),
                ("converter", 1.5),
                ("atualizar", 1.5),
            ],
        );

        map.insert(
            ArtifactIntent::Audit,
            vec![
                ("audit", 2.0),
                ("review", 2.0),
                ("assess", 2.0),
                ("evaluate", 1.5),
                ("inspect", 1.5),
                ("check", 1.0),
                ("compliance", 2.0),
                ("security", 1.0),
                ("auditar", 2.0),
                ("auditoria", 2.0),
                ("revisar", 1.5),
                ("revisão", 1.5),
                ("avaliar", 1.5),
                ("avaliação", 1.5),
                ("inspecionar", 1.5),
                ("conformidade", 2.0),
                ("segurança", 1.0),
            ],
        );

        map.insert(
            ArtifactIntent::Research,
            vec![
                ("research", 2.0),
                ("investigate", 2.0),
                ("explore", 2.0),
                ("compare", 1.5),
                ("analyze", 1.5),
                ("study", 1.0),
                ("benchmark", 1.5),
                ("pesquisar", 2.0),
                ("pesquisa", 2.0),
                ("investigar", 2.0),
                ("investigação", 2.0),
                ("explorar", 2.0),
                ("comparar", 1.5),
                ("comparação", 1.5),
                ("analisar", 1.5),
                ("análise", 1.5),
            ],
        );

        map.insert(
            ArtifactIntent::Operate,
            vec![
                ("deploy", 2.0),
                ("operate", 2.0),
                ("run", 1.5),
                ("host", 1.5),
                ("monitor", 1.5),
                ("observability", 2.0),
                ("ci/cd", 1.5),
                ("implantar", 2.0),
                ("implantação", 2.0),
                ("operar", 2.0),
                ("monitorar", 1.5),
                ("monitoramento", 1.5),
                ("observabilidade", 2.0),
            ],
        );

        map.insert(
            ArtifactIntent::Onboard,
            vec![
                ("onboard", 2.0),
                ("document", 2.0),
                ("explain", 2.0),
                ("guide", 1.5),
                ("tutorial", 1.5),
                ("setup", 1.0),
                ("how-to", 1.5),
                ("documentar", 2.0),
                ("documentação", 2.0),
                ("explicar", 2.0),
                ("guiar", 1.0),
            ],
        );

        map
    }

    fn detect_scope(requirements: &Requirements) -> ArtifactScope {
        let count = requirements.items.len();
        if count <= 5 {
            ArtifactScope::Small
        } else if count <= 15 {
            ArtifactScope::Medium
        } else {
            ArtifactScope::Large
        }
    }

    fn detect_audience(idea: &str) -> Audience {
        let lower = idea.to_lowercase();

        let stakeholder_signals = [
            "stakeholder",
            "executive",
            "roi",
            "budget",
            "timeline",
            "business",
            "non-technical",
        ];
        let ops_signals = [
            "deploy",
            "k8s",
            "kubernetes",
            "monitoring",
            "ops",
            "sre",
            "runbook",
            "incident",
        ];
        let dev_signals = [
            "code",
            "api",
            "endpoint",
            "library",
            "sdk",
            "framework",
            "test",
            "ci",
        ];

        let stakeholder_score = stakeholder_signals
            .iter()
            .filter(|s| lower.contains(**s))
            .count() as f32
            * 2.0;
        let ops_score = ops_signals.iter().filter(|s| lower.contains(**s)).count() as f32 * 2.0;
        let dev_score = dev_signals.iter().filter(|s| lower.contains(**s)).count() as f32;

        if stakeholder_score > ops_score
            && stakeholder_score > dev_score
            && stakeholder_score >= 1.0
        {
            Audience::Stakeholder
        } else if ops_score > dev_score && ops_score > 1.0 {
            Audience::Operations
        } else {
            Audience::Developer
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Requirement, RequirementSource};

    fn empty_req() -> Requirements {
        Requirements::default()
    }

    fn reqs(count: usize) -> Requirements {
        let items: Vec<Requirement> = (0..count)
            .map(|i| Requirement {
                id: format!("R{}", i + 1),
                text: format!("requirement {}", i + 1),
                source: RequirementSource::User,
                answered: true,
            })
            .collect();
        Requirements { items }
    }

    #[test]
    fn classify_design_idea() {
        let result =
            HeuristicClassifier::classify("design a microservices architecture", &empty_req());
        assert_eq!(result.intent, ArtifactIntent::Design);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn classify_build_idea() {
        let result = HeuristicClassifier::classify("build a REST API for tasks", &empty_req());
        assert_eq!(result.intent, ArtifactIntent::Build);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn classify_migrate_idea() {
        let result = HeuristicClassifier::classify("migrate from Express to Fastify", &empty_req());
        assert_eq!(result.intent, ArtifactIntent::Migrate);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn classify_audit_idea() {
        let result = HeuristicClassifier::classify("audit security of this system", &empty_req());
        assert_eq!(result.intent, ArtifactIntent::Audit);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn classify_research_idea() {
        let result =
            HeuristicClassifier::classify("research best auth approach for 2026", &empty_req());
        assert_eq!(result.intent, ArtifactIntent::Research);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn classify_modernize_legacy_idea() {
        let result = HeuristicClassifier::classify(
            "modernizar API Express legada + CI/CD seguro",
            &empty_req(),
        );
        assert_eq!(result.intent, ArtifactIntent::Migrate);
        assert!(result.confidence > 0.5);
    }

    #[test]
    fn classify_scope_small_for_few_requirements() {
        let result = HeuristicClassifier::classify("build something", &reqs(3));
        assert_eq!(result.scope, ArtifactScope::Small);
    }

    #[test]
    fn classify_scope_medium_for_moderate_requirements() {
        let result = HeuristicClassifier::classify("build something", &reqs(10));
        assert_eq!(result.scope, ArtifactScope::Medium);
    }

    #[test]
    fn classify_scope_large_for_many_requirements() {
        let result = HeuristicClassifier::classify("build something", &reqs(20));
        assert_eq!(result.scope, ArtifactScope::Large);
    }

    #[test]
    fn classify_audience_developer_by_default() {
        let result = HeuristicClassifier::classify("build an API", &empty_req());
        assert_eq!(result.audience, Audience::Developer);
    }

    #[test]
    fn classify_audience_operations_on_deploy_keywords() {
        let result = HeuristicClassifier::classify("deploy to k8s with monitoring", &empty_req());
        assert_eq!(result.audience, Audience::Operations);
    }

    #[test]
    fn classify_audience_stakeholder_on_business_keywords() {
        let result = HeuristicClassifier::classify(
            "stakeholder report on budget and timeline",
            &empty_req(),
        );
        assert_eq!(result.audience, Audience::Stakeholder);
    }

    #[test]
    fn detect_intent_no_keywords_returns_mixed() {
        let (intent, conf, reason) =
            HeuristicClassifier::detect_intent("something completely different");
        assert_eq!(intent, ArtifactIntent::Mixed);
        assert!(conf < 0.5);
        assert!(reason.contains("no clear intent"));
    }

    #[test]
    fn detect_intent_mixed_when_two_intents_close() {
        let (intent, _, _) = HeuristicClassifier::detect_intent("design and build a system");
        assert_eq!(intent, ArtifactIntent::Mixed);
    }

    #[test]
    fn classification_has_current_date() {
        let result = HeuristicClassifier::classify("build test", &empty_req());
        let now = chrono::Utc::now();
        let diff = now - result.classified_at;
        assert!(diff.num_seconds() < 10);
    }

    #[test]
    fn classify_confidence_is_bounded() {
        let result = HeuristicClassifier::classify("build", &empty_req());
        assert!(result.confidence >= 0.0);
        assert!(result.confidence <= 1.0);
    }

    #[test]
    fn merge_uses_refinement_when_more_confident() {
        let heuristic = Classification {
            intent: ArtifactIntent::Build,
            scope: ArtifactScope::Medium,
            audience: Audience::Developer,
            confidence: 0.7,
            reasoning: "keywords: build".into(),
            classified_at: chrono::Utc::now(),
        };
        let refinement = RefinementResponse {
            intent: ArtifactIntent::Migrate,
            scope: ArtifactScope::Large,
            audience: Audience::Stakeholder,
            confidence: 0.9,
            reasoning: "LLM says migrate".into(),
        };
        let merged = merge_classifications(&heuristic, Some(&refinement));
        assert_eq!(merged.intent, ArtifactIntent::Migrate);
        assert_eq!(merged.confidence, 0.9);
        assert!(merged.reasoning.contains("heuristic"));
        assert!(merged.reasoning.contains("refinement"));
    }

    #[test]
    fn merge_keeps_heuristic_when_refinement_less_confident() {
        let heuristic = Classification {
            intent: ArtifactIntent::Build,
            scope: ArtifactScope::Small,
            audience: Audience::Developer,
            confidence: 0.9,
            reasoning: "clear build keywords".into(),
            classified_at: chrono::Utc::now(),
        };
        let refinement = RefinementResponse {
            intent: ArtifactIntent::Audit,
            scope: ArtifactScope::Large,
            audience: Audience::Stakeholder,
            confidence: 0.6,
            reasoning: "LLM guess".into(),
        };
        let merged = merge_classifications(&heuristic, Some(&refinement));
        assert_eq!(merged.intent, ArtifactIntent::Build);
        assert_eq!(merged.confidence, 0.9);
    }

    #[test]
    fn merge_falls_back_to_heuristic_on_none() {
        let heuristic = Classification {
            intent: ArtifactIntent::Research,
            scope: ArtifactScope::Small,
            audience: Audience::Developer,
            confidence: 0.8,
            reasoning: "research keywords".into(),
            classified_at: chrono::Utc::now(),
        };
        let merged = merge_classifications(&heuristic, None);
        assert_eq!(merged.intent, ArtifactIntent::Research);
    }

    #[test]
    fn refinement_request_serializes() {
        let req = RefinementRequest {
            idea: "build a CLI tool".into(),
            heuristic_intent: ArtifactIntent::Build,
            heuristic_scope: ArtifactScope::Small,
            heuristic_audience: Audience::Developer,
            heuristic_confidence: 0.75,
            requirements_count: 3,
            date: "2026-07-28".into(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("CLI tool"));
        let back: RefinementRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.idea, "build a CLI tool");
    }
}
