use crate::{brain, init, model, research, skills, verify};
use clap::{Parser, Subcommand};
use orquestra_core::config::OutputFormat;
use serde::Serialize;

#[derive(Parser)]
#[command(
    name = "orquestra",
    version,
    about = "Orquestra Universal Plugin — install orchestration skills into any AI CLI"
)]
pub struct Cli {
    #[arg(long, default_value = "human", value_parser = parse_output_format)]
    pub output: OutputFormat,

    #[arg(long, default_value = "warn")]
    pub log_level: String,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Show system diagnostics
    Doctor {
        /// Include security and policy diagnostics
        #[arg(long)]
        security: bool,
    },

    /// List, detect, and inspect host adapters
    Adapter {
        #[command(subcommand)]
        action: AdapterAction,
    },

    /// Skill inventory management
    Skill {
        #[command(subcommand)]
        action: skills::SkillsAction,
    },

    /// BRAIN local skill adaptation workflow
    Brain {
        #[command(subcommand)]
        action: brain::BrainAction,
    },

    /// Wrap a host CLI process with Orquestra policy checks
    Proxy {
        /// Host name, or `doctor` for proxy diagnostics
        host: String,
        /// Arguments forwarded to the host after `--`
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Ticket verification reports and policy checks
    Verify {
        #[command(subcommand)]
        action: verify::VerifyAction,
    },

    /// Model catalog and per-ticket model recommendations
    Model {
        #[command(subcommand)]
        action: model::ModelAction,
    },

    /// Research briefs, cross-source validation, and project memory storage
    Research {
        #[command(subcommand)]
        action: research::ResearchAction,
    },

    /// Plan validation, wave derivation, and export
    Plan {
        #[command(subcommand)]
        action: PlanAction,
    },

    /// Create, manage, and export execution sessions
    Run {
        #[command(subcommand)]
        action: RunAction,
    },

    /// List, inspect, and resume execution sessions
    Session {
        #[command(subcommand)]
        action: SessionAction,
    },

    /// Automated discovery loop — questions, research, validation, and plan generation
    Init {
        #[command(subcommand)]
        action: init::InitAction,
    },

    /// Install Orquestra skills into a target host
    Setup {
        /// Target host (opencode, claude-code, codex, antigravity)
        #[arg(long)]
        host: String,

        /// Show install plan without writing files
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum AdapterAction {
    /// List all known adapters and their capabilities
    List,
    /// Detect installed CLIs on this system
    Detect,
    /// Show detailed adapter information
    Inspect {
        /// Adapter name to inspect
        host: String,
    },
}

#[derive(Subcommand)]
pub enum PlanAction {
    /// Validate a plan file
    Validate { plan_file: String },
    /// Derive execution waves from a plan
    Waves { plan_file: String },
    /// Show a human-readable plan explanation
    Explain { plan_file: String },
    /// Export a plan in a given format
    Export {
        plan_file: String,
        #[arg(long, default_value = "json")]
        format: String,
    },
}

#[derive(Debug, Serialize, Subcommand)]
pub enum RunAction {
    /// Create a new session from a plan file
    Create { plan_file: String },
    /// Start or resume a session
    Start { session_id: String },
    /// Dispatch a session wave into local ticket manifests
    Dispatch {
        session_id: String,
        #[arg(long)]
        wave: Option<u32>,
        #[arg(long, default_value = "manual")]
        host: String,
    },
    /// Mark a dispatched ticket completed
    CompleteTicket {
        session_id: String,
        ticket_id: String,
        #[arg(long)]
        output: Option<String>,
        #[arg(long = "evidence")]
        evidence: Vec<String>,
    },
    /// Mark a dispatched ticket failed
    FailTicket {
        session_id: String,
        ticket_id: String,
        #[arg(long)]
        output: Option<String>,
        #[arg(long = "evidence")]
        evidence: Vec<String>,
    },
    /// Return a failed ticket to the current wave for another bounded attempt
    RerouteTicket {
        session_id: String,
        ticket_id: String,
        #[arg(long)]
        reason: String,
        /// Active inventory skill to use for the next attempt
        #[arg(long)]
        skill: Option<String>,
    },
    /// Approve a completed wave checkpoint and advance execution
    ApproveWave {
        session_id: String,
        #[arg(long)]
        wave: u32,
        #[arg(long)]
        notes: Option<String>,
    },
    /// Show session status
    Status { session_id: String },
    /// Pause at the next wave boundary (checkpoint)
    Checkpoint {
        session_id: String,
        #[arg(long)]
        wave: u32,
    },
    /// Cancel a running session
    Cancel { session_id: String },
    /// Export session as JSON or markdown
    Export {
        session_id: String,
        #[arg(long, default_value = "json")]
        format: String,
        #[arg(long)]
        output_file: Option<String>,
    },
}

#[derive(Debug, Serialize, Subcommand)]
pub enum SessionAction {
    /// List all sessions
    List,
    /// Show session details
    Show { session_id: String },
    /// Resume session from checkpoint
    Resume { session_id: String },
    /// Show session events
    Events {
        session_id: String,
        #[arg(long)]
        tail: Option<usize>,
    },
    /// Export session
    Export {
        session_id: String,
        #[arg(long, default_value = "json")]
        format: String,
        #[arg(long)]
        output_file: Option<String>,
    },
}

fn parse_output_format(s: &str) -> Result<OutputFormat, String> {
    s.parse()
}
