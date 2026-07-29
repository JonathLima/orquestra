use crate::{
    RuntimeError, Session, SessionEvent, SessionStatus, TicketStatus, evaluate_report,
    load_verification_report, storage,
};
use orquestra_core::security::redact_secrets;
use orquestra_plan::{ModelRecommendation, Plan, Ticket, VerificationPolicy, WaveResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DispatchMode {
    Manual,
    Host { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketManifest {
    pub session_id: String,
    pub ticket_id: String,
    pub dispatch_attempt_id: String,
    pub wave: u32,
    pub title: String,
    pub objective: String,
    pub acceptance_criteria: Vec<String>,
    pub assigned_skill: Option<String>,
    pub model_recommendation: Option<ModelRecommendation>,
    pub verification: VerificationPolicy,
    pub dispatch_mode: DispatchMode,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketDispatch {
    pub ticket_id: String,
    pub assigned_skill: Option<String>,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaveDispatch {
    pub session_id: String,
    pub wave: u32,
    pub dispatch_mode: DispatchMode,
    pub tickets: Vec<TicketDispatch>,
}

const MAX_TICKET_RETRIES: u32 = 3;

pub fn dispatch_wave(
    project_dir: &Path,
    session_id: &str,
    requested_wave: Option<u32>,
    dispatch_mode: DispatchMode,
) -> Result<WaveDispatch, RuntimeError> {
    let _lock = storage::acquire_session_lock(project_dir, session_id)?;
    let session = crate::load_session(project_dir, session_id)?;
    let wave = requested_wave.unwrap_or(session.current_wave);
    ensure_dispatchable(&session, wave)?;

    let plan: Plan = storage::read_json(&storage::plan_file(project_dir, session_id)?)?;
    let waves: WaveResult = storage::read_json(&storage::waves_file(project_dir, session_id)?)?;
    let wave_ticket_ids = waves
        .waves
        .iter()
        .find(|item| item.wave_number == wave)
        .map(|item| item.ticket_ids.clone())
        .ok_or_else(|| RuntimeError::InvalidTransition(format!("Wave {wave} does not exist")))?;

    let mut dispatches = Vec::new();
    let mut model_recommendations = HashMap::new();
    let mut attempt_ids = HashMap::new();
    for ticket_id in &wave_ticket_ids {
        let state = session
            .ticket_states
            .get(ticket_id)
            .ok_or_else(|| RuntimeError::InvalidTicketId(ticket_id.clone()))?;
        if state.status != TicketStatus::Pending {
            continue;
        }
        let ticket = plan
            .tickets
            .iter()
            .find(|item| &item.id == ticket_id)
            .ok_or_else(|| RuntimeError::InvalidTicketId(ticket_id.clone()))?;
        let assigned_skill = state
            .assigned_skill
            .as_ref()
            .or(ticket.assigned_skill.as_ref())
            .ok_or_else(|| {
                RuntimeError::InvalidTransition(format!(
                    "Cannot dispatch ticket {}: assigned skill is required",
                    ticket.id
                ))
            })?;
        let mut effective_ticket = ticket.clone();
        effective_ticket.assigned_skill = Some(assigned_skill.clone());
        let model_recommendation = orquestra_plan::model::recommend_for_ticket(
            &effective_ticket,
            plan.model_policy.as_ref(),
            host_name(&dispatch_mode).as_deref(),
        )
        .map_err(RuntimeError::Other)?;
        let attempt_id = uuid::Uuid::new_v4().to_string();
        model_recommendations.insert(ticket.id.clone(), model_recommendation.clone());
        attempt_ids.insert(ticket.id.clone(), attempt_id.clone());
        let manifest = manifest_for_ticket(
            session_id,
            &attempt_id,
            wave,
            &effective_ticket,
            dispatch_mode.clone(),
            Some(model_recommendation.clone()),
        );
        let path = storage::ticket_file(project_dir, session_id, &ticket.id)?;
        storage::atomic_write_json(&path, &manifest)?;
        dispatches.push(TicketDispatch {
            ticket_id: ticket.id.clone(),
            assigned_skill: Some(assigned_skill.clone()),
            manifest_path: path,
        });
    }
    if dispatches.is_empty() {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot dispatch wave {wave}: no pending tickets"
        )));
    }

    let mut next_session = session.clone();
    next_session.status = SessionStatus::Running;
    next_session.current_wave = wave;
    next_session.updated_at = crate::iso_now();
    for ticket_id in &wave_ticket_ids {
        if let Some(state) = next_session.ticket_states.get_mut(ticket_id)
            && state.status == TicketStatus::Pending
        {
            state.status = TicketStatus::Running;
            state.dispatch_attempt_id = attempt_ids.get(ticket_id).cloned();
        }
        if let Some(model_recommendation) = model_recommendations.get(ticket_id)
            && let Some(state) = next_session.ticket_states.get_mut(ticket_id)
        {
            state.model_recommendation = Some(model_recommendation.clone());
        }
    }

    crate::commit_session_transition(
        project_dir,
        session_id,
        &session,
        &next_session,
        SessionEvent {
            ts: crate::iso_now(),
            session_id: session_id.to_string(),
            event: "wave_dispatched".to_string(),
            data: serde_json::json!({
                "wave": wave,
                "tickets": dispatches.iter().map(|item| item.ticket_id.clone()).collect::<Vec<_>>(),
                "attemptIds": attempt_ids,
            }),
        },
    )?;

    Ok(WaveDispatch {
        session_id: session_id.to_string(),
        wave,
        dispatch_mode,
        tickets: dispatches,
    })
}

pub fn record_ticket_result(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
    completed: bool,
    output: Option<String>,
    evidence: Vec<String>,
) -> Result<Session, RuntimeError> {
    storage::validate_ticket_id(ticket_id)?;
    let _lock = storage::acquire_session_lock(project_dir, session_id)?;
    let session = crate::load_session(project_dir, session_id)?;
    if session.status != SessionStatus::Running {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot record ticket {ticket_id}: session is not running"
        )));
    }
    ensure_ticket_recordable(project_dir, &session, ticket_id, completed)?;

    let mut next_session = session.clone();
    let wave = {
        let state = next_session
            .ticket_states
            .get_mut(ticket_id)
            .ok_or_else(|| RuntimeError::InvalidTicketId(ticket_id.to_string()))?;
        state.status = if completed {
            TicketStatus::Completed
        } else {
            state.retries += 1;
            TicketStatus::Failed
        };
        state.output = output.map(|value| redact_secrets(&value));
        state.evidence = evidence
            .into_iter()
            .map(|value| redact_secrets(&value))
            .collect();
        state.wave
    };
    next_session.updated_at = crate::iso_now();

    if completed && wave_complete(&next_session, wave) {
        if wave >= next_session.total_waves {
            next_session.status = SessionStatus::Completed;
        } else {
            next_session.status = SessionStatus::Checkpoint;
            persist_wave_checkpoint(project_dir, session_id, wave)?;
        }
    }

    let event_name = if completed {
        "ticket_completed"
    } else {
        "ticket_failed"
    };
    crate::commit_session_transition(
        project_dir,
        session_id,
        &session,
        &next_session,
        SessionEvent {
            ts: crate::iso_now(),
            session_id: session_id.to_string(),
            event: event_name.to_string(),
            data: serde_json::json!({"ticket": ticket_id, "wave": wave}),
        },
    )?;

    Ok(next_session)
}

pub fn reroute_ticket(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
    reason: &str,
) -> Result<Session, RuntimeError> {
    reroute_ticket_to_skill(project_dir, session_id, ticket_id, reason, None)
}

pub fn reroute_ticket_to_skill(
    project_dir: &Path,
    session_id: &str,
    ticket_id: &str,
    reason: &str,
    assigned_skill: Option<&str>,
) -> Result<Session, RuntimeError> {
    storage::validate_ticket_id(ticket_id)?;
    let assigned_skill = assigned_skill
        .map(str::trim)
        .filter(|skill| !skill.is_empty())
        .map(str::to_string);
    let _lock = storage::acquire_session_lock(project_dir, session_id)?;
    let session = crate::load_session(project_dir, session_id)?;
    if session.status != SessionStatus::Running {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot reroute ticket {ticket_id}: session is not running"
        )));
    }

    let mut next_session = session.clone();
    let current_wave = next_session.current_wave;
    let retries = {
        let state = next_session
            .ticket_states
            .get_mut(ticket_id)
            .ok_or_else(|| RuntimeError::InvalidTicketId(ticket_id.to_string()))?;
        if state.status != TicketStatus::Failed {
            return Err(RuntimeError::InvalidTransition(format!(
                "Cannot reroute ticket {ticket_id}: ticket is not failed"
            )));
        }
        if state.wave != current_wave {
            return Err(RuntimeError::InvalidTransition(format!(
                "Cannot reroute ticket {ticket_id}: ticket is not in the current wave"
            )));
        }
        if state.retries >= MAX_TICKET_RETRIES {
            return Err(RuntimeError::InvalidTransition(format!(
                "Cannot reroute ticket {ticket_id}: retry limit of {MAX_TICKET_RETRIES} reached"
            )));
        }
        state.status = TicketStatus::Pending;
        state.dispatch_attempt_id = None;
        state.model_recommendation = None;
        state.output = None;
        state.evidence.clear();
        if assigned_skill.is_some() {
            state.assigned_skill = assigned_skill.clone();
        }
        state.retries
    };
    next_session.updated_at = crate::iso_now();

    crate::commit_session_transition(
        project_dir,
        session_id,
        &session,
        &next_session,
        SessionEvent {
            ts: crate::iso_now(),
            session_id: session_id.to_string(),
            event: "ticket_rerouted".to_string(),
            data: serde_json::json!({
                "ticket": ticket_id,
                "retries": retries,
                "reason": redact_secrets(reason),
                "assignedSkill": assigned_skill,
            }),
        },
    )?;

    Ok(next_session)
}

pub fn approve_wave(
    project_dir: &Path,
    session_id: &str,
    wave: u32,
    notes: Option<String>,
) -> Result<Session, RuntimeError> {
    let _lock = storage::acquire_session_lock(project_dir, session_id)?;
    let session = crate::load_session(project_dir, session_id)?;
    if session.status != SessionStatus::Checkpoint {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot approve wave {wave}: session is not at a checkpoint"
        )));
    }
    if session.current_wave != wave {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot approve wave {wave}: current checkpoint is wave {}",
            session.current_wave
        )));
    }
    if !wave_complete(&session, wave) {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot approve wave {wave}: wave is not complete"
        )));
    }

    let checkpoint_path = storage::checkpoint_file(project_dir, session_id, wave)?;
    let mut checkpoint: crate::Checkpoint = storage::read_json(&checkpoint_path)?;
    checkpoint.approved = Some(true);
    checkpoint.approved_at = Some(crate::iso_now());
    checkpoint.notes = notes;
    // Write checkpoint FIRST so a crash between the two writes leaves the
    // session at Checkpoint with an already-approved checkpoint file; the
    // operation is then idempotently recoverable by re-running approve_wave.
    // The atomic_write_json now also fsyncs the parent directory so the
    // checkpoint entry is durable before we transition the session.
    storage::atomic_write_json(&checkpoint_path, &checkpoint)?;

    let mut next_session = session.clone();
    next_session.status = SessionStatus::Running;
    next_session.current_wave = wave + 1;
    next_session.updated_at = crate::iso_now();
    crate::commit_session_transition(
        project_dir,
        session_id,
        &session,
        &next_session,
        SessionEvent {
            ts: crate::iso_now(),
            session_id: session_id.to_string(),
            event: "wave_approved".to_string(),
            data: serde_json::json!({"wave": wave}),
        },
    )?;
    Ok(next_session)
}

fn ensure_dispatchable(session: &Session, wave: u32) -> Result<(), RuntimeError> {
    if wave == 0 || wave > session.total_waves {
        return Err(RuntimeError::InvalidTransition(format!(
            "Dispatch wave {wave} is outside valid range 1..={}",
            session.total_waves
        )));
    }
    if matches!(
        session.status,
        SessionStatus::Completed | SessionStatus::Cancelled
    ) {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot dispatch terminal session {}",
            session.id
        )));
    }
    if session.status == SessionStatus::Checkpoint {
        return Err(RuntimeError::InvalidTransition(format!(
            "Approve checkpoint wave {} before dispatching wave {wave}",
            session.current_wave
        )));
    }
    if session.status != SessionStatus::Created && session.status != SessionStatus::Running {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot dispatch session {} in status {:?}",
            session.id, session.status
        )));
    }
    if wave != session.current_wave {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot dispatch wave {wave}: current wave is {}",
            session.current_wave
        )));
    }
    if !prior_waves_complete(session, wave) {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot dispatch wave {wave}: prior waves are not complete"
        )));
    }
    Ok(())
}

fn ensure_ticket_recordable(
    project_dir: &Path,
    session: &Session,
    ticket_id: &str,
    completed: bool,
) -> Result<(), RuntimeError> {
    let state = session
        .ticket_states
        .get(ticket_id)
        .ok_or_else(|| RuntimeError::InvalidTicketId(ticket_id.to_string()))?;
    if state.status != TicketStatus::Running {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot record ticket {ticket_id}: ticket is not running"
        )));
    }
    if state.wave != session.current_wave {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot record ticket {ticket_id}: ticket wave {} is not current wave {}",
            state.wave, session.current_wave
        )));
    }
    if completed {
        ensure_completion_verified(project_dir, session, ticket_id)?;
    }
    Ok(())
}

fn ensure_completion_verified(
    project_dir: &Path,
    session: &Session,
    ticket_id: &str,
) -> Result<(), RuntimeError> {
    let plan: Plan = storage::read_json(&storage::plan_file(project_dir, &session.id)?)?;
    let ticket = plan
        .tickets
        .iter()
        .find(|ticket| ticket.id == ticket_id)
        .ok_or_else(|| RuntimeError::InvalidTicketId(ticket_id.to_string()))?;
    ensure_research_verified(project_dir, session, ticket_id)?;
    let report = load_verification_report(project_dir, &session.id, ticket_id)
        .map_err(|error| RuntimeError::Other(format!("verification report required: {error}")))?;
    if report.session_id != session.id || report.ticket_id != ticket_id {
        return Err(RuntimeError::Other(format!(
            "verification report identity mismatch for ticket {ticket_id}"
        )));
    }
    let expected_attempt_id = session
        .ticket_states
        .get(ticket_id)
        .and_then(|state| state.dispatch_attempt_id.as_deref())
        .ok_or_else(|| {
            RuntimeError::Other(format!("missing dispatch attempt for ticket {ticket_id}"))
        })?;
    if report.dispatch_attempt_id.as_deref() != Some(expected_attempt_id) {
        return Err(RuntimeError::Other(format!(
            "verification dispatch attempt mismatch for ticket {ticket_id}"
        )));
    }
    let expected_skill = session
        .ticket_states
        .get(ticket_id)
        .and_then(|state| state.assigned_skill.as_ref())
        .or(ticket.assigned_skill.as_ref())
        .ok_or_else(|| {
            RuntimeError::Other(format!(
                "verification skill mismatch for ticket {ticket_id}: assigned skill is required"
            ))
        })?;
    if !expected_skill.eq_ignore_ascii_case(&report.skill_name) {
        return Err(RuntimeError::Other(format!(
            "verification skill mismatch for ticket {ticket_id}: expected {expected_skill}, got {}",
            report.skill_name
        )));
    }
    let outcome = evaluate_report(&ticket.verification, &report)?;
    if !outcome.passed {
        return Err(RuntimeError::Other(format!(
            "verification failed for ticket {ticket_id}: {}",
            outcome.reasons.join("; ")
        )));
    }
    Ok(())
}

fn ensure_research_verified(
    project_dir: &Path,
    session: &Session,
    ticket_id: &str,
) -> Result<(), RuntimeError> {
    let Some(state) = session.ticket_states.get(ticket_id) else {
        return Err(RuntimeError::InvalidTicketId(ticket_id.to_string()));
    };
    let web_required = state
        .model_recommendation
        .as_ref()
        .map(|recommendation| recommendation.web_required)
        .unwrap_or(false);
    if !web_required {
        return Ok(());
    }

    let report = crate::research::load_research_report(project_dir, &session.id, ticket_id)
        .map_err(|error| {
            RuntimeError::Other(format!(
                "validated research report required for ticket {ticket_id}: {error}"
            ))
        })?;
    if let Some(report_session_id) = &report.session_id
        && report_session_id != &session.id
    {
        return Err(RuntimeError::Other(format!(
            "validated research report identity mismatch for ticket {ticket_id}"
        )));
    }
    let validation = crate::research::validate_research_report(&report);
    if !validation.valid {
        return Err(RuntimeError::Other(format!(
            "validated research report required for ticket {ticket_id}: {}",
            validation.errors.join("; ")
        )));
    }
    Ok(())
}

fn manifest_for_ticket(
    session_id: &str,
    dispatch_attempt_id: &str,
    wave: u32,
    ticket: &Ticket,
    dispatch_mode: DispatchMode,
    model_recommendation: Option<ModelRecommendation>,
) -> TicketManifest {
    let prompt = format!(
        "Use skill `{skill}` to complete ticket `{ticket_id}`.\n\nObjective:\n{objective}\n\nAcceptance criteria:\n{criteria}\n\nReturn a concise implementation summary and evidence for verification.",
        skill = ticket
            .assigned_skill
            .as_deref()
            .unwrap_or("best matching local skill"),
        ticket_id = ticket.id,
        objective = ticket.objective,
        criteria = ticket.acceptance_criteria.join("\n- "),
    );
    TicketManifest {
        session_id: session_id.to_string(),
        ticket_id: ticket.id.clone(),
        dispatch_attempt_id: dispatch_attempt_id.to_string(),
        wave,
        title: ticket.title.clone(),
        objective: ticket.objective.clone(),
        acceptance_criteria: ticket.acceptance_criteria.clone(),
        assigned_skill: ticket.assigned_skill.clone(),
        model_recommendation,
        verification: ticket.verification.clone(),
        dispatch_mode,
        prompt,
    }
}

fn host_name(dispatch_mode: &DispatchMode) -> Option<String> {
    match dispatch_mode {
        DispatchMode::Manual => None,
        DispatchMode::Host { name } => Some(name.clone()),
    }
}

fn wave_complete(session: &Session, wave: u32) -> bool {
    session
        .ticket_states
        .values()
        .filter(|state| state.wave == wave)
        .all(|state| state.status == TicketStatus::Completed)
}

fn prior_waves_complete(session: &Session, wave: u32) -> bool {
    session
        .ticket_states
        .values()
        .filter(|state| state.wave < wave)
        .all(|state| state.status == TicketStatus::Completed)
}

fn persist_wave_checkpoint(
    project_dir: &Path,
    session_id: &str,
    wave: u32,
) -> Result<(), RuntimeError> {
    let checkpoint = crate::Checkpoint {
        wave_number: wave,
        created_at: crate::iso_now(),
        approved: None,
        approved_at: None,
        notes: None,
    };
    storage::atomic_write_json(
        &storage::checkpoint_file(project_dir, session_id, wave)?,
        &checkpoint,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        VerificationReport, create_session, load_session, save_verification_report, start_session,
    };
    use orquestra_plan::{Plan, Ticket, VerificationPolicy, Wave, WaveResult};

    fn plan() -> Plan {
        Plan {
            schema_version: 1,
            title: "Dispatch".to_string(),
            model_policy: None,
            tickets: vec![
                Ticket {
                    id: "T1".to_string(),
                    title: "One".to_string(),
                    objective: "Do one".to_string(),
                    acceptance_criteria: vec!["one done".to_string()],
                    blocked_by: vec![],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("skill-one".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
                Ticket {
                    id: "T2".to_string(),
                    title: "Two".to_string(),
                    objective: "Do two".to_string(),
                    acceptance_criteria: vec!["two done".to_string()],
                    blocked_by: vec!["T1".to_string()],
                    preferred_capabilities: vec![],
                    assigned_skill: Some("skill-two".to_string()),
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
            ],
        }
    }

    fn waves() -> WaveResult {
        WaveResult {
            total_waves: 2,
            total_tickets: 2,
            waves: vec![
                Wave {
                    wave_number: 1,
                    ticket_ids: vec!["T1".to_string()],
                },
                Wave {
                    wave_number: 2,
                    ticket_ids: vec!["T2".to_string()],
                },
            ],
        }
    }

    fn passing_report(
        session_id: &str,
        ticket_id: &str,
        attempt_id: &str,
        skill_name: &str,
    ) -> VerificationReport {
        VerificationReport {
            session_id: session_id.to_string(),
            ticket_id: ticket_id.to_string(),
            dispatch_attempt_id: Some(attempt_id.to_string()),
            skill_name: skill_name.to_string(),
            score: 0.96,
            summary: "verified".to_string(),
            evidence: vec![crate::Evidence {
                kind: "diff".to_string(),
                description: "reviewed diff".to_string(),
                path: None,
            }],
        }
    }

    fn attempt_id(dir: &Path, session_id: &str, ticket_id: &str) -> String {
        load_session(dir, session_id)
            .expect("load session")
            .ticket_states
            .get(ticket_id)
            .and_then(|state| state.dispatch_attempt_id.clone())
            .expect("dispatch attempt id")
    }

    #[test]
    fn dispatch_wave_writes_ticket_manifest_and_marks_running() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");

        let dispatch = dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual)
            .expect("dispatch wave");

        assert_eq!(dispatch.wave, 1);
        assert_eq!(dispatch.tickets.len(), 1);
        assert!(dispatch.tickets[0].manifest_path.exists());
        let loaded = load_session(dir.path(), &session.id).expect("load session");
        assert_eq!(loaded.ticket_states["T1"].status, TicketStatus::Running);
    }

    #[test]
    fn dispatch_requires_resolved_assigned_skill() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut plan = plan();
        plan.tickets[0].assigned_skill = None;
        let session = create_session(dir.path(), &plan, &waves()).expect("create session");

        let error = dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual)
            .expect_err("dispatch without resolved skill must fail");

        assert!(error.to_string().contains("assigned skill"));
    }

    #[test]
    fn dispatch_rejects_duplicate_wave_without_pending_tickets() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");

        let error = dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual)
            .expect_err("duplicate dispatch must fail");

        assert!(error.to_string().contains("no pending tickets"));
    }

    #[test]
    fn completing_wave_creates_checkpoint_and_approval_advances() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");
        let attempt_id = attempt_id(dir.path(), &session.id, "T1");
        save_verification_report(
            dir.path(),
            &passing_report(&session.id, "T1", &attempt_id, "skill-one"),
        )
        .expect("save verification report");

        let checkpointed = record_ticket_result(
            dir.path(),
            &session.id,
            "T1",
            true,
            Some("done".to_string()),
            vec!["test".to_string()],
        )
        .expect("complete ticket");

        assert_eq!(checkpointed.status, SessionStatus::Checkpoint);
        assert!(
            storage::checkpoint_file(dir.path(), &session.id, 1)
                .expect("checkpoint path")
                .exists()
        );

        let advanced =
            approve_wave(dir.path(), &session.id, 1, Some("approved".to_string())).unwrap();
        assert_eq!(advanced.status, SessionStatus::Running);
        assert_eq!(advanced.current_wave, 2);
    }

    #[test]
    fn completing_ticket_without_verification_report_is_rejected() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");

        let error = record_ticket_result(
            dir.path(),
            &session.id,
            "T1",
            true,
            Some("done".to_string()),
            vec!["test".to_string()],
        )
        .expect_err("completion without verification report must fail");

        assert!(error.to_string().contains("verification"));
        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load unchanged session")
                .ticket_states["T1"]
                .status,
            TicketStatus::Running
        );
    }

    #[test]
    fn reroute_returns_a_failed_ticket_to_pending_with_a_bounded_retry_count() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");
        record_ticket_result(
            dir.path(),
            &session.id,
            "T1",
            false,
            Some("verification failed".to_string()),
            vec![],
        )
        .expect("fail ticket");

        let rerouted = reroute_ticket(dir.path(), &session.id, "T1", "use another skill")
            .expect("reroute failed ticket");

        let state = &rerouted.ticket_states["T1"];
        assert_eq!(state.status, TicketStatus::Pending);
        assert_eq!(state.retries, 1);
    }

    #[test]
    fn dispatch_rejects_future_wave_before_current_wave_is_approved() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");

        let error = dispatch_wave(dir.path(), &session.id, Some(2), DispatchMode::Manual)
            .expect_err("future wave dispatch must fail");

        assert!(error.to_string().contains("current wave"));
    }

    #[test]
    fn dispatch_rejects_checkpoint_until_explicit_approval() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");
        persist_wave_checkpoint(dir.path(), &session.id, 1).expect("checkpoint");
        let mut checkpointed = load_session(dir.path(), &session.id).expect("load session");
        checkpointed.status = SessionStatus::Checkpoint;
        checkpointed.current_wave = 1;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).expect("session path"),
            &checkpointed,
        )
        .expect("write checkpointed session");

        let error = dispatch_wave(dir.path(), &session.id, Some(2), DispatchMode::Manual)
            .expect_err("checkpoint cannot be bypassed");

        assert!(error.to_string().contains("Approve checkpoint"));
    }

    #[test]
    fn approve_wave_rejects_incomplete_wave() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");
        persist_wave_checkpoint(dir.path(), &session.id, 1).expect("checkpoint");
        let mut checkpointed = load_session(dir.path(), &session.id).expect("load session");
        checkpointed.status = SessionStatus::Checkpoint;
        checkpointed.current_wave = 1;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).expect("session path"),
            &checkpointed,
        )
        .expect("write checkpointed session");

        let error = approve_wave(dir.path(), &session.id, 1, None)
            .expect_err("incomplete wave approval must fail");

        assert!(error.to_string().contains("not complete"));
    }

    #[test]
    fn completing_pending_ticket_is_rejected() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        start_session(dir.path(), &session.id).expect("start session");

        let error = record_ticket_result(
            dir.path(),
            &session.id,
            "T1",
            true,
            Some("done".to_string()),
            vec!["test".to_string()],
        )
        .expect_err("pending ticket completion must fail");

        assert!(error.to_string().contains("not running"));
    }

    #[test]
    fn completion_rejects_failing_verification_report() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");
        let attempt_id = attempt_id(dir.path(), &session.id, "T1");
        let mut report = passing_report(&session.id, "T1", &attempt_id, "skill-one");
        report.score = 0.10;
        save_verification_report(dir.path(), &report).expect("save verification report");

        let error = record_ticket_result(
            dir.path(),
            &session.id,
            "T1",
            true,
            Some("done".to_string()),
            vec!["diff".to_string()],
        )
        .expect_err("failing verification report must reject completion");

        assert!(error.to_string().contains("verification failed"));
    }

    #[test]
    fn completion_rejects_verification_report_for_wrong_skill() {
        let dir = tempfile::tempdir().expect("temp dir");
        let session = create_session(dir.path(), &plan(), &waves()).expect("create session");
        dispatch_wave(dir.path(), &session.id, None, DispatchMode::Manual).expect("dispatch wave");
        let attempt_id = attempt_id(dir.path(), &session.id, "T1");
        save_verification_report(
            dir.path(),
            &passing_report(&session.id, "T1", &attempt_id, "other-skill"),
        )
        .expect("save verification report");

        let error = record_ticket_result(
            dir.path(),
            &session.id,
            "T1",
            true,
            Some("done".to_string()),
            vec!["diff".to_string()],
        )
        .expect_err("wrong skill verification report must reject completion");

        assert!(error.to_string().contains("skill"));
    }
}
