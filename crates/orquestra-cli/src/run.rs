use crate::cli::RunAction;
use crate::output::{OutputData, print_output};
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::load_plan;
use orquestra_runtime::{
    Checkpoint, DispatchMode, Session, WaveDispatch, approve_wave, cancel_session,
    checkpoint_session, create_session, dispatch_wave, events::read_events, list_research_reports,
    list_verification_reports, load_session, record_ticket_result, reroute_ticket_to_skill,
    start_session, storage,
};
use orquestra_skills::{SkillStatus, inventory};
use serde::Serialize;

fn project_dir() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

#[derive(Debug, Serialize)]
pub(super) struct SessionOutput {
    pub(super) session: Session,
}

impl OutputData for SessionOutput {
    fn render_human(&self) -> String {
        let s = &self.session;
        format!(
            "Session: {}\nPlan: {}\nStatus: {:?}\nWave: {}/{}\nCreated: {}\nUpdated: {}\nTickets: {}",
            s.id,
            s.plan_title,
            s.status,
            s.current_wave,
            s.total_waves,
            s.created_at,
            s.updated_at,
            s.ticket_states.len()
        )
    }
}

#[derive(Debug, Serialize)]
struct SessionIdOutput {
    id: String,
}

impl OutputData for SessionIdOutput {
    fn render_human(&self) -> String {
        format!("Session created: {}", self.id)
    }
}

pub fn run(action: &RunAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        RunAction::Create { plan_file } => run_create(plan_file, output),
        RunAction::Start { session_id } => {
            validate_session_id(session_id)?;
            run_start(session_id, output)
        }
        RunAction::Dispatch {
            session_id,
            wave,
            host,
        } => {
            validate_session_id(session_id)?;
            run_dispatch(session_id, *wave, host, output)
        }
        RunAction::CompleteTicket {
            session_id,
            ticket_id,
            output: ticket_output,
            evidence,
        } => {
            validate_session_id(session_id)?;
            run_ticket_result(
                session_id,
                ticket_id,
                true,
                ticket_output.clone(),
                evidence.clone(),
                output,
            )
        }
        RunAction::FailTicket {
            session_id,
            ticket_id,
            output: ticket_output,
            evidence,
        } => {
            validate_session_id(session_id)?;
            run_ticket_result(
                session_id,
                ticket_id,
                false,
                ticket_output.clone(),
                evidence.clone(),
                output,
            )
        }
        RunAction::RerouteTicket {
            session_id,
            ticket_id,
            reason,
            skill,
        } => {
            validate_session_id(session_id)?;
            run_reroute_ticket(session_id, ticket_id, reason, skill.as_deref(), output)
        }
        RunAction::ApproveWave {
            session_id,
            wave,
            notes,
        } => {
            validate_session_id(session_id)?;
            run_approve_wave(session_id, *wave, notes.clone(), output)
        }
        RunAction::Status { session_id } => {
            validate_session_id(session_id)?;
            run_status(session_id, output)
        }
        RunAction::Checkpoint { session_id, wave } => {
            validate_session_id(session_id)?;
            run_checkpoint(session_id, *wave, output)
        }
        RunAction::Cancel { session_id } => {
            validate_session_id(session_id)?;
            run_cancel(session_id, output)
        }
        RunAction::Export {
            session_id,
            format,
            output_file,
        } => {
            validate_session_id(session_id)?;
            run_export(session_id, format, output_file.as_deref(), output)
        }
    }
}

pub(super) fn validate_session_id(session_id: &str) -> Result<(), OrquestraError> {
    orquestra_runtime::storage::validate_session_id(session_id)
        .map_err(|_| OrquestraError::from("Session ID must be a UUID v4"))
}

fn run_create(plan_file: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let plan =
        load_plan(plan_file).map_err(|e| OrquestraError::from(format!("Cannot load plan: {e}")))?;
    let validation = orquestra_plan::validate_plan(&plan);
    let mut validation = validation;
    crate::plan::validate_inventory_skills(&plan, &mut validation)?;
    if !validation.valid {
        return Err(OrquestraError::from(format!(
            "Plan validation failed: {} errors",
            validation.errors.len()
        )));
    }
    let waves = orquestra_plan::derive_waves(&plan)
        .map_err(|e| OrquestraError::from(format!("Cannot derive waves: {e}")))?;
    let session = create_session(&project_dir(), &plan, &waves)
        .map_err(|e| OrquestraError::from(format!("Cannot create session: {e}")))?;
    print_output(&SessionIdOutput { id: session.id }, output);
    Ok(())
}

fn run_start(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let session = start_session(&project_dir(), session_id)
        .map_err(|e| OrquestraError::from(format!("Cannot start session: {e}")))?;
    print_output(&SessionOutput { session }, output);
    Ok(())
}

fn run_dispatch(
    session_id: &str,
    wave: Option<u32>,
    host: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let dispatch_mode = if host == "manual" {
        DispatchMode::Manual
    } else {
        DispatchMode::Host {
            name: host.to_string(),
        }
    };
    let dispatch = dispatch_wave(&project_dir(), session_id, wave, dispatch_mode)
        .map_err(|e| OrquestraError::from(format!("Cannot dispatch wave: {e}")))?;
    print_output(&WaveDispatchOutput { dispatch }, output);
    Ok(())
}

fn run_ticket_result(
    session_id: &str,
    ticket_id: &str,
    completed: bool,
    ticket_output: Option<String>,
    evidence: Vec<String>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    orquestra_runtime::storage::validate_ticket_id(ticket_id)
        .map_err(|_| OrquestraError::from("Ticket ID must be a safe filename"))?;
    let session = record_ticket_result(
        &project_dir(),
        session_id,
        ticket_id,
        completed,
        ticket_output,
        evidence,
    )
    .map_err(|e| OrquestraError::from(format!("Cannot record ticket result: {e}")))?;
    print_output(&SessionOutput { session }, output);
    Ok(())
}

fn run_reroute_ticket(
    session_id: &str,
    ticket_id: &str,
    reason: &str,
    skill: Option<&str>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    orquestra_runtime::storage::validate_ticket_id(ticket_id)
        .map_err(|_| OrquestraError::from("Ticket ID must be a safe filename"))?;
    if let Some(requested_skill) = skill {
        let current_inventory = inventory::read_inventory()
            .map_err(|error| {
                OrquestraError::from(format!("Cannot read skills inventory: {error}"))
            })?
            .ok_or_else(|| {
                OrquestraError::from(
                    "No skills inventory found. Run 'orquestra skill scan' before rerouting.",
                )
            })?;
        let active = current_inventory.skills.iter().any(|candidate| {
            candidate.status == SkillStatus::Active
                && (candidate.name.eq_ignore_ascii_case(requested_skill)
                    || candidate.id.eq_ignore_ascii_case(requested_skill))
        });
        if !active {
            return Err(OrquestraError::from(format!(
                "Cannot reroute ticket: skill '{requested_skill}' is not active in the current inventory"
            )));
        }
    }
    let session = reroute_ticket_to_skill(&project_dir(), session_id, ticket_id, reason, skill)
        .map_err(|error| OrquestraError::from(format!("Cannot reroute ticket: {error}")))?;
    print_output(&SessionOutput { session }, output);
    Ok(())
}

fn run_approve_wave(
    session_id: &str,
    wave: u32,
    notes: Option<String>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let session = approve_wave(&project_dir(), session_id, wave, notes)
        .map_err(|e| OrquestraError::from(format!("Cannot approve wave: {e}")))?;
    print_output(&SessionOutput { session }, output);
    Ok(())
}

fn run_status(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let session = load_session(&project_dir(), session_id)
        .map_err(|e| OrquestraError::from(format!("Cannot load session: {e}")))?;
    print_output(&SessionOutput { session }, output);
    Ok(())
}

fn run_checkpoint(
    session_id: &str,
    wave: u32,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let cp = checkpoint_session(&project_dir(), session_id, wave)
        .map_err(|e| OrquestraError::from(format!("Cannot checkpoint: {e}")))?;
    print_output(&CheckpointOutput { checkpoint: cp }, output);
    Ok(())
}

fn run_cancel(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let session = cancel_session(&project_dir(), session_id)
        .map_err(|e| OrquestraError::from(format!("Cannot cancel session: {e}")))?;
    print_output(&SessionOutput { session }, output);
    Ok(())
}

fn run_export(
    session_id: &str,
    format: &str,
    output_file: Option<&str>,
    _output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let content = match format {
        "json" => {
            let session = load_session(&project_dir(), session_id)
                .map_err(|e| OrquestraError::from(format!("Cannot load session: {e}")))?;
            serde_json::to_string_pretty(&session)
                .map_err(|e| OrquestraError::from(format!("Cannot serialize: {e}")))?
        }
        "md" => export_markdown(session_id)?,
        other => {
            return Err(OrquestraError::from(format!(
                "Unknown format: {other}. Use json or md"
            )));
        }
    };
    if let Some(path) = output_file {
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, &content)
            .map_err(|e| OrquestraError::from(format!("Cannot write to {path}: {e}")))?;
    } else {
        println!("{content}");
    }
    Ok(())
}

fn export_markdown(session_id: &str) -> Result<String, OrquestraError> {
    let project_dir = project_dir();
    let session = load_session(&project_dir, session_id)
        .map_err(|e| OrquestraError::from(format!("Cannot load session: {e}")))?;

    let mut out = String::new();

    out.push_str(&format!("# Session: {}\n\n", session.id));
    out.push_str(&format!("**Plan:** {}\n", session.plan_title));
    out.push_str(&format!("**Status:** {:?}\n", session.status));
    out.push_str(&format!(
        "**Wave:** {}/{}\n",
        session.current_wave, session.total_waves
    ));
    out.push_str(&format!("**Created:** {}\n", session.created_at));
    out.push_str(&format!("**Updated:** {}\n", session.updated_at));
    if let Some(ref hash) = session.inventory_hash {
        out.push_str(&format!("**Inventory Hash:** `{}`\n", hash));
    }
    out.push('\n');

    let mut ticket_states = session.ticket_states.values().collect::<Vec<_>>();
    ticket_states.sort_by(|left, right| left.id.cmp(&right.id));
    out.push_str("## Tickets\n\n");
    out.push_str("| ID | Status | Wave | Skill | Retries |\n");
    out.push_str("|----|--------|------|-------|---------|\n");
    for ts in &ticket_states {
        let skill = ts.assigned_skill.as_deref().unwrap_or("-");
        out.push_str(&format!(
            "| {} | {:?} | {} | {} | {} |\n",
            ts.id, ts.status, ts.wave, skill, ts.retries
        ));
    }
    out.push('\n');

    let events_path = storage::event_log_file(&project_dir, session_id)
        .map_err(|e| OrquestraError::from(format!("Cannot locate events: {e}")))?;
    let events = match read_events(&events_path) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("cannot read session events for export: {e}");
            vec![]
        }
    };
    if !events.is_empty() {
        out.push_str("## Events\n\n");
        out.push_str("| Timestamp | Event |\n");
        out.push_str("|-----------|-------|\n");
        for event in events.iter().rev().take(50).rev() {
            let ts: String = event.ts.chars().take(19).collect();
            out.push_str(&format!("| {} | {} |\n", ts, event.event));
        }
        out.push('\n');
    }

    let verifications = match list_verification_reports(&project_dir, session_id) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("cannot read verification reports for export: {e}");
            vec![]
        }
    };
    if !verifications.is_empty() {
        out.push_str("## Verification Reports\n\n");
        for report in &verifications {
            let icon = if report.score >= 0.7 { "✅" } else { "❌" };
            out.push_str(&format!(
                "### {} — score: {:.2} {}\n\n",
                report.ticket_id, report.score, icon
            ));
            out.push_str(&format!("**Skill:** {}\n", report.skill_name));
            out.push_str(&format!("**Summary:** {}\n", report.summary));
            if !report.evidence.is_empty() {
                out.push_str("**Evidence:** ");
                let kinds: Vec<&str> = report.evidence.iter().map(|e| e.kind.as_str()).collect();
                out.push_str(&kinds.join(", "));
                out.push('\n');
            }
            out.push('\n');
        }
    }

    let research = match list_research_reports(&project_dir, session_id) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("cannot read research briefs for export: {e}");
            vec![]
        }
    };
    if !research.is_empty() {
        out.push_str("## Research Briefs\n\n");
        for report in &research {
            out.push_str(&format!(
                "### {} — {} claims\n\n",
                report.ticket_id,
                report.claims.len()
            ));
            for claim in &report.claims {
                out.push_str(&format!(
                    "- **{}:** {} (confidence: {:.2})\n",
                    claim.id, claim.statement, claim.confidence
                ));
                for source in &claim.sources {
                    let st = if source.source_type.eq_ignore_ascii_case("primary") {
                        "primary"
                    } else {
                        "secondary"
                    };
                    out.push_str(&format!(
                        "  - [{}]({}) ({}, {})\n",
                        source.title, source.url, st, source.trust_level
                    ));
                }
            }
            out.push('\n');
        }
    }

    Ok(out)
}

#[derive(Debug, Serialize)]
struct CheckpointOutput {
    checkpoint: Checkpoint,
}

impl OutputData for CheckpointOutput {
    fn render_human(&self) -> String {
        format!(
            "Checkpoint created for wave {}",
            self.checkpoint.wave_number
        )
    }
}

#[derive(Debug, Serialize)]
struct WaveDispatchOutput {
    dispatch: WaveDispatch,
}

impl OutputData for WaveDispatchOutput {
    fn render_human(&self) -> String {
        let dispatch = &self.dispatch;
        let mut out = format!(
            "Dispatched wave {} for session {}\n",
            dispatch.wave, dispatch.session_id
        );
        for ticket in &dispatch.tickets {
            out.push_str(&format!(
                "  {} -> {}\n",
                ticket.ticket_id,
                ticket.manifest_path.display()
            ));
        }
        out
    }
}
