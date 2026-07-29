#![allow(clippy::result_large_err)]

pub mod error;
pub mod explain;
pub mod model;
pub mod types;
pub mod validate;
pub mod wave;

pub use error::PlanError;
pub use types::{
    ModelPolicy, ModelRecommendation, ModelTier, Plan, ReasoningEffort, Ticket, ValidationError,
    ValidationResult, VerificationPolicy, Wave, WaveResult,
};
pub use validate::validate_plan;
pub use wave::derive_waves;

pub fn load_plan(path: &str) -> Result<Plan, PlanError> {
    let content = std::fs::read_to_string(path)?;
    let plan: Plan = serde_json::from_str(&content)?;
    Ok(plan)
}

pub fn explain_plan(plan: &Plan, waves: &WaveResult, validation: &ValidationResult) -> String {
    crate::explain::explain_plan(plan, waves, validation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    fn valid_plan() -> Plan {
        Plan {
            schema_version: 1,
            title: "Test Plan".to_string(),
            model_policy: None,
            tickets: vec![
                Ticket {
                    id: "T1".to_string(),
                    title: "Task 1".to_string(),
                    objective: "Do thing 1".to_string(),
                    acceptance_criteria: vec!["C1 done".to_string()],
                    blocked_by: vec![],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("test-skill".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
                Ticket {
                    id: "T2".to_string(),
                    title: "Task 2".to_string(),
                    objective: "Do thing 2".to_string(),
                    acceptance_criteria: vec!["C2 done".to_string()],
                    blocked_by: vec!["T1".to_string()],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("test-skill".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
                Ticket {
                    id: "T3".to_string(),
                    title: "Task 3".to_string(),
                    objective: "Do thing 3".to_string(),
                    acceptance_criteria: vec!["C3 done".to_string()],
                    blocked_by: vec!["T2".to_string()],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("test-skill".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
            ],
        }
    }

    #[test]
    fn test_valid_plan_passes() {
        let plan = valid_plan();
        let result = validate_plan(&plan);
        assert!(
            result.valid,
            "Expected valid plan, got errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_duplicate_id_detected() {
        let mut plan = valid_plan();
        let mut t3 = plan.tickets[2].clone();
        t3.id = "T1".to_string();
        plan.tickets.push(t3);
        let result = validate_plan(&plan);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "duplicate_id"));
    }

    #[test]
    fn test_missing_dependency_detected() {
        let mut plan = valid_plan();
        plan.tickets[0].blocked_by.push("NONEXISTENT".to_string());
        let result = validate_plan(&plan);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "missing_dependency"));
    }

    #[test]
    fn test_cycle_detected() {
        let mut plan = valid_plan();
        plan.tickets[0].blocked_by.push("T3".to_string());
        let result = validate_plan(&plan);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "cycle_detected"));
    }

    #[test]
    fn test_score_out_of_range() {
        let mut plan = valid_plan();
        plan.tickets[0].verification.minimum_score = 1.5;
        let result = validate_plan(&plan);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "invalid_score"));
    }

    #[test]
    fn test_missing_objective() {
        let mut plan = valid_plan();
        plan.tickets[0].objective = String::new();
        let result = validate_plan(&plan);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "missing_objective"));
    }

    #[test]
    fn test_missing_criteria() {
        let mut plan = valid_plan();
        plan.tickets[0].acceptance_criteria = vec![];
        let result = validate_plan(&plan);
        assert!(!result.valid);
        assert!(result.errors.iter().any(|e| e.code == "missing_criteria"));
    }

    #[test]
    fn test_missing_assigned_skill() {
        let plan = Plan {
            schema_version: 1,
            title: "Missing skill".to_string(),
            model_policy: None,
            tickets: vec![Ticket {
                id: "T1".to_string(),
                title: "Task".to_string(),
                objective: "Implement safely".to_string(),
                acceptance_criteria: vec!["done".to_string()],
                blocked_by: vec![],
                preferred_capabilities: vec![],
                assigned_skill: None,
                model_policy: None,
                model_recommendation: None,
                verification: VerificationPolicy::default(),
            }],
        };

        let result = validate_plan(&plan);

        assert!(!result.valid);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.code == "missing_assigned_skill")
        );
    }

    #[test]
    fn test_wave_derivation_chained() {
        let plan = valid_plan();
        let waves = derive_waves(&plan).unwrap();
        assert_eq!(waves.total_waves, 3);
        assert_eq!(waves.waves[0].wave_number, 1);
        assert!(waves.waves[0].ticket_ids.contains(&"T1".to_string()));
        assert!(waves.waves[1].ticket_ids.contains(&"T2".to_string()));
        assert!(waves.waves[2].ticket_ids.contains(&"T3".to_string()));
    }

    #[test]
    fn test_wave_derivation_independent() {
        let plan = Plan {
            schema_version: 1,
            title: "Parallel".to_string(),
            model_policy: None,
            tickets: vec![
                Ticket {
                    id: "T1".to_string(),
                    title: "A".to_string(),
                    objective: "A".to_string(),
                    acceptance_criteria: vec!["done".to_string()],
                    blocked_by: vec![],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("test-skill".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
                Ticket {
                    id: "T2".to_string(),
                    title: "B".to_string(),
                    objective: "B".to_string(),
                    acceptance_criteria: vec!["done".to_string()],
                    blocked_by: vec![],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("test-skill".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
                Ticket {
                    id: "T3".to_string(),
                    title: "C".to_string(),
                    objective: "C".to_string(),
                    acceptance_criteria: vec!["done".to_string()],
                    blocked_by: vec![],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("test-skill".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
            ],
        };
        let waves = derive_waves(&plan).unwrap();
        assert_eq!(waves.total_waves, 1);
        assert_eq!(waves.waves[0].ticket_ids.len(), 3);
    }
}
