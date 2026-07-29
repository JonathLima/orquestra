use crate::output::{OutputData, print_output};
use clap::Subcommand;
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::{Plan, Ticket};
use orquestra_skills::{
    BrainCandidate, SkillInfo, adapt_local_skill, approve_candidate, brain_policy,
    external_discovery_disabled, inspect_candidate, read_inventory, reject_candidate,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum BrainAction {
    /// Show current BRAIN policy
    Policy,
    /// Create a project-local pending adaptation from an existing skill
    Adapt {
        /// JSON file containing either a Ticket or a Plan with one ticket
        #[arg(long)]
        ticket: String,
        /// Source skill name or ID from the current inventory
        #[arg(long = "from-skill")]
        from_skill: String,
    },
    /// Show a pending candidate's metadata and adapted SKILL.md
    Inspect { candidate_id: String },
    /// Approve a pending candidate into .orquestra/skills/
    Approve { candidate_id: String },
    /// Reject a pending candidate without installing it
    Reject { candidate_id: String },
    /// External discovery placeholder; disabled by default policy
    Search {
        /// Search query
        query: String,
    },
}

#[derive(Debug, Serialize)]
struct BrainPolicyOutput {
    external_discovery_enabled: bool,
}

impl OutputData for BrainPolicyOutput {
    fn render_human(&self) -> String {
        format!(
            "BRAIN policy:\n  externalDiscoveryEnabled: {}",
            self.external_discovery_enabled
        )
    }
}

#[derive(Debug, Serialize)]
struct BrainCandidateOutput {
    candidate: BrainCandidate,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

impl OutputData for BrainCandidateOutput {
    fn render_human(&self) -> String {
        let c = &self.candidate;
        let mut rendered = format!(
            "Candidate: {}\nSkill: {}\nTicket: {}\nStatus: {:?}\nPath: {}",
            c.id,
            c.skill_name,
            c.ticket_id,
            c.status,
            c.path.display()
        );
        if let Some(content) = &self.content {
            rendered.push_str("\n\n--- Adapted SKILL.md ---\n");
            rendered.push_str(content);
        }
        rendered
    }
}

pub fn run(action: &BrainAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        BrainAction::Policy => {
            let policy = brain_policy();
            print_output(
                &BrainPolicyOutput {
                    external_discovery_enabled: policy.external_discovery_enabled,
                },
                output,
            );
            Ok(())
        }
        BrainAction::Adapt { ticket, from_skill } => run_adapt(ticket, from_skill, output),
        BrainAction::Inspect { candidate_id } => {
            let candidate = inspect_candidate(&project_dir(), candidate_id)?;
            let skill_path = candidate.path.join("SKILL.md");
            let content = std::fs::read_to_string(&skill_path).map_err(|error| {
                OrquestraError::from(format!(
                    "Cannot read adapted skill '{}': {error}",
                    skill_path.display()
                ))
            })?;
            print_output(
                &BrainCandidateOutput {
                    candidate,
                    content: Some(content),
                },
                output,
            );
            Ok(())
        }
        BrainAction::Approve { candidate_id } => {
            let candidate = approve_candidate(&project_dir(), candidate_id)?;
            print_output(
                &BrainCandidateOutput {
                    candidate,
                    content: None,
                },
                output,
            );
            Ok(())
        }
        BrainAction::Reject { candidate_id } => {
            let candidate = reject_candidate(&project_dir(), candidate_id)?;
            print_output(
                &BrainCandidateOutput {
                    candidate,
                    content: None,
                },
                output,
            );
            Ok(())
        }
        BrainAction::Search { query: _ } => Err(external_discovery_disabled()),
    }
}

fn run_adapt(
    ticket_file: &str,
    from_skill: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let ticket = load_single_ticket(ticket_file)?;
    let source_skill = find_skill(from_skill)?;
    let candidate = adapt_local_skill(&project_dir(), &ticket, &source_skill)?;
    print_output(
        &BrainCandidateOutput {
            candidate,
            content: None,
        },
        output,
    );
    Ok(())
}

fn load_single_ticket(path: &str) -> Result<Ticket, OrquestraError> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| OrquestraError::from(format!("Cannot read ticket file: {error}")))?;
    if let Ok(ticket) = serde_json::from_str::<Ticket>(&content) {
        return Ok(ticket);
    }
    let plan: Plan = serde_json::from_str(&content)
        .map_err(|error| OrquestraError::from(format!("Cannot parse ticket or plan: {error}")))?;
    if plan.tickets.len() != 1 {
        return Err(OrquestraError::from(format!(
            "Expected one ticket, found {}. Pass a single-ticket file.",
            plan.tickets.len()
        )));
    }
    Ok(plan.tickets.into_iter().next().expect("one ticket"))
}

fn find_skill(name_or_id: &str) -> Result<SkillInfo, OrquestraError> {
    let inventory = read_inventory()?.ok_or_else(|| {
        OrquestraError::from("No inventory found. Run 'orquestra skill scan' first.")
    })?;
    inventory
        .skills
        .into_iter()
        .find(|skill| skill.name == name_or_id || skill.id == name_or_id)
        .ok_or_else(|| {
            OrquestraError::from(format!("Skill '{name_or_id}' not found in inventory."))
        })
}

fn project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
