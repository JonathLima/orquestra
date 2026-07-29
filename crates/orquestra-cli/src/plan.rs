use crate::cli::PlanAction;
use crate::output::{OutputData, print_output};
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::{
    ValidationResult, WaveResult, derive_waves, explain_plan, load_plan, validate_plan,
};
use orquestra_skills::{SkillStatus, read_inventory};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct PlanValidationOutput {
    result: ValidationResult,
}

impl OutputData for PlanValidationOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        if self.result.valid {
            out.push_str("Plan is **valid**.\n");
        } else {
            out.push_str(&format!(
                "Plan has {} errors:\n\n",
                self.result.errors.len()
            ));
            for err in &self.result.errors {
                match &err.ticket_id {
                    Some(tid) => {
                        out.push_str(&format!("  [{}] {}: {}\n", err.code, tid, err.message))
                    }
                    None => out.push_str(&format!("  [{}] {}\n", err.code, err.message)),
                }
            }
        }
        if !self.result.warnings.is_empty() {
            out.push_str("\nWarnings:\n");
            for w in &self.result.warnings {
                out.push_str(&format!("  {w}\n"));
            }
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct PlanWavesOutput {
    result: WaveResult,
}

impl OutputData for PlanWavesOutput {
    fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Waves: {}\n\n", self.result.total_waves));
        for wave in &self.result.waves {
            out.push_str(&format!(
                "Wave {}: {}\n",
                wave.wave_number,
                wave.ticket_ids.join(", ")
            ));
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct PlanExplainOutput {
    details: String,
}

impl OutputData for PlanExplainOutput {
    fn render_human(&self) -> String {
        self.details.clone()
    }
}

pub fn run(action: &PlanAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        PlanAction::Validate { plan_file } => run_validate(plan_file, output),
        PlanAction::Waves { plan_file } => run_waves(plan_file, output),
        PlanAction::Explain { plan_file } => run_explain(plan_file, output),
        PlanAction::Export { plan_file, format } => run_export(plan_file, format, output),
    }
}

fn run_validate(plan_file: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let plan =
        load_plan(plan_file).map_err(|e| OrquestraError::from(format!("Cannot load plan: {e}")))?;
    let mut result = validate_plan(&plan);
    validate_inventory_skills(&plan, &mut result)?;
    let valid = result.valid;
    print_output(&PlanValidationOutput { result }, output);
    if !valid {
        return Err(OrquestraError::from("plan validation failed"));
    }
    Ok(())
}

pub(crate) fn validate_inventory_skills(
    plan: &orquestra_plan::Plan,
    result: &mut ValidationResult,
) -> Result<(), OrquestraError> {
    let Some(inventory) = read_inventory()? else {
        return Ok(());
    };
    for ticket in &plan.tickets {
        let Some(skill_name) = ticket.assigned_skill.as_deref() else {
            continue;
        };
        let matched = inventory.skills.iter().any(|skill| {
            skill.status == SkillStatus::Active
                && (skill.name.eq_ignore_ascii_case(skill_name)
                    || skill.id.eq_ignore_ascii_case(skill_name))
        });
        if !matched {
            result.errors.push(orquestra_plan::ValidationError {
                code: "unknown_assigned_skill".to_string(),
                message: format!(
                    "Ticket {} assignedSkill '{}' is not active in .orquestra/skills_inventory.json",
                    ticket.id, skill_name
                ),
                ticket_id: Some(ticket.id.clone()),
            });
        }
    }
    result.valid = result.errors.is_empty();
    Ok(())
}

fn run_waves(plan_file: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let plan =
        load_plan(plan_file).map_err(|e| OrquestraError::from(format!("Cannot load plan: {e}")))?;
    let result = derive_waves(&plan)
        .map_err(|e| OrquestraError::from(format!("Cannot derive waves: {e}")))?;
    print_output(&PlanWavesOutput { result }, output);
    Ok(())
}

fn run_explain(plan_file: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let plan =
        load_plan(plan_file).map_err(|e| OrquestraError::from(format!("Cannot load plan: {e}")))?;
    let validation = validate_plan(&plan);
    let wave_result = if validation.valid {
        derive_waves(&plan).unwrap_or(WaveResult {
            waves: vec![],
            total_waves: 0,
            total_tickets: plan.tickets.len(),
        })
    } else {
        WaveResult {
            waves: vec![],
            total_waves: 0,
            total_tickets: plan.tickets.len(),
        }
    };
    let details = explain_plan(&plan, &wave_result, &validation);
    print_output(&PlanExplainOutput { details }, output);
    Ok(())
}

fn run_export(plan_file: &str, format: &str, _output: &OutputFormat) -> Result<(), OrquestraError> {
    let plan =
        load_plan(plan_file).map_err(|e| OrquestraError::from(format!("Cannot load plan: {e}")))?;
    match format {
        "json" => {
            let json = serde_json::to_string_pretty(&plan)
                .map_err(|e| OrquestraError::from(format!("Cannot serialize: {e}")))?;
            println!("{json}");
        }
        "md" => {
            let validation = validate_plan(&plan);
            let waves = if validation.valid {
                derive_waves(&plan).ok()
            } else {
                None
            };
            let wr = waves.unwrap_or(WaveResult {
                waves: vec![],
                total_waves: 0,
                total_tickets: plan.tickets.len(),
            });
            let details = explain_plan(&plan, &wr, &validation);
            println!("{details}");
        }
        other => {
            return Err(OrquestraError::from(format!(
                "Unknown format: {other}. Use json or md"
            )));
        }
    }
    Ok(())
}
