use crate::output::{OutputData, print_output};
use clap::Subcommand;
use orquestra_adapters::get_adapter;
use orquestra_core::config::{Config, OutputFormat};
use orquestra_core::error::OrquestraError;
use orquestra_init::state::{InitPhase, PlanDraft, RankedSource};
use orquestra_init::{
    RefinementRequest, add_note, add_requirement, answer_question, assess_convergence,
    build_artifacts, cancel_init, classify_init, create_init_session, generate_brief,
    generate_plan_draft, list_sessions, list_topics, load_research_topic, load_state,
    record_tokens, save_state, store_results_with_config,
};
use orquestra_plan::{Plan, Ticket, VerificationPolicy, derive_waves};
use orquestra_runtime::create_session;
use orquestra_skills::{SkillInventory, SkillStatus, TrustLevel, brain, inventory, matching};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum InitAction {
    /// Start a new init discovery session
    Start {
        /// Host name (opencode, codex, claude-code, antigravity)
        #[arg(long)]
        host: String,
        /// Initial idea description
        #[arg(long)]
        idea: Option<String>,
    },
    /// Show init session status
    Status {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
    },
    /// Answer the current question in an init session
    Answer {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Question ID
        #[arg(long = "q")]
        question_id: String,
        /// Answer text
        #[arg(long)]
        answer: String,
    },
    /// Add a note to the current init round
    Note {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Note text
        #[arg(long)]
        text: String,
    },
    /// Cancel an init session
    Cancel {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
    },
    /// Initiate research for a technical topic in the init session
    Research {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Topic to research
        #[arg(long)]
        topic: String,
    },
    /// Record token usage for the current round
    RecordTokens {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Tokens in
        #[arg(long = "in")]
        tokens_in: u32,
        /// Tokens out
        #[arg(long = "out")]
        tokens_out: u32,
    },
    /// List all init sessions
    List,
    /// Generate a plan draft from the current init session
    Plan {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Host-generated adaptive plan draft JSON
        #[arg(long = "draft-file")]
        draft_file: Option<PathBuf>,
        /// Max tickets in the plan (0 = use config.toml `[init] max_tickets`)
        #[arg(long, default_value = "0")]
        max_tickets: usize,
    },
    /// Apply the plan draft: create plan.json, session, and BRAIN adaptations
    Apply {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
    },
    /// Run heuristic classifier on the init session
    Classify {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Optional JSON refinement response from an LLM (inline)
        #[arg(long, conflicts_with = "refinement_response_file")]
        refinement_response: Option<String>,
        /// Optional JSON refinement response from a file (avoids shell escaping issues)
        #[arg(
            long = "refinement-response-file",
            conflicts_with = "refinement_response"
        )]
        refinement_response_file: Option<PathBuf>,
    },
    /// Add a requirement extracted from answers or research
    AddRequirement {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Requirement text
        #[arg(long)]
        text: String,
        /// Source: user, research, or inferred (default: user)
        #[arg(long, default_value = "user")]
        source: String,
    },
    /// Evaluate research topics and advance the init session phase
    Evaluate {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
    },
    /// Store research results for a topic (accepts JSON array or WIE-MCP markdown via --sources-file)
    StoreResearch {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Research topic ID
        #[arg(long = "topic-id")]
        topic_id: String,
        /// JSON array of ranked sources (inline)
        #[arg(long = "sources-json", conflicts_with_all = ["sources_json_file", "markdown_file"])]
        sources_json: Option<String>,
        /// JSON array of ranked sources from a file (avoids shell escaping issues)
        #[arg(long = "sources-json-file", conflicts_with_all = ["sources_json", "markdown_file"])]
        sources_json_file: Option<PathBuf>,
        /// WIE-MCP markdown response from a file (parsed into RankedSource[])
        #[arg(long = "markdown-file", conflicts_with_all = ["sources_json", "sources_json_file"])]
        markdown_file: Option<PathBuf>,
    },
    /// Emit a delegation envelope requesting the host CLI's MCP client to perform web research.
    /// The envelope (JSON on stdout) contains the resolved webSearch tool name from the active
    /// host's tool_map plus a callback command the LLM host invokes after dispatching the MCP call.
    RequestResearch {
        /// Init session ID
        #[arg(long = "session-id")]
        session_id: String,
        /// Research topic ID
        #[arg(long = "topic-id")]
        topic_id: String,
        /// Host name whose tool_map resolves webSearch (opencode, codex, claude-code, antigravity)
        #[arg(long, default_value = "opencode")]
        host: String,
        /// Maximum number of sources to request
        #[arg(long)]
        max_sources: Option<usize>,
    },
}

#[derive(Debug, Serialize)]
struct InitResearchOutput {
    topic_id: String,
    query: String,
}

impl OutputData for InitResearchOutput {
    fn render_human(&self) -> String {
        format!(
            "Research started: {}\nQuery: {}\n",
            self.topic_id, self.query
        )
    }
}

#[derive(Debug, Serialize)]
struct InitStartOutput {
    session_id: String,
    phase: String,
    idea: String,
    current_date: String,
}

impl OutputData for InitStartOutput {
    fn render_human(&self) -> String {
        format!(
            "Init session started: {}\nPhase: {}\nIdea: {}\nDate: {}\n",
            self.session_id, self.phase, self.idea, self.current_date
        )
    }
}

#[derive(Debug, Serialize)]
struct InitStatusOutput {
    session_id: String,
    phase: String,
    round: u32,
    answered_ratio: f32,
    contradictions: u32,
    tokens_in: u32,
    tokens_out: u32,
    idea: String,
    confidence: f32,
    blockers: Vec<String>,
}

impl OutputData for InitStatusOutput {
    fn render_human(&self) -> String {
        format!(
            "Init session: {}\nPhase: {}\nRound: {}\nConfidence: {:.0}%\nAnswered: {:.0}%\nContradictions: {}\nTokens in/out: {}/{}\nIdea: {}\nBlockers: {}\n",
            self.session_id,
            self.phase,
            self.round,
            self.confidence * 100.0,
            self.answered_ratio * 100.0,
            self.contradictions,
            self.tokens_in,
            self.tokens_out,
            self.idea,
            if self.blockers.is_empty() {
                "none".to_string()
            } else {
                self.blockers.join("; ")
            },
        )
    }
}

#[derive(Debug, Serialize)]
struct InitOutput {
    session_id: String,
    phase: String,
    round: u32,
}

impl OutputData for InitOutput {
    fn render_human(&self) -> String {
        format!(
            "Init session {}\n  Phase: {}\n  Round: {}\n",
            self.session_id, self.phase, self.round
        )
    }
}

#[derive(Debug, Serialize)]
struct InitListOutput {
    sessions: Vec<InitSessionSummary>,
}

#[derive(Debug, Serialize)]
struct InitSessionSummary {
    id: String,
    idea: String,
    phase: String,
    round: u32,
}

impl OutputData for InitListOutput {
    fn render_human(&self) -> String {
        if self.sessions.is_empty() {
            return "No init sessions found.".to_string();
        }
        let rows: String = self
            .sessions
            .iter()
            .map(|s| {
                format!(
                    "  {}  {}  Round {}  {}\n",
                    s.id.chars().take(8).collect::<String>(),
                    s.phase,
                    s.round,
                    s.idea
                )
            })
            .collect();
        format!("Init sessions ({}):\n\n{}", self.sessions.len(), rows)
    }
}

fn project_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn load_json_arg(inline: Option<&str>, file: Option<&PathBuf>) -> Result<Option<String>, String> {
    if let Some(path) = file {
        std::fs::read_to_string(path)
            .map(Some)
            .map_err(|e| format!("Cannot read JSON file {}: {e}", path.display()))
    } else {
        Ok(inline.map(str::to_string))
    }
}

pub fn run(action: &InitAction, config: &Config) -> Result<(), OrquestraError> {
    let output = &config.output;
    match action {
        InitAction::Start { host, idea } => run_start(host, idea.as_deref(), output),
        InitAction::Status { session_id } => run_status(session_id, config, output),
        InitAction::Answer {
            session_id,
            question_id,
            answer,
        } => run_answer(session_id, question_id, answer, output),
        InitAction::Note { session_id, text } => run_note(session_id, text, output),
        InitAction::Cancel { session_id } => run_cancel(session_id, output),
        InitAction::Research { session_id, topic } => run_research(session_id, topic, output),
        InitAction::RecordTokens {
            session_id,
            tokens_in,
            tokens_out,
        } => run_record_tokens(session_id, *tokens_in, *tokens_out, output),
        InitAction::Plan {
            session_id,
            draft_file,
            max_tickets,
        } => {
            let effective = if *max_tickets == 0 {
                config.init.max_tickets
            } else {
                (*max_tickets).min(config.init.max_tickets_hard_limit)
            };
            run_plan(
                session_id,
                draft_file.as_ref(),
                effective,
                &config.init,
                output,
            )
        }
        InitAction::Apply { session_id } => run_apply(session_id, &config.init, output),
        InitAction::Classify {
            session_id,
            refinement_response,
            refinement_response_file,
        } => {
            let json = load_json_arg(
                refinement_response.as_deref(),
                refinement_response_file.as_ref(),
            )
            .map_err(OrquestraError::from)?;
            run_classify(
                session_id,
                json.as_deref(),
                config.init.min_confidence,
                output,
            )
        }
        InitAction::AddRequirement {
            session_id,
            text,
            source,
        } => run_add_requirement(session_id, text, source, output),
        InitAction::Evaluate { session_id } => run_evaluate(session_id, config, output),
        InitAction::StoreResearch {
            session_id,
            topic_id,
            sources_json,
            sources_json_file,
            markdown_file,
        } => run_store_research(
            session_id,
            topic_id,
            sources_json.as_deref(),
            sources_json_file.as_ref(),
            markdown_file.as_ref(),
            config,
            output,
        ),
        InitAction::RequestResearch {
            session_id,
            topic_id,
            host,
            max_sources,
        } => run_request_research(session_id, topic_id, host, *max_sources, config, output),
        InitAction::List => run_list(output),
    }
}

fn run_start(host: &str, idea: Option<&str>, output: &OutputFormat) -> Result<(), OrquestraError> {
    let state = create_init_session(&project_dir(), host, idea)
        .map_err(|error| OrquestraError::from(format!("Cannot start init: {error}")))?;
    print_output(
        &InitStartOutput {
            session_id: state.id,
            phase: format!("{:?}", state.phase),
            idea: state.idea,
            current_date: orquestra_init::today_date(),
        },
        output,
    );
    Ok(())
}

fn run_status(
    session_id: &str,
    config: &Config,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = load_state(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot load init session: {error}")))?;
    let phase = format!("{:?}", state.phase);
    let contradictions = state.contradictions_open_count();
    let tokens_in = state.metrics.total_tokens_in;
    let tokens_out = state.metrics.total_tokens_out;
    let topics = list_topics(&project_dir(), session_id).unwrap_or_default();
    let assessment = assess_convergence(&state, &topics, &config.init);
    let answered_ratio = assessment.requirements_confidence;
    print_output(
        &InitStatusOutput {
            session_id: state.id,
            phase,
            round: state.round,
            answered_ratio,
            contradictions,
            tokens_in,
            tokens_out,
            idea: state.idea,
            confidence: assessment.confidence,
            blockers: assessment.blockers,
        },
        output,
    );
    Ok(())
}

fn run_answer(
    session_id: &str,
    question_id: &str,
    answer_text: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = answer_question(&project_dir(), session_id, question_id, answer_text)
        .map_err(|error| OrquestraError::from(format!("Cannot answer: {error}")))?;
    print_output(
        &InitOutput {
            session_id: state.id,
            phase: format!("{:?}", state.phase),
            round: state.round,
        },
        output,
    );
    Ok(())
}

fn run_note(
    session_id: &str,
    note_text: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = add_note(&project_dir(), session_id, note_text)
        .map_err(|error| OrquestraError::from(format!("Cannot add note: {error}")))?;
    print_output(
        &InitOutput {
            session_id: state.id,
            phase: format!("{:?}", state.phase),
            round: state.round,
        },
        output,
    );
    Ok(())
}

fn run_cancel(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = cancel_init(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot cancel init: {error}")))?;
    print_output(
        &InitOutput {
            session_id: state.id,
            phase: format!("{:?}", state.phase),
            round: state.round,
        },
        output,
    );
    Ok(())
}

fn run_research(
    session_id: &str,
    topic: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let brief = generate_brief(&project_dir(), session_id, topic)
        .map_err(|error| OrquestraError::from(format!("Cannot start research: {error}")))?;
    let out = InitResearchOutput {
        topic_id: brief.id,
        query: brief.query,
    };
    print_output(&out, output);
    Ok(())
}

fn run_record_tokens(
    session_id: &str,
    tokens_in: u32,
    tokens_out: u32,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = record_tokens(&project_dir(), session_id, tokens_in, tokens_out)
        .map_err(|error| OrquestraError::from(format!("Cannot record tokens: {error}")))?;
    print_output(
        &InitOutput {
            session_id: state.id,
            phase: format!("{:?}", state.phase),
            round: state.round,
        },
        output,
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct InitPlanOutput {
    plan_id: String,
    title: String,
    ticket_count: usize,
    wave_count: usize,
    skills: Vec<String>,
    research_topics: Vec<String>,
    contradictions: u32,
    brain_candidates: Vec<String>,
}

impl OutputData for InitPlanOutput {
    fn render_human(&self) -> String {
        let skills = self.skills.join(", ");
        let research = self.research_topics.join(", ");
        format!(
            "Plan: {}\nTitle: {}\nTickets: {} in {} waves\nSkills: {}\nResearch: {}\nContradictions: {}\nAdapted skills pending approval: {}\n",
            self.plan_id,
            self.title,
            self.ticket_count,
            self.wave_count,
            skills,
            research,
            self.contradictions,
            if self.brain_candidates.is_empty() {
                "none".to_string()
            } else {
                self.brain_candidates.join(", ")
            },
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostPlanInput {
    title: String,
    tickets: Vec<HostTicketInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HostTicketInput {
    id: String,
    title: String,
    objective: String,
    acceptance_criteria: Vec<String>,
    #[serde(default)]
    blocked_by: Vec<String>,
    preferred_capabilities: Vec<String>,
}

fn run_plan(
    session_id: &str,
    draft_file: Option<&PathBuf>,
    max_tickets: usize,
    config: &orquestra_core::config::InitConfig,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let mut state = load_state(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot load init session: {error}")))?;
    let topics = list_topics(&project_dir(), session_id).unwrap_or_default();
    let assessment = assess_convergence(&state, &topics, config);
    if !matches!(state.phase, InitPhase::Converged { .. }) || !assessment.blockers.is_empty() {
        return Err(OrquestraError::from(format!(
            "Cannot generate plan before convergence. Confidence {:.2}; blockers: {}",
            assessment.confidence,
            assessment.blockers.join("; ")
        )));
    }
    let mut draft = if let Some(path) = draft_file {
        load_host_plan_draft(path, &state, &topics, max_tickets)?
    } else {
        generate_plan_draft(&state, &topics, max_tickets)
    };

    let inventory = inventory::read_inventory()
        .map_err(|error| OrquestraError::from(format!("Cannot read skills inventory: {error}")))?
        .ok_or_else(|| {
            OrquestraError::from(
                "No skills inventory found. Run 'orquestra skill scan' before planning.",
            )
        })?;
    let unresolved = route_draft_to_real_skills(&mut draft, &inventory);
    if !unresolved.is_empty() {
        state.plan_draft = Some(draft);
        save_state(&project_dir(), &state)
            .map_err(|error| OrquestraError::from(format!("Cannot save plan draft: {error}")))?;
        let has_find_skills = inventory.skills.iter().any(|skill| {
            skill.status == SkillStatus::Active && skill.name.eq_ignore_ascii_case("find-skills")
        });
        return Err(OrquestraError::from(format!(
            "SKILL_GAP: {}. find-skills available: {}. Discover or install only the missing relevant skills, rescan the inventory, then rerun init plan.",
            unresolved.join("; "),
            if has_find_skills { "yes" } else { "no" }
        )));
    }
    adapt_selected_skills(&mut draft, &inventory)?;
    refresh_draft_waves(&mut draft)?;

    state.plan_draft = Some(draft);
    save_state(&project_dir(), &state)
        .map_err(|error| OrquestraError::from(format!("Cannot save plan draft: {error}")))?;
    let saved = state.plan_draft.as_ref().expect("just saved");
    print_output(
        &InitPlanOutput {
            plan_id: saved.id.clone(),
            title: saved.title.clone(),
            ticket_count: saved.tickets.len(),
            wave_count: saved.waves.len(),
            skills: saved.skills_required.clone(),
            research_topics: saved.research_validated_topics.clone(),
            contradictions: saved.contradictions_open,
            brain_candidates: saved.skills_brain_required.clone(),
        },
        output,
    );
    Ok(())
}

fn load_host_plan_draft(
    path: &PathBuf,
    state: &orquestra_init::InitState,
    topics: &[orquestra_init::ResearchTopic],
    max_tickets: usize,
) -> Result<PlanDraft, OrquestraError> {
    let content = std::fs::read_to_string(path).map_err(|error| {
        OrquestraError::from(format!(
            "Cannot read host plan draft '{}': {error}",
            path.display()
        ))
    })?;
    let input: HostPlanInput = serde_json::from_str(&content)
        .map_err(|error| OrquestraError::from(format!("Invalid host plan draft JSON: {error}")))?;

    if input.title.trim().is_empty() {
        return Err(OrquestraError::from(
            "Invalid host plan draft: title cannot be empty.",
        ));
    }
    if input.tickets.is_empty() || input.tickets.len() > max_tickets {
        return Err(OrquestraError::from(format!(
            "Invalid host plan draft: expected 1..={max_tickets} tickets, got {}.",
            input.tickets.len()
        )));
    }

    let ids = input
        .tickets
        .iter()
        .map(|ticket| ticket.id.trim().to_string())
        .collect::<BTreeSet<_>>();
    if ids.len() != input.tickets.len() || ids.contains("") {
        return Err(OrquestraError::from(
            "Invalid host plan draft: ticket IDs must be non-empty and unique.",
        ));
    }

    let mut covered_requirements = BTreeSet::new();
    let mut tickets = Vec::with_capacity(input.tickets.len());
    for ticket in input.tickets {
        if ticket.title.trim().is_empty()
            || ticket.objective.trim().is_empty()
            || ticket.acceptance_criteria.is_empty()
            || ticket.preferred_capabilities.is_empty()
        {
            return Err(OrquestraError::from(format!(
                "Invalid host plan draft: ticket {} requires title, objective, acceptanceCriteria, and preferredCapabilities.",
                ticket.id
            )));
        }
        if ticket
            .blocked_by
            .iter()
            .any(|dependency| dependency == &ticket.id || !ids.contains(dependency))
        {
            return Err(OrquestraError::from(format!(
                "Invalid host plan draft: ticket {} has an unknown or self dependency.",
                ticket.id
            )));
        }
        for criterion in &ticket.acceptance_criteria {
            covered_requirements.insert(criterion.trim().to_ascii_lowercase());
        }
        tickets.push(orquestra_init::DraftTicket {
            id: ticket.id,
            title: ticket.title,
            objective: ticket.objective,
            acceptance_criteria: ticket.acceptance_criteria,
            blocked_by: ticket.blocked_by,
            preferred_capabilities: ticket.preferred_capabilities,
            assigned_skill: None,
            research_validated: true,
            wave: 0,
        });
    }

    let uncovered = state
        .requirements
        .items
        .iter()
        .filter(|requirement| {
            !covered_requirements.contains(&requirement.text.trim().to_ascii_lowercase())
        })
        .map(|requirement| requirement.id.clone())
        .collect::<Vec<_>>();
    if !uncovered.is_empty() {
        return Err(OrquestraError::from(format!(
            "Invalid host plan draft: every discovered requirement must appear verbatim in acceptanceCriteria. Missing: {}.",
            uncovered.join(", ")
        )));
    }

    let research_validated_topics = topics
        .iter()
        .filter(|topic| topic.status == orquestra_init::ResearchStatus::Validated)
        .map(|topic| topic.topic.clone())
        .collect::<Vec<_>>();

    Ok(PlanDraft {
        id: format!("plan-host-{}", chrono::Utc::now().timestamp_millis()),
        title: input.title,
        tickets,
        waves: Vec::new(),
        skills_required: Vec::new(),
        skills_brain_required: Vec::new(),
        research_validated_topics,
        contradictions_open: state.contradictions_open_count(),
        created_at: chrono::Utc::now(),
    })
}

fn refresh_draft_waves(draft: &mut PlanDraft) -> Result<(), OrquestraError> {
    let result = derive_waves(&convert_draft_to_plan(draft))
        .map_err(|error| OrquestraError::from(format!("Cannot derive draft waves: {error}")))?;
    draft.waves = result
        .waves
        .iter()
        .map(|wave| orquestra_init::DraftWave {
            wave_number: wave.wave_number,
            ticket_ids: wave.ticket_ids.clone(),
        })
        .collect();
    for ticket in &mut draft.tickets {
        ticket.wave = result
            .waves
            .iter()
            .find(|wave| wave.ticket_ids.contains(&ticket.id))
            .map(|wave| wave.wave_number)
            .unwrap_or(0);
    }
    Ok(())
}

fn route_draft_to_real_skills(
    draft: &mut PlanDraft,
    full_inventory: &SkillInventory,
) -> Vec<String> {
    let domain_inventory = SkillInventory {
        schema_version: full_inventory.schema_version,
        generated_at: full_inventory.generated_at,
        sources: full_inventory.sources.clone(),
        skills: full_inventory
            .skills
            .iter()
            .filter(|skill| {
                skill.status == SkillStatus::Active
                    && !skill.name.starts_with("orquestra-")
                    && !skill.name.eq_ignore_ascii_case("find-skills")
            })
            .cloned()
            .collect(),
    };
    let mut selected = Vec::new();
    let mut unresolved = Vec::new();
    for draft_ticket in &mut draft.tickets {
        let ticket = Ticket {
            id: draft_ticket.id.clone(),
            title: draft_ticket.title.clone(),
            objective: draft_ticket.objective.clone(),
            acceptance_criteria: draft_ticket.acceptance_criteria.clone(),
            blocked_by: draft_ticket.blocked_by.clone(),
            preferred_capabilities: draft_ticket.preferred_capabilities.clone(),
            assigned_skill: None,
            model_policy: None,
            model_recommendation: None,
            verification: VerificationPolicy::default(),
        };
        let approved_adaptation = domain_inventory
            .skills
            .iter()
            .find(|skill| approved_adaptation_matches_ticket(skill, draft_ticket))
            .map(|skill| skill.name.clone());
        let report = matching::match_ticket(&ticket, &domain_inventory);
        let skill = approved_adaptation.or_else(|| {
            report
                .matches
                .first()
                .filter(|candidate| candidate.score >= 0.2)
                .map(|candidate| candidate.skill_name.clone())
        });
        if let Some(skill) = skill {
            draft_ticket.assigned_skill = Some(skill.clone());
            selected.push(skill);
        } else {
            draft_ticket.assigned_skill = None;
            unresolved.push(format!(
                "{} requires [{}]",
                draft_ticket.id,
                draft_ticket.preferred_capabilities.join(", ")
            ));
        }
    }
    selected.sort();
    selected.dedup();
    draft.skills_required = selected;
    draft.skills_brain_required = unresolved.clone();
    unresolved
}

fn adapt_selected_skills(
    draft: &mut PlanDraft,
    inventory: &SkillInventory,
) -> Result<(), OrquestraError> {
    let mut candidates = Vec::new();
    let mut adapted_skills = Vec::new();
    for draft_ticket in &mut draft.tickets {
        let source_name = draft_ticket.assigned_skill.as_ref().ok_or_else(|| {
            OrquestraError::from(format!(
                "Cannot adapt ticket {} without an assigned source skill",
                draft_ticket.id
            ))
        })?;
        let source = inventory
            .skills
            .iter()
            .find(|skill| {
                skill.status == SkillStatus::Active
                    && (skill.name.eq_ignore_ascii_case(source_name)
                        || skill.id.eq_ignore_ascii_case(source_name))
            })
            .ok_or_else(|| {
                OrquestraError::from(format!(
                    "Assigned source skill '{}' for ticket {} disappeared from inventory",
                    source_name, draft_ticket.id
                ))
            })?;
        if approved_adaptation_matches_ticket(source, draft_ticket) {
            adapted_skills.push(source.name.clone());
            continue;
        }
        let ticket = Ticket {
            id: draft_ticket.id.clone(),
            title: draft_ticket.title.clone(),
            objective: draft_ticket.objective.clone(),
            acceptance_criteria: draft_ticket.acceptance_criteria.clone(),
            blocked_by: draft_ticket.blocked_by.clone(),
            preferred_capabilities: draft_ticket.preferred_capabilities.clone(),
            assigned_skill: Some(source.name.clone()),
            model_policy: None,
            model_recommendation: None,
            verification: VerificationPolicy {
                minimum_score: 0.95,
                required_evidence: vec!["artifact".to_string()],
            },
        };
        let candidate =
            brain::adapt_local_skill(&project_dir(), &ticket, source).map_err(|error| {
                OrquestraError::from(format!(
                    "Cannot create project adaptation for ticket {} from '{}': {error}",
                    draft_ticket.id, source.name
                ))
            })?;
        draft_ticket.assigned_skill = Some(candidate.skill_name.clone());
        candidates.push(candidate.id);
        adapted_skills.push(candidate.skill_name);
    }
    adapted_skills.sort();
    adapted_skills.dedup();
    draft.skills_required = adapted_skills;
    draft.skills_brain_required = candidates;
    Ok(())
}

fn approved_adaptation_matches_ticket(
    source: &orquestra_skills::SkillInfo,
    ticket: &orquestra_init::DraftTicket,
) -> bool {
    if source.trust != TrustLevel::BrainApproved || source.scope != "project" {
        return false;
    }
    let Ok(content) = std::fs::read_to_string(&source.source_path) else {
        return false;
    };
    let content = content.replace("\r\n", "\n");
    let objective = format!(
        "## Ticket Objective\n\n{}\n\n## Acceptance Criteria",
        ticket.objective
    );
    let acceptance_criteria = format!(
        "## Acceptance Criteria\n\n{}\n\n## Preferred Capabilities",
        ticket
            .acceptance_criteria
            .iter()
            .map(|criterion| format!("- {criterion}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let capabilities = format!(
        "## Preferred Capabilities\n\n{}\n\n## Inherited Skill",
        ticket
            .preferred_capabilities
            .iter()
            .map(|capability| format!("- {capability}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    content.contains(&objective)
        && content.contains(&acceptance_criteria)
        && content.contains(&capabilities)
}

#[derive(Debug, Serialize)]
struct InitApplyOutput {
    session_id: String,
    plan_id: String,
    plan_path: String,
    ticket_count: usize,
    wave_count: usize,
    brain_candidates: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    artifacts: Vec<String>,
}

impl OutputData for InitApplyOutput {
    fn render_human(&self) -> String {
        let mut out = format!(
            "Applied plan {} -> session {}\nPlan file: {}\nTickets: {} in {} waves\nBRAIN candidates: {}\n",
            self.plan_id,
            self.session_id,
            self.plan_path,
            self.ticket_count,
            self.wave_count,
            self.brain_candidates
        );
        if !self.artifacts.is_empty() {
            out.push_str(&format!("Artifacts: {}\n", self.artifacts.join(", ")));
        }
        out
    }
}

fn run_apply(
    session_id: &str,
    config: &orquestra_core::config::InitConfig,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let mut state = load_state(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot load init session: {error}")))?;
    let topics = list_topics(&project_dir(), session_id).unwrap_or_default();
    let assessment = assess_convergence(&state, &topics, config);
    if !matches!(state.phase, InitPhase::Converged { .. }) || !assessment.blockers.is_empty() {
        return Err(OrquestraError::from(format!(
            "Cannot apply plan before convergence. Confidence {:.2}; blockers: {}",
            assessment.confidence,
            assessment.blockers.join("; ")
        )));
    }
    let draft = state
        .plan_draft
        .as_ref()
        .ok_or_else(|| OrquestraError::from("No plan draft. Run 'init plan' first."))?
        .clone();

    let plan = convert_draft_to_plan(&draft);
    let current_inventory = inventory::read_inventory()
        .map_err(|error| OrquestraError::from(format!("Cannot read skills inventory: {error}")))?
        .ok_or_else(|| {
            OrquestraError::from(
                "No skills inventory found. Run 'orquestra skill scan' before applying a plan.",
            )
        })?;
    for ticket in &plan.tickets {
        let assigned = ticket.assigned_skill.as_ref().ok_or_else(|| {
            OrquestraError::from(format!(
                "Cannot apply plan: ticket {} has no assigned real skill",
                ticket.id
            ))
        })?;
        let active = current_inventory.skills.iter().any(|skill| {
            skill.status == SkillStatus::Active
                && (skill.name.eq_ignore_ascii_case(assigned)
                    || skill.id.eq_ignore_ascii_case(assigned))
        });
        if !active {
            return Err(OrquestraError::from(format!(
                "Cannot apply plan: assigned skill '{}' for ticket {} is not active in the current inventory. Rescan and replan.",
                assigned, ticket.id
            )));
        }
    }
    let brain_candidates = 0;

    let plan_dir = project_dir()
        .join(".orquestra")
        .join("init")
        .join(session_id);
    std::fs::create_dir_all(&plan_dir)
        .map_err(|e| OrquestraError::from(format!("Cannot create plan dir: {e}")))?;
    let plan_path = plan_dir.join("plan.json");
    std::fs::write(
        &plan_path,
        serde_json::to_string_pretty(&plan)
            .map_err(|e| OrquestraError::from(format!("Cannot serialize plan: {e}")))?,
    )
    .map_err(|e| OrquestraError::from(format!("Cannot write plan file: {e}")))?;

    let waves = derive_waves(&plan)
        .map_err(|e| OrquestraError::from(format!("Cannot derive waves: {e}")))?;

    let session = create_session(&project_dir(), &plan, &waves)
        .map_err(|e| OrquestraError::from(format!("Cannot create runtime session: {e}")))?;

    state.phase = InitPhase::Applied {
        session_id: session.id.clone(),
    };
    state.plan_draft = Some(draft);
    state.updated_at = chrono::Utc::now();
    save_state(&project_dir(), &state)
        .map_err(|error| OrquestraError::from(format!("Cannot save state: {error}")))?;

    let artifacts = match build_artifacts(&project_dir(), session_id, &state) {
        Ok(files) => files,
        Err(e) => {
            tracing::warn!("Artifact generation failed: {e}");
            Vec::new()
        }
    };

    print_output(
        &InitApplyOutput {
            session_id: session.id,
            plan_id: plan.title,
            plan_path: plan_path.to_string_lossy().to_string(),
            ticket_count: plan.tickets.len(),
            wave_count: waves.waves.len(),
            brain_candidates,
            artifacts,
        },
        output,
    );
    Ok(())
}

#[derive(Debug, Serialize)]
struct InitClassifyOutput {
    session_id: String,
    intent: String,
    scope: String,
    audience: String,
    confidence: f32,
    reasoning: String,
    minimum_confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    refinement_request_json: Option<String>,
}

impl OutputData for InitClassifyOutput {
    fn render_human(&self) -> String {
        let header = format!(
            "Classification for {}\n  Intent: {} (conf: {:.0}%)\n  Scope: {}\n  Audience: {}\n",
            self.session_id,
            self.intent,
            self.confidence * 100.0,
            self.scope,
            self.audience
        );
        if let Some(ref req) = self.refinement_request_json {
            format!(
                "{}  Confidence < {:.2} - refinement request:\n  {}\n",
                header, self.minimum_confidence, req
            )
        } else {
            header
        }
    }
}

#[derive(Debug, Serialize)]
struct InitAddReqOutput {
    session_id: String,
    requirement_id: String,
    requirement_text: String,
    total_requirements: usize,
}

impl OutputData for InitAddReqOutput {
    fn render_human(&self) -> String {
        format!(
            "Requirement added: {} ({})\n  Total: {} requirements\n",
            self.requirement_id, self.requirement_text, self.total_requirements
        )
    }
}

#[derive(Debug, Serialize)]
struct InitEvaluateOutput {
    session_id: String,
    phase: String,
    round: u32,
    topics_evaluated: usize,
    verdicts: Vec<String>,
    confidence: f32,
    blockers: Vec<String>,
}

impl OutputData for InitEvaluateOutput {
    fn render_human(&self) -> String {
        let verdicts = self.verdicts.join("\n  ");
        format!(
            "Evaluation for {}\n  Phase: {}\n  Round: {}\n  Confidence: {:.0}%\n  Topics: {}\n  {}\n  Blockers: {}\n",
            self.session_id,
            self.phase,
            self.round,
            self.confidence * 100.0,
            self.topics_evaluated,
            verdicts,
            if self.blockers.is_empty() {
                "none".to_string()
            } else {
                self.blockers.join("; ")
            }
        )
    }
}

#[derive(Debug, Serialize)]
struct InitStoreResultsOutput {
    topic_id: String,
    status: String,
    source_count: usize,
    average_score: Option<f32>,
}

impl OutputData for InitStoreResultsOutput {
    fn render_human(&self) -> String {
        format!(
            "Stored results for {}\n  Status: {}\n  Sources: {}\n  Avg score: {}\n",
            self.topic_id,
            self.status,
            self.source_count,
            self.average_score
                .map_or("N/A".into(), |s| format!("{:.1}", s))
        )
    }
}

fn run_classify(
    session_id: &str,
    refinement_response: Option<&str>,
    minimum_confidence: f32,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = classify_init(&project_dir(), session_id, refinement_response)
        .map_err(|error| OrquestraError::from(format!("Cannot classify: {error}")))?;

    let classification = state.classification.as_ref().expect("just classified");

    let refinement_request_json =
        if refinement_response.is_none() && classification.confidence < minimum_confidence {
            let req = RefinementRequest {
                idea: state.idea.clone(),
                heuristic_intent: classification.intent,
                heuristic_scope: classification.scope,
                heuristic_audience: classification.audience,
                heuristic_confidence: classification.confidence,
                requirements_count: state.requirements.items.len(),
                date: orquestra_init::today_date(),
            };
            Some(serde_json::to_string(&req).unwrap_or_default())
        } else {
            None
        };

    print_output(
        &InitClassifyOutput {
            session_id: state.id,
            intent: format!("{:?}", classification.intent),
            scope: format!("{:?}", classification.scope),
            audience: format!("{:?}", classification.audience),
            confidence: classification.confidence,
            reasoning: classification.reasoning.clone(),
            minimum_confidence,
            refinement_request_json,
        },
        output,
    );
    Ok(())
}

fn run_add_requirement(
    session_id: &str,
    text: &str,
    source: &str,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let state = add_requirement(&project_dir(), session_id, text, source)
        .map_err(|error| OrquestraError::from(format!("Cannot add requirement: {error}")))?;

    let last_req = state
        .requirements
        .items
        .last()
        .ok_or_else(|| OrquestraError::from("No requirement was added"))?;

    print_output(
        &InitAddReqOutput {
            session_id: state.id,
            requirement_id: last_req.id.clone(),
            requirement_text: last_req.text.clone(),
            total_requirements: state.requirements.items.len(),
        },
        output,
    );
    Ok(())
}

fn run_evaluate(
    session_id: &str,
    config: &Config,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let (state, reports) =
        orquestra_init::evaluate_session_with_config(&project_dir(), session_id, &config.init)
            .map_err(|e| OrquestraError::from(format!("Cannot evaluate: {e}")))?;

    let verdicts: Vec<String> = reports
        .iter()
        .map(|(topic, report)| format!("{}: {:?}", topic, report.verdict))
        .collect();
    let topics = list_topics(&project_dir(), session_id).unwrap_or_default();
    let assessment = assess_convergence(&state, &topics, &config.init);

    print_output(
        &InitEvaluateOutput {
            session_id: state.id,
            phase: format!("{:?}", state.phase),
            round: state.round,
            topics_evaluated: reports.len(),
            verdicts,
            confidence: assessment.confidence,
            blockers: assessment.blockers,
        },
        output,
    );
    Ok(())
}

fn run_store_research(
    session_id: &str,
    topic_id: &str,
    sources_json: Option<&str>,
    sources_json_file: Option<&PathBuf>,
    markdown_file: Option<&PathBuf>,
    config: &Config,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let sources = if let Some(path) = markdown_file {
        let md = std::fs::read_to_string(path)
            .map_err(|e| OrquestraError::from(format!("Cannot read markdown file: {e}")))?;
        parse_wie_markdown(&md).map_err(OrquestraError::from)?
    } else {
        let json = load_json_arg(sources_json, sources_json_file).map_err(OrquestraError::from)?;
        let json = json.ok_or_else(|| {
            OrquestraError::from(
                "Provide one of --sources-json, --sources-json-file, or --markdown-file",
            )
        })?;
        serde_json::from_str::<Vec<RankedSource>>(&json)
            .map_err(|e| OrquestraError::from(format!("Invalid sources JSON: {e}")))?
    };
    if sources.is_empty() {
        return Err(OrquestraError::from(
            "At least one source is required. Provide a non-empty result.".to_string(),
        ));
    }
    let topic =
        store_results_with_config(&project_dir(), session_id, topic_id, sources, &config.init)
            .map_err(|e| OrquestraError::from(format!("Cannot store results: {e}")))?;
    print_output(
        &InitStoreResultsOutput {
            topic_id: topic.id,
            status: format!("{:?}", topic.status),
            source_count: topic.sources.len(),
            average_score: topic.average_score,
        },
        output,
    );
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResearchDelegationEnvelope {
    session_id: String,
    topic_id: String,
    host: String,
    query: String,
    max_sources: usize,
    tool_hints: BTreeMap<String, String>,
    callback: ResearchCallback,
    instructions: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResearchCallback {
    command: String,
    args: Vec<String>,
    description: String,
}

impl OutputData for ResearchDelegationEnvelope {
    fn render_human(&self) -> String {
        let mut out = format!(
            "Research delegation for topic {}\nHost: {}\nQuery: {}\nMax sources: {}\n",
            self.topic_id, self.host, self.query, self.max_sources
        );
        out.push_str("\nTool hints (resolved from host tool_map):\n");
        for (name, hint) in &self.tool_hints {
            out.push_str(&format!("  {name}: {hint}\n"));
        }
        out.push_str("\nCallback (invoke after dispatching the MCP call):\n");
        out.push_str(&format!(
            "  {} {}\n",
            self.callback.command,
            self.callback.args.join(" ")
        ));
        out.push_str(&format!("  {}\n", self.callback.description));
        out.push_str("\nInstructions:\n");
        for instruction in &self.instructions {
            out.push_str(&format!("  - {instruction}\n"));
        }
        out
    }
}

fn run_request_research(
    session_id: &str,
    topic_id: &str,
    host: &str,
    max_sources: Option<usize>,
    config: &Config,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    validate_init_id(session_id)?;
    let topic = load_research_topic(&project_dir(), session_id, topic_id)
        .map_err(|e| OrquestraError::from(format!("Cannot load topic: {e}")))?;

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

    let callback = ResearchCallback {
        command: "orquestra".to_string(),
        args: vec![
            "init".to_string(),
            "store-research".to_string(),
            "--session-id".to_string(),
            session_id.to_string(),
            "--topic-id".to_string(),
            topic_id.to_string(),
            "--markdown-file".to_string(),
            "<path>".to_string(),
        ],
        description:
            "Write the MCP call's text response to <path>, then run this command with --markdown-file <path>"
                .to_string(),
    };

    let max_sources = max_sources
        .unwrap_or(config.init.research.min_sources_per_topic)
        .max(config.init.research.min_sources_per_topic);
    let envelope = ResearchDelegationEnvelope {
        session_id: session_id.to_string(),
        topic_id: topic.id.clone(),
        host: host.to_string(),
        query: topic.query.clone(),
        max_sources,
        tool_hints,
        callback,
        instructions: vec![
            "Dispatch the host's webSearch tool (see toolHints) with arguments { query, limit: max_sources }."
                .to_string(),
            "Normalize the MCP response into the callback markdown format. Every source must include one or more Claim lines copied or faithfully paraphrased from that source; snippets alone are not corroboration. Write that report to a temp file, then invoke the callback with --markdown-file <path>."
                .to_string(),
            "If the host has no webSearch tool available, the LLM must report the gap and abort the research loop for this topic."
                .to_string(),
            "Do not invent sources. Only sources actually returned by the MCP call may be stored."
                .to_string(),
        ],
    };
    print_output(&envelope, output);
    Ok(())
}

fn parse_wie_markdown(md: &str) -> Result<Vec<RankedSource>, String> {
    let mut sources: Vec<RankedSource> = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_url: Option<String> = None;
    let mut current_snippet: Option<String> = None;
    let mut current_claims: Vec<String> = Vec::new();
    let mut current_score: Option<f32> = None;

    let flush_if_ready = |title: &mut Option<String>,
                          url: &mut Option<String>,
                          snippet: &mut Option<String>,
                          claims: &mut Vec<String>,
                          score: &mut Option<f32>| {
        if let (Some(t), Some(u)) = (title.take(), url.take()) {
            let raw_authority = orquestra_init::authority_for_url(&u);
            let raw_score = score.take().unwrap_or(0.6);
            let now = chrono::Utc::now();
            let mut src = RankedSource {
                url: u,
                title: t,
                authority: raw_authority,
                recency: 1.0,
                relevance: raw_score,
                agreement: 0.0,
                score: 0.0,
                claims: std::mem::take(claims),
                snippet: snippet.take().filter(|value| !value.is_empty()),
                fetched_at: now,
            };
            src.compute_score();
            return Some(src);
        }
        None
    };

    for line in md.lines() {
        let line = line.trim();
        if let Some(score) = parse_wie_score(line) {
            current_score = Some(score);
        }
        if let Some(rest) = line.strip_prefix("### ") {
            if let Some(src) = flush_if_ready(
                &mut current_title,
                &mut current_url,
                &mut current_snippet,
                &mut current_claims,
                &mut current_score,
            ) {
                sources.push(src);
            }
            let title_text = rest.trim();
            let title_text = strip_numbering_prefix(title_text);
            current_title = Some(title_text.to_string());
        } else if let Some(rest) = line.strip_prefix("URL:") {
            current_url = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("Claims:") {
            current_claims.extend(parse_wie_claims(rest));
        } else if let Some(rest) = line.strip_prefix("Claim:") {
            current_claims.extend(parse_wie_claims(rest));
        } else if let Some(rest) = line.strip_prefix("Snippet:") {
            current_snippet = Some(rest.trim().to_string());
        }
    }
    if let Some(src) = flush_if_ready(
        &mut current_title,
        &mut current_url,
        &mut current_snippet,
        &mut current_claims,
        &mut current_score,
    ) {
        sources.push(src);
    }

    if sources.is_empty() {
        return Err("No sources parsed from markdown. Expected format: '### N. Title\\nURL: ...\\nScore: N/100\\nClaim: ...\\nSnippet: ...'".to_string());
    }
    if sources.iter().any(|source| source.claims.is_empty()) {
        return Err(
            "The MCP response must be normalized before storage: every source requires at least one Claim line; snippets alone are not corroborated evidence."
                .to_string(),
        );
    }
    Ok(sources)
}

fn parse_wie_claims(value: &str) -> Vec<String> {
    value
        .split(['|', ';'])
        .map(orquestra_init::normalize_claim)
        .filter(|claim| !claim.is_empty())
        .collect()
}

fn parse_wie_score(line: &str) -> Option<f32> {
    let rest = line.split_once("Score:")?.1.trim();
    let token = rest.split_whitespace().next()?.trim_end_matches("/100");
    token
        .parse::<f32>()
        .ok()
        .map(|score| (score / 100.0).clamp(0.0, 1.0))
}

fn strip_numbering_prefix(title: &str) -> &str {
    if let Some((num, rest)) = title.split_once(". ")
        && num.chars().all(|c| c.is_ascii_digit())
        && rest.starts_with(|c: char| c.is_ascii_uppercase())
    {
        rest
    } else {
        title
    }
}

fn convert_draft_to_plan(draft: &PlanDraft) -> Plan {
    let tickets: Vec<Ticket> = draft
        .tickets
        .iter()
        .map(|dt| Ticket {
            id: dt.id.clone(),
            title: dt.title.clone(),
            objective: dt.objective.clone(),
            acceptance_criteria: dt.acceptance_criteria.clone(),
            blocked_by: dt.blocked_by.clone(),
            preferred_capabilities: dt.preferred_capabilities.clone(),
            assigned_skill: dt.assigned_skill.clone(),
            model_policy: None,
            model_recommendation: None,
            verification: VerificationPolicy {
                minimum_score: 0.95,
                required_evidence: vec!["artifact".to_string()],
            },
        })
        .collect();

    Plan {
        schema_version: 1,
        title: draft.title.clone(),
        model_policy: None,
        tickets,
    }
}

fn run_list(output: &OutputFormat) -> Result<(), OrquestraError> {
    let ids = list_sessions(&project_dir())
        .map_err(|error| OrquestraError::from(format!("Cannot list init sessions: {error}")))?;
    let mut sessions = Vec::new();
    for id in &ids {
        if let Ok(state) = load_state(&project_dir(), id) {
            sessions.push(InitSessionSummary {
                id: state.id,
                idea: state.idea,
                phase: format!("{:?}", state.phase),
                round: state.round,
            });
        }
    }
    print_output(&InitListOutput { sessions }, output);
    Ok(())
}

fn validate_init_id(id: &str) -> Result<(), OrquestraError> {
    orquestra_init::validate_init_id(id)
        .map_err(|error| OrquestraError::from(format!("Invalid init session ID: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIE_SAMPLE: &str = "## Search Results — 2 results\nQuery: `Rust async tokio 2026`  |  Engines: google, duckduckgo, bing, wikipedia, startpage\n\n### 1. Rust Async and Tokio 2026\nURL: https://example.com/rust-async-2026\nSource: 🔗 example.com\nScore: 75/100\nClaim: async Rust uses Tokio in production systems\nSnippet: Async Rust and Tokio matured into production-ready technologies.\n\n### 2. Rust Async Programming with Tokio\nURL: https://rust-lang.org/async-tokio-guide\nSource: 🔗 rust-lang.org\nScore: 90/100\nClaim: async Rust uses Tokio in production systems\nSnippet: Tokio powers Discord, AWS, and Cloudflare's Rust services.\n";

    #[test]
    fn parse_wie_markdown_extracts_sources() {
        let sources = parse_wie_markdown(WIE_SAMPLE).expect("parse markdown");
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].url, "https://example.com/rust-async-2026");
        assert_eq!(sources[0].title, "Rust Async and Tokio 2026");
        assert!((sources[0].authority - 0.6).abs() < f32::EPSILON);
        assert!((sources[0].relevance - 0.75).abs() < f32::EPSILON);
        assert_eq!(sources[0].agreement, 0.0);
        assert_eq!(sources[0].score, 0.0);
        assert!((sources[1].authority - 1.0).abs() < f32::EPSILON);
        assert_eq!(sources[1].url, "https://rust-lang.org/async-tokio-guide");
    }

    #[test]
    fn parse_wie_markdown_official_score_preserves_gate_input() {
        let markdown = "### 1. Node.js documentation\n\
URL: https://nodejs.org/api/http.html\n\
Source: nodejs.org\n\
Score: 95/100\n\
Claim: node provides an HTTP client API\n\
Snippet: Primary runtime documentation.";
        let sources = parse_wie_markdown(markdown).expect("parse markdown");
        let source = &sources[0];

        assert!((0.0..=1.0).contains(&source.authority));
        assert!((0.0..=1.0).contains(&source.recency));
        assert!((0.0..=1.0).contains(&source.agreement));
        assert!((0.0..=1.0).contains(&source.score));
        assert!(source.relevance >= 0.95);
        assert_eq!(source.agreement, 0.0);
        assert_eq!(source.score, 0.0);
    }

    #[test]
    fn parse_wie_markdown_reads_inline_source_score() {
        let markdown = "### 1. Node.js documentation\n\
URL: https://nodejs.org/api/http.html\n\
Source: nodejs.org  Score: 95/100\n\
Claim: node provides an HTTP client API\n\
Snippet: Primary runtime documentation.";
        let sources = parse_wie_markdown(markdown).expect("parse markdown");
        assert!(sources[0].relevance >= 0.95);
        assert_eq!(sources[0].agreement, 0.0);
        assert_eq!(sources[0].score, 0.0);
    }

    #[test]
    fn parse_wie_markdown_does_not_trust_spoofed_source_domain() {
        let markdown = "### 1. Spoofed result\n\
URL: https://evil.example.com/payload\n\
Source: nodejs.org\n\
Score: 100/100\n\
Claim: untrusted content must not control authority\n\
Snippet: Untrusted content.";
        let sources = parse_wie_markdown(markdown).expect("parse markdown");
        assert!((sources[0].authority - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_wie_markdown_does_not_use_snippet_as_claim_agreement() {
        let markdown = "### 1. Node.js documentation\n\
URL: https://nodejs.org/api/fs.html\n\
Source: nodejs.org\n\
Score: 95/100\n\
Claim:  Use   async I/O \n\
Snippet: Example text returned by search.";
        let sources = parse_wie_markdown(markdown).expect("parse markdown");
        let source = &sources[0];

        assert_eq!(source.agreement, 0.0);
        assert!((source.relevance - 0.95).abs() < f32::EPSILON);
        assert_eq!(source.claims, vec!["use async i/o"]);
        assert_eq!(
            source.snippet.as_deref(),
            Some("Example text returned by search.")
        );
    }

    #[test]
    fn parse_wie_markdown_rejects_unusable_input() {
        let err = parse_wie_markdown("no headers here\nnothing useful").expect_err("must reject");
        assert!(err.contains("No sources parsed"));

        let raw_snippet =
            "### 1. Result\nURL: https://nodejs.org/api/http.html\nSnippet: unverified text";
        let err = parse_wie_markdown(raw_snippet).expect_err("raw snippets must be normalized");
        assert!(err.contains("requires at least one Claim"));
    }

    #[test]
    fn delegation_envelope_resolves_tool_hints_per_host() {
        let envelope_for_opencode = build_envelope_for_test("sid", "tid", "opencode", 4);
        assert!(envelope_for_opencode.tool_hints.contains_key("webSearch"));
        assert_eq!(
            envelope_for_opencode.tool_hints.get("webSearch").unwrap(),
            "WebSearch tool"
        );

        let envelope_for_antigravity = build_envelope_for_test("sid", "tid", "antigravity", 4);
        assert_eq!(
            envelope_for_antigravity
                .tool_hints
                .get("webSearch")
                .unwrap(),
            "WIE web_search_advanced"
        );

        let envelope_for_codex = build_envelope_for_test("sid", "tid", "codex", 4);
        assert_eq!(
            envelope_for_codex.tool_hints.get("webSearch").unwrap(),
            "MCP search_web"
        );
    }

    fn build_envelope_for_test(
        sid: &str,
        tid: &str,
        host: &str,
        max_sources: usize,
    ) -> ResearchDelegationEnvelope {
        let adapter = get_adapter(host).expect("adapter");
        let tool_hints = ["webSearch", "webFetch"]
            .into_iter()
            .filter_map(|k| {
                adapter
                    .tool_map()
                    .get(k)
                    .map(|v| (k.to_string(), v.to_string()))
            })
            .collect::<BTreeMap<_, _>>();
        ResearchDelegationEnvelope {
            session_id: sid.to_string(),
            topic_id: tid.to_string(),
            host: host.to_string(),
            query: "test query".to_string(),
            max_sources,
            tool_hints,
            callback: ResearchCallback {
                command: "orquestra-cli".to_string(),
                args: vec!["init".to_string(), "store-research".to_string()],
                description: "test".to_string(),
            },
            instructions: vec![],
        }
    }
}
