use crate::output::{OutputData, print_output};
use clap::Subcommand;
use orquestra_adapters::get_adapter;
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::{load_plan, model::recommend_for_plan};
use orquestra_runtime::{
    ResearchReport, ResearchValidation, save_research_report, validate_research_report,
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum ResearchAction {
    /// Generate a host-specific research brief for a ticket
    Brief {
        /// Plan file containing the ticket
        #[arg(long)]
        ticket: String,
        /// Ticket ID inside the plan
        #[arg(long = "ticket-id")]
        ticket_id: String,
        /// Host name (codex, claude-code, opencode, antigravity)
        #[arg(long, default_value = "codex")]
        host: String,
    },
    /// Validate a completed research report
    Validate {
        /// Research report JSON
        #[arg(long)]
        report: String,
    },
    /// Validate and store a research report into .orquestra/research and .orquestra/memory
    Store {
        /// Research report JSON
        #[arg(long)]
        report: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResearchBriefOutput {
    ticket_id: String,
    host: String,
    current_date: String,
    web_required: bool,
    minimum_sources: u8,
    primary_source_required: bool,
    validation_rule: String,
    tool_hints: BTreeMap<String, String>,
    instructions: Vec<String>,
}

impl OutputData for ResearchBriefOutput {
    fn render_human(&self) -> String {
        let mut out = format!(
            "Research brief for {}\nHost: {}\nCurrent date: {}\nMinimum sources: {}\nPrimary source required: {}\nWeb required: {}\n",
            self.ticket_id,
            self.host,
            self.current_date,
            self.minimum_sources,
            self.primary_source_required,
            self.web_required
        );
        out.push_str("\nTool hints:\n");
        for (name, hint) in &self.tool_hints {
            out.push_str(&format!("  {name}: {hint}\n"));
        }
        out.push_str("\nInstructions:\n");
        for instruction in &self.instructions {
            out.push_str(&format!("  - {instruction}\n"));
        }
        out
    }
}

impl OutputData for ResearchValidation {
    fn render_human(&self) -> String {
        if self.valid {
            format!(
                "Research report valid for {} ({} claims)",
                self.ticket_id, self.validated_claims
            )
        } else {
            format!(
                "Research report invalid for {}: {}",
                self.ticket_id,
                self.errors.join("; ")
            )
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResearchStoredOutput {
    ticket_id: String,
    validation: ResearchValidation,
}

impl OutputData for ResearchStoredOutput {
    fn render_human(&self) -> String {
        format!("Stored research report for {}", self.ticket_id)
    }
}

pub fn run(action: &ResearchAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        ResearchAction::Brief {
            ticket,
            ticket_id,
            host,
        } => run_brief(ticket, ticket_id, host, output),
        ResearchAction::Validate { report } => run_validate(report, output),
        ResearchAction::Store { report } => run_store(report, output),
    }
}

fn run_brief(
    plan_file: &str,
    ticket_id: &str,
    host: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let plan = load_plan(plan_file)
        .map_err(|error| OrquestraError::from(format!("Cannot load plan: {error}")))?;
    let recommendation = recommend_for_plan(&plan, Some(ticket_id), Some(host))
        .map_err(|error| OrquestraError::from(format!("Cannot recommend model: {error}")))?;
    let adapter =
        get_adapter(host).ok_or_else(|| OrquestraError::from(format!("Unknown host: {host}")))?;
    let tool_map = adapter.tool_map();
    let tool_hints = ["webSearch", "webFetch"]
        .into_iter()
        .filter_map(|key| {
            tool_map
                .get(key)
                .map(|value| (key.to_string(), value.to_string()))
        })
        .collect::<BTreeMap<_, _>>();

    let brief = ResearchBriefOutput {
        ticket_id: ticket_id.to_string(),
        host: host.to_string(),
        current_date: chrono::Utc::now().date_naive().to_string(),
        web_required: recommendation.web_required,
        minimum_sources: 2,
        primary_source_required: true,
        validation_rule:
            "Use at least 2 supporting sources, including a primary source when one exists."
                .to_string(),
        tool_hints,
        instructions: vec![
            "Search with the current date included in the query.".to_string(),
            "Prefer official documentation for primary claims.".to_string(),
            "Cross-check every claim against at least one independent source.".to_string(),
            "Record unresolved conflicts instead of hiding them.".to_string(),
            "Save findings as a research report and run `orquestra research validate`.".to_string(),
        ],
    };
    print_output(&brief, output);
    Ok(())
}

fn run_validate(report: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let report = load_report(report)?;
    let validation = validate_research_report(&report);
    if !validation.valid {
        return Err(OrquestraError::from(validation.errors.join("; ")));
    }
    print_output(&validation, output);
    Ok(())
}

fn run_store(report: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let report = load_report(report)?;
    let validation = save_research_report(&project_dir(), &report)
        .map_err(|error| OrquestraError::from(format!("Cannot store research report: {error}")))?;
    print_output(
        &ResearchStoredOutput {
            ticket_id: report.ticket_id,
            validation,
        },
        output,
    );
    Ok(())
}

fn load_report(path: &str) -> Result<ResearchReport, OrquestraError> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| OrquestraError::from(format!("Cannot read report: {error}")))?;
    serde_json::from_str(&content)
        .map_err(|error| OrquestraError::from(format!("Cannot parse report: {error}")))
}

fn project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
