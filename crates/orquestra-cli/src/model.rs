use crate::output::{OutputData, print_output};
use clap::Subcommand;
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::{
    ModelRecommendation, load_plan,
    model::{ModelCatalog, catalog_for_host, recommend_for_plan},
};
use orquestra_runtime::{load_session, storage};
use serde::Serialize;

#[derive(Debug, Subcommand)]
pub enum ModelAction {
    /// Show model catalog for one host
    Catalog {
        /// Host name (codex, claude-code, opencode, antigravity)
        #[arg(long, default_value = "codex")]
        host: String,
    },
    /// Recommend a model for a ticket or a plan ticket
    Recommend {
        /// JSON file containing either a single-ticket plan or a plan
        #[arg(long)]
        ticket: String,
        /// Host name (codex, claude-code, opencode, antigravity)
        #[arg(long)]
        host: Option<String>,
        /// Ticket ID when the file contains multiple tickets
        #[arg(long = "ticket-id")]
        ticket_id: Option<String>,
    },
    /// Explain the persisted model recommendation for a session ticket
    Explain {
        /// Session ID
        #[arg(long)]
        session: String,
        /// Ticket ID
        #[arg(long = "ticket-id")]
        ticket_id: String,
    },
}

#[derive(Debug, Serialize)]
struct ModelCatalogOutput {
    #[serde(flatten)]
    catalog: ModelCatalog,
}

impl OutputData for ModelCatalogOutput {
    fn render_human(&self) -> String {
        let mut out = format!("Model catalog for {}\n\n", self.catalog.host);
        for model in &self.catalog.models {
            out.push_str(&format!(
                "  {}  {:?}  web={}  efforts={:?}\n",
                model.model, model.tier, model.web_capable, model.reasoning_efforts
            ));
            out.push_str(&format!("      {}\n", model.notes));
        }
        if !self.catalog.source_urls.is_empty() {
            out.push_str("\nSources:\n");
            for source_url in &self.catalog.source_urls {
                out.push_str(&format!("  {source_url}\n"));
            }
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct ModelRecommendationOutput {
    recommendation: ModelRecommendation,
}

impl OutputData for ModelRecommendationOutput {
    fn render_human(&self) -> String {
        let rec = &self.recommendation;
        format!(
            "Ticket: {}\nHost: {}\nModel: {}\nTier: {:?}\nReasoning: {:?}\nWeb required: {}\nRisk: {}\nSource: {}\nValid until: {}\nReason: {}\nResolved: {}\n",
            rec.ticket_id,
            rec.host,
            rec.model,
            rec.tier,
            rec.reasoning_effort,
            rec.web_required,
            rec.quality_risk,
            rec.source,
            rec.valid_until,
            rec.reason,
            rec.resolved_at
        )
    }
}

pub fn run(action: &ModelAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        ModelAction::Catalog { host } => run_catalog(host, output),
        ModelAction::Recommend {
            ticket,
            host,
            ticket_id,
        } => run_recommend(ticket, host.as_deref(), ticket_id.as_deref(), output),
        ModelAction::Explain { session, ticket_id } => run_explain(session, ticket_id, output),
    }
}

fn run_catalog(host: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let catalog = catalog_for_host(host)
        .ok_or_else(|| OrquestraError::from(format!("Unknown model host: {host}")))?;
    print_output(&ModelCatalogOutput { catalog }, output);
    Ok(())
}

fn run_recommend(
    ticket_file: &str,
    host: Option<&str>,
    ticket_id: Option<&str>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let plan = load_plan(ticket_file)
        .map_err(|error| OrquestraError::from(format!("Cannot load plan: {error}")))?;
    let recommendation = recommend_for_plan(&plan, ticket_id, host)
        .map_err(|error| OrquestraError::from(format!("Cannot recommend model: {error}")))?;
    print_output(&ModelRecommendationOutput { recommendation }, output);
    Ok(())
}

fn run_explain(
    session_id: &str,
    ticket_id: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    crate::run::validate_session_id(session_id)?;
    storage::validate_ticket_id(ticket_id)
        .map_err(|_| OrquestraError::from("Ticket ID must be a safe filename"))?;
    let project_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let session = load_session(&project_dir, session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot load session: {error}")))?;
    let recommendation = session
        .ticket_states
        .get(ticket_id)
        .and_then(|state| state.model_recommendation.clone())
        .ok_or_else(|| {
            OrquestraError::from(format!("No model recommendation for ticket {ticket_id}"))
        })?;
    print_output(&ModelRecommendationOutput { recommendation }, output);
    Ok(())
}
