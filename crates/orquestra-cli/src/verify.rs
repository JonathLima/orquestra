use crate::output::{OutputData, print_output};
use clap::Subcommand;
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_plan::load_plan;
use orquestra_runtime::{
    VerificationOutcome, VerificationReport, evaluate_report, load_session,
    load_verification_report, save_verification_report, storage, verify_with_profile,
};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum VerifyAction {
    /// Evaluate and persist a ticket verification report
    Ticket {
        /// JSON verification report file (mutually exclusive with --profile)
        #[arg(long, conflicts_with = "profile")]
        report: Option<String>,
        /// Optional plan file used to read the ticket policy
        #[arg(long)]
        plan: Option<String>,
        /// Verification profile name from config.toml (mutually exclusive with --report)
        #[arg(long, conflicts_with = "report")]
        profile: Option<String>,
        /// Ticket output directory (required with --profile)
        #[arg(long)]
        ticket_dir: Option<String>,
    },
    /// Evaluate a persisted report for a session ticket
    Run {
        session_id: String,
        ticket_id: String,
    },
    /// List persisted reports for a session
    Report { session_id: String },
}

#[derive(Debug, Serialize)]
struct VerifyOutcomeOutput {
    outcome: VerificationOutcome,
}

impl OutputData for VerifyOutcomeOutput {
    fn render_human(&self) -> String {
        let outcome = &self.outcome;
        let mut out = format!(
            "Ticket: {}\nPassed: {}\nScore: {} / {}\n",
            outcome.ticket_id, outcome.passed, outcome.score, outcome.minimum_score
        );
        if !outcome.missing_evidence.is_empty() {
            out.push_str(&format!(
                "Missing evidence: {}\n",
                outcome.missing_evidence.join(", ")
            ));
        }
        for reason in &outcome.reasons {
            out.push_str(&format!("Reason: {reason}\n"));
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct VerifyReportListOutput {
    session_id: String,
    reports: Vec<String>,
}

impl OutputData for VerifyReportListOutput {
    fn render_human(&self) -> String {
        if self.reports.is_empty() {
            return format!("No verification reports for session {}.", self.session_id);
        }
        format!(
            "Verification reports for session {}:\n  {}",
            self.session_id,
            self.reports.join("\n  ")
        )
    }
}

pub fn run(action: &VerifyAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        VerifyAction::Ticket {
            report,
            plan,
            profile,
            ticket_dir,
        } => {
            if let Some(profile_name) = profile {
                run_ticket_profile(ticket_dir.as_deref().unwrap_or("."), profile_name, output)
            } else {
                let report_path = report.as_deref().ok_or_else(|| {
                    OrquestraError::from("either --report or --profile is required")
                })?;
                run_ticket(report_path, plan.as_deref(), output)
            }
        }
        VerifyAction::Run {
            session_id,
            ticket_id,
        } => {
            crate::run::validate_session_id(session_id)?;
            storage::validate_ticket_id(ticket_id)
                .map_err(|_| OrquestraError::from("Ticket ID must be a safe filename"))?;
            run_session_ticket(session_id, ticket_id, output)
        }
        VerifyAction::Report { session_id } => {
            crate::run::validate_session_id(session_id)?;
            run_report(session_id, output)
        }
    }
}

fn run_ticket(
    report_file: &str,
    plan_file: Option<&str>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let report = load_report_file(report_file)?;
    let policy = match plan_file {
        Some(path) => policy_from_plan(path, &report.ticket_id)?,
        None => Default::default(),
    };
    let outcome = evaluate_report(&policy, &report)
        .map_err(|error| OrquestraError::from(format!("Cannot verify report: {error}")))?;
    save_verification_report(&project_dir(), &report)
        .map_err(|error| OrquestraError::from(format!("Cannot save report: {error}")))?;
    let passed = outcome.passed;
    print_output(&VerifyOutcomeOutput { outcome }, output);
    if !passed {
        return Err(OrquestraError::from("verification failed"));
    }
    Ok(())
}

fn run_ticket_profile(
    ticket_dir: &str,
    profile_name: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let project_dir = project_dir();
    let ticket_path = PathBuf::from(ticket_dir);
    let result = verify_with_profile(&project_dir, &ticket_path, profile_name)
        .map_err(|e| OrquestraError::from(format!("Profile verification failed: {e}")))?;
    if output == &OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap());
    } else {
        let status = if result.exit_code == 0 {
            "PASSED"
        } else {
            "FAILED"
        };
        println!(
            "[{status}] profile={}, exit_code={}, duration={}ms, artifacts={}",
            result.profile_name,
            result.exit_code,
            result.duration_ms,
            result.artifacts.len()
        );
    }
    Ok(())
}

fn run_session_ticket(
    session_id: &str,
    ticket_id: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let plan_path = storage::plan_file(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot locate session plan: {error}")))?;
    let plan = load_plan(&plan_path.display().to_string())
        .map_err(|error| OrquestraError::from(format!("Cannot load session plan: {error}")))?;
    let policy = plan
        .tickets
        .iter()
        .find(|ticket| ticket.id == ticket_id)
        .map(|ticket| ticket.verification.clone())
        .ok_or_else(|| OrquestraError::from(format!("Ticket '{ticket_id}' not found in plan")))?;
    let report =
        load_verification_report(&project_dir(), session_id, ticket_id).map_err(|error| {
            OrquestraError::from(format!("Cannot load verification report: {error}"))
        })?;
    let outcome = evaluate_report(&policy, &report)
        .map_err(|error| OrquestraError::from(format!("Cannot verify report: {error}")))?;
    let passed = outcome.passed;
    print_output(&VerifyOutcomeOutput { outcome }, output);
    if !passed {
        return Err(OrquestraError::from("verification failed"));
    }
    Ok(())
}

fn run_report(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let _ = load_session(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot load session: {error}")))?;
    let dir = project_dir()
        .join(".orquestra")
        .join("verification")
        .join(session_id);
    let mut reports = Vec::new();
    if dir.exists() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
                && let Some(name) = entry.file_name().to_str()
            {
                reports.push(name.to_string());
            }
        }
    }
    reports.sort();
    print_output(
        &VerifyReportListOutput {
            session_id: session_id.to_string(),
            reports,
        },
        output,
    );
    Ok(())
}

fn policy_from_plan(
    plan_file: &str,
    ticket_id: &str,
) -> Result<orquestra_plan::VerificationPolicy, OrquestraError> {
    let plan = load_plan(plan_file)
        .map_err(|error| OrquestraError::from(format!("Cannot load plan: {error}")))?;
    plan.tickets
        .iter()
        .find(|ticket| ticket.id == ticket_id)
        .map(|ticket| ticket.verification.clone())
        .ok_or_else(|| OrquestraError::from(format!("Ticket '{ticket_id}' not found in plan")))
}

fn load_report_file(path: &str) -> Result<VerificationReport, OrquestraError> {
    let content = std::fs::read_to_string(path)
        .map_err(|error| OrquestraError::from(format!("Cannot read report file: {error}")))?;
    serde_json::from_str(&content)
        .map_err(|error| OrquestraError::from(format!("Cannot parse report: {error}")))
}

fn project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
