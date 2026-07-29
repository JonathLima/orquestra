pub mod dispatch;
pub mod error;
pub mod events;
pub mod research;
pub mod storage;
pub mod types;
pub mod verification;

pub use dispatch::{
    DispatchMode, TicketDispatch, TicketManifest, WaveDispatch, approve_wave, dispatch_wave,
    record_ticket_result, reroute_ticket, reroute_ticket_to_skill,
};
pub use error::RuntimeError;
pub use research::{
    ResearchClaim, ResearchReport, ResearchSource, ResearchValidation, list_research_reports,
    load_research_report, research_report_file, save_research_report, validate_research_report,
};
pub use types::*;
pub use verification::{
    Evidence, VerificationOutcome, VerificationReport, evaluate_report, list_verification_reports,
    load_verification_report, save_verification_report, verification_report_file,
    verify_with_profile,
};

use chrono::Utc;
use orquestra_plan::{Plan, WaveResult};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn iso_now() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn inventory_hash(project_dir: &Path) -> Option<String> {
    let path = project_dir.join(".orquestra").join("skills_inventory.json");
    let content = std::fs::read(path).ok()?;
    let mut inventory: Value = serde_json::from_slice(&content).ok()?;
    let root = inventory.as_object_mut()?;
    root.remove("generatedAt");
    if let Some(skills) = root.get_mut("skills").and_then(Value::as_array_mut) {
        for skill in skills.iter_mut().filter_map(Value::as_object_mut) {
            skill.remove("inspectedAt");
        }
        skills.sort_by_key(|skill| serde_json::to_string(skill).unwrap_or_default());
    }
    if let Some(sources) = root.get_mut("sources").and_then(Value::as_array_mut) {
        sources.sort_by_key(|source| serde_json::to_string(source).unwrap_or_default());
    }
    let canonical = serde_json::to_vec(&inventory).ok()?;
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    Some(format!("semantic:v1:{:x}", hasher.finalize()))
}

pub fn create_session(
    project_dir: &Path,
    plan: &Plan,
    waves: &WaveResult,
) -> Result<Session, RuntimeError> {
    for ticket in &plan.tickets {
        storage::validate_ticket_id(&ticket.id)?;
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = iso_now();

    let mut ticket_states = HashMap::new();
    for ticket in &plan.tickets {
        let wave = waves
            .waves
            .iter()
            .find(|w| w.ticket_ids.contains(&ticket.id))
            .map(|w| w.wave_number)
            .unwrap_or(1);
        ticket_states.insert(
            ticket.id.clone(),
            TicketState {
                id: ticket.id.clone(),
                status: TicketStatus::Pending,
                wave,
                assigned_skill: ticket.assigned_skill.clone(),
                model_recommendation: None,
                dispatch_attempt_id: None,
                output: None,
                evidence: vec![],
                retries: 0,
            },
        );
    }

    let session = Session {
        id: id.clone(),
        plan_title: plan.title.clone(),
        status: SessionStatus::Created,
        total_waves: waves.total_waves,
        current_wave: 1,
        created_at: now.clone(),
        updated_at: now.clone(),
        ticket_states,
        inventory_hash: inventory_hash(project_dir),
    };
    let session_path = storage::session_file(project_dir, &id)?;
    let plan_path = storage::plan_file(project_dir, &id)?;
    let waves_path = storage::waves_file(project_dir, &id)?;
    let log_path = storage::event_log_file(project_dir, &id)?;
    let session_dir = storage::session_dir(project_dir, &id)?;
    let persistence_result = (|| {
        storage::atomic_write_json(&plan_path, plan)?;
        storage::atomic_write_json(&waves_path, waves)?;
        events::append_event(
            &log_path,
            &SessionEvent {
                ts: iso_now(),
                session_id: id.clone(),
                event: "session_created".to_string(),
                data: serde_json::json!({"title": plan.title, "total_waves": waves.total_waves}),
            },
        )?;
        storage::atomic_write_json(&session_path, &session)
    })();
    if let Err(error) = persistence_result {
        let _ = std::fs::remove_dir_all(session_dir);
        if log_path.is_file() {
            let _ = std::fs::remove_file(log_path);
        }
        return Err(error);
    }

    Ok(session)
}

fn load_current_checkpoint(
    project_dir: &Path,
    session: &Session,
) -> Result<Checkpoint, RuntimeError> {
    if session.current_wave == 0 || session.current_wave > session.total_waves {
        return Err(RuntimeError::InvalidTransition(format!(
            "Session {} has invalid current wave {}/{}",
            session.id, session.current_wave, session.total_waves
        )));
    }
    let path = storage::checkpoint_file(project_dir, &session.id, session.current_wave)?;
    let checkpoint: Checkpoint = storage::read_json(&path)?;
    if checkpoint.wave_number != session.current_wave {
        return Err(RuntimeError::InvalidTransition(format!(
            "Checkpoint wave {} does not match session current wave {}",
            checkpoint.wave_number, session.current_wave
        )));
    }
    Ok(checkpoint)
}

fn transition_to_running(
    project_dir: &Path,
    id: &str,
    session: Session,
) -> Result<Session, RuntimeError> {
    let now = iso_now();
    let mut next_session = session.clone();
    next_session.status = SessionStatus::Running;
    next_session.updated_at = now.clone();
    commit_session_transition(
        project_dir,
        id,
        &session,
        &next_session,
        SessionEvent {
            ts: now,
            session_id: id.to_string(),
            event: "session_started".to_string(),
            data: serde_json::json!({"current_wave": next_session.current_wave}),
        },
    )?;
    Ok(next_session)
}

pub(crate) fn commit_session_transition(
    project_dir: &Path,
    id: &str,
    previous: &Session,
    next: &Session,
    event: SessionEvent,
) -> Result<(), RuntimeError> {
    let session_path = storage::session_file(project_dir, id)?;
    storage::atomic_write_json(&session_path, next)?;
    let log_path = storage::event_log_file(project_dir, id)?;
    if let Err(error) = events::append_event(&log_path, &event) {
        if let Err(rollback_error) = storage::atomic_write_json(&session_path, previous) {
            return Err(RuntimeError::Other(format!(
                "{error}; rollback to previous session state failed: {rollback_error}"
            )));
        }
        return Err(error);
    }
    Ok(())
}

pub fn start_session(project_dir: &Path, id: &str) -> Result<Session, RuntimeError> {
    let _lock = storage::acquire_session_lock(project_dir, id)?;
    let session = load_session(project_dir, id)?;
    match session.status {
        SessionStatus::Created => transition_to_running(project_dir, id, session),
        SessionStatus::Checkpoint => Err(RuntimeError::InvalidTransition(format!(
            "Session {id} is waiting for checkpoint approval; use run approve-wave"
        ))),
        _ => Err(RuntimeError::InvalidTransition(format!(
            "Cannot start session {} in status {:?}",
            id, session.status
        ))),
    }
}

pub fn checkpoint_session(
    project_dir: &Path,
    id: &str,
    wave: u32,
) -> Result<Checkpoint, RuntimeError> {
    let _lock = storage::acquire_session_lock(project_dir, id)?;
    let mut session = load_session(project_dir, id)?;
    if session.status != SessionStatus::Running {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot checkpoint session {}: not running",
            id
        )));
    }
    if wave == 0 || wave > session.total_waves {
        return Err(RuntimeError::InvalidTransition(format!(
            "Checkpoint wave {} is outside valid range 1..={}",
            wave, session.total_waves
        )));
    }
    if wave < session.current_wave {
        return Err(RuntimeError::InvalidTransition(format!(
            "Checkpoint wave {} would regress current wave {}",
            wave, session.current_wave
        )));
    }
    if wave != session.current_wave {
        return Err(RuntimeError::InvalidTransition(format!(
            "Checkpoint wave {wave} is not the current wave {}",
            session.current_wave
        )));
    }
    if !wave_complete(&session, wave) {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot checkpoint wave {wave}: wave is not complete"
        )));
    }

    let previous_session = session.clone();
    let now = iso_now();
    let checkpoint = Checkpoint {
        wave_number: wave,
        created_at: now.clone(),
        approved: None,
        approved_at: None,
        notes: None,
    };
    storage::atomic_write_json(
        &storage::checkpoint_file(project_dir, id, wave)?,
        &checkpoint,
    )?;

    session.status = SessionStatus::Checkpoint;
    session.current_wave = wave;
    session.updated_at = now.clone();
    commit_session_transition(
        project_dir,
        id,
        &previous_session,
        &session,
        SessionEvent {
            ts: now,
            session_id: id.to_string(),
            event: "checkpoint_reached".to_string(),
            data: serde_json::json!({"wave": wave}),
        },
    )?;

    Ok(checkpoint)
}

pub fn cancel_session(project_dir: &Path, id: &str) -> Result<Session, RuntimeError> {
    let _lock = storage::acquire_session_lock(project_dir, id)?;
    let mut session = load_session(project_dir, id)?;
    if session.status == SessionStatus::Completed || session.status == SessionStatus::Cancelled {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot cancel session {}: already in terminal state {:?}",
            id, session.status
        )));
    }
    let previous_session = session.clone();
    let now = iso_now();
    session.status = SessionStatus::Cancelled;
    session.updated_at = now.clone();
    commit_session_transition(
        project_dir,
        id,
        &previous_session,
        &session,
        SessionEvent {
            ts: now,
            session_id: id.to_string(),
            event: "session_cancelled".to_string(),
            data: serde_json::json!({"reason": "user_requested"}),
        },
    )?;

    Ok(session)
}

pub fn complete_session(project_dir: &Path, id: &str) -> Result<Session, RuntimeError> {
    let _lock = storage::acquire_session_lock(project_dir, id)?;
    let mut session = load_session(project_dir, id)?;
    if session.status == SessionStatus::Completed || session.status == SessionStatus::Cancelled {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot complete session {}: already in terminal state {:?}",
            id, session.status
        )));
    }
    if !session_complete(&session) {
        return Err(RuntimeError::InvalidTransition(format!(
            "Cannot complete session {id}: tickets are not complete"
        )));
    }
    let previous_session = session.clone();
    let now = iso_now();
    session.status = SessionStatus::Completed;
    session.updated_at = now.clone();
    session.current_wave = session.total_waves;
    commit_session_transition(
        project_dir,
        id,
        &previous_session,
        &session,
        SessionEvent {
            ts: now,
            session_id: id.to_string(),
            event: "session_completed".to_string(),
            data: serde_json::json!({"total_waves": session.total_waves}),
        },
    )?;

    Ok(session)
}

fn wave_complete(session: &Session, wave: u32) -> bool {
    session
        .ticket_states
        .values()
        .filter(|state| state.wave == wave)
        .all(|state| state.status == TicketStatus::Completed)
}

fn session_complete(session: &Session) -> bool {
    session
        .ticket_states
        .values()
        .all(|state| state.status == TicketStatus::Completed)
}

pub fn load_session(project_dir: &Path, id: &str) -> Result<Session, RuntimeError> {
    let path = storage::session_file(project_dir, id)?;
    if !path.exists() {
        return Err(RuntimeError::SessionNotFound(id.to_string()));
    }
    let session: Session = storage::read_json(&path)?;
    if let Some(stored_hash) = &session.inventory_hash {
        let current_hash = inventory_hash(project_dir);
        if current_hash.as_ref() != Some(stored_hash) {
            tracing::warn!(
                "inventory hash mismatch for session {}: stored={}, current={:?}",
                id,
                stored_hash,
                current_hash.as_deref().unwrap_or("none")
            );
        }
    }
    Ok(session)
}

pub fn resume_session(project_dir: &Path, id: &str) -> Result<Session, RuntimeError> {
    let session = load_session(project_dir, id)?;
    match session.status {
        SessionStatus::Checkpoint => {
            load_current_checkpoint(project_dir, &session)?;
            Err(RuntimeError::InvalidTransition(format!(
                "Session {id} is waiting for checkpoint approval; use run approve-wave"
            )))
        }
        _ => Err(RuntimeError::InvalidTransition(format!(
            "Session {} is not in checkpoint state (status: {:?})",
            id, session.status
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orquestra_plan::{Plan, Ticket, VerificationPolicy, Wave, WaveResult};

    fn test_plan() -> Plan {
        Plan {
            schema_version: 1,
            title: "Test".to_string(),
            model_policy: None,
            tickets: vec![
                Ticket {
                    id: "T1".to_string(),
                    title: "Task 1".to_string(),
                    objective: "Do thing".to_string(),
                    acceptance_criteria: vec!["done".to_string()],
                    blocked_by: vec![],
                    preferred_capabilities: vec![],
                    assigned_skill: None,
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
                Ticket {
                    id: "T2".to_string(),
                    title: "Task 2".to_string(),
                    objective: "Do thing 2".to_string(),
                    acceptance_criteria: vec!["done".to_string()],
                    blocked_by: vec!["T1".to_string()],
                    preferred_capabilities: vec![],
                    assigned_skill: None,
                    model_policy: None,
                    model_recommendation: None,
                    verification: VerificationPolicy::default(),
                },
            ],
        }
    }

    fn test_waves() -> WaveResult {
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

    fn tmp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("create temp dir")
    }

    fn create_test_session(dir: &Path) -> Session {
        create_session(dir, &test_plan(), &test_waves()).expect("create test session")
    }

    fn event_log_path(dir: &Path, id: &str) -> std::path::PathBuf {
        storage::event_log_file(dir, id).expect("valid event log path")
    }

    fn checkpoint_path(dir: &Path, id: &str, wave: u32) -> std::path::PathBuf {
        storage::checkpoint_file(dir, id, wave).expect("valid checkpoint path")
    }

    fn block_event_log(dir: &Path, id: &str) {
        let path = event_log_path(dir, id);
        std::fs::remove_file(&path).expect("remove event log");
        std::fs::create_dir(&path).expect("replace event log with directory");
    }

    #[test]
    fn test_create_session_persists_files() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        assert_eq!(session.status, SessionStatus::Created);
        assert!(
            storage::session_file(dir.path(), &session.id)
                .expect("valid session path")
                .exists()
        );
        assert!(
            storage::plan_file(dir.path(), &session.id)
                .expect("valid plan path")
                .exists()
        );
        assert!(
            storage::waves_file(dir.path(), &session.id)
                .expect("valid waves path")
                .exists()
        );
        assert!(event_log_path(dir.path(), &session.id).exists());
    }

    #[test]
    fn test_start_session_updates_status() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        let started = start_session(dir.path(), &session.id).unwrap();
        assert_eq!(started.status, SessionStatus::Running);
        let loaded = load_session(dir.path(), &session.id).unwrap();
        assert_eq!(loaded.status, SessionStatus::Running);
        let events = events::read_events(&event_log_path(dir.path(), &session.id)).unwrap();
        assert!(events.iter().any(|e| e.event == "session_started"));
    }

    #[test]
    fn test_checkpoint_write_and_read() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        start_session(dir.path(), &session.id).unwrap();
        let mut ready = load_session(dir.path(), &session.id).unwrap();
        ready.ticket_states.get_mut("T1").unwrap().status = TicketStatus::Completed;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).unwrap(),
            &ready,
        )
        .expect("write ready session");
        let cp = checkpoint_session(dir.path(), &session.id, 1).unwrap();
        assert_eq!(cp.wave_number, 1);
        assert!(cp.approved.is_none());
        let loaded = load_session(dir.path(), &session.id).unwrap();
        assert_eq!(loaded.status, SessionStatus::Checkpoint);
        assert!(checkpoint_path(dir.path(), &session.id, 1).exists());
    }

    #[test]
    fn test_cancel_session() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        start_session(dir.path(), &session.id).unwrap();
        let cancelled = cancel_session(dir.path(), &session.id).unwrap();
        assert_eq!(cancelled.status, SessionStatus::Cancelled);
        let events = events::read_events(&event_log_path(dir.path(), &session.id)).unwrap();
        assert!(events.iter().any(|e| e.event == "session_cancelled"));
    }

    #[test]
    fn test_resume_from_checkpoint_requires_explicit_approval() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        start_session(dir.path(), &session.id).unwrap();
        let mut ready = load_session(dir.path(), &session.id).unwrap();
        ready.ticket_states.get_mut("T1").unwrap().status = TicketStatus::Completed;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).unwrap(),
            &ready,
        )
        .expect("write ready session");
        checkpoint_session(dir.path(), &session.id, 1).unwrap();
        let error = resume_session(dir.path(), &session.id).unwrap_err();
        assert!(error.to_string().contains("approval"));
    }

    #[test]
    fn test_cancel_from_created() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        let cancelled = cancel_session(dir.path(), &session.id).unwrap();
        assert_eq!(cancelled.status, SessionStatus::Cancelled);
    }

    #[test]
    fn test_list_sessions() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let s1 = create_session(dir.path(), &plan, &waves).unwrap();
        let s2 = create_session(dir.path(), &plan, &waves).unwrap();
        let list = storage::list_sessions(dir.path()).unwrap();
        assert!(list.contains(&s1.id));
        assert!(list.contains(&s2.id));
    }

    #[test]
    fn test_event_log_append() {
        let dir = tmp_dir();
        let plan = test_plan();
        let waves = test_waves();
        let session = create_session(dir.path(), &plan, &waves).unwrap();
        start_session(dir.path(), &session.id).unwrap();
        let events = events::read_events(&event_log_path(dir.path(), &session.id)).unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "session_created");
        assert_eq!(events[1].event, "session_started");
    }

    #[test]
    fn test_session_not_found() {
        let dir = tmp_dir();
        let missing_id = uuid::Uuid::new_v4().to_string();
        let err = load_session(dir.path(), &missing_id);
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), RuntimeError::SessionNotFound(_)));
    }

    #[test]
    fn load_session_rejects_path_traversal_at_runtime_boundary() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        let escaped_path = dir
            .path()
            .join(".orquestra")
            .join("escape")
            .join("session.json");
        storage::atomic_write_json(&escaped_path, &session).expect("write escaped session");

        let error =
            load_session(dir.path(), "../escape").expect_err("path traversal must be rejected");

        assert!(error.to_string().contains("UUID v4"));
    }

    #[test]
    fn checkpoint_rejects_zero_and_out_of_range_waves() {
        for wave in [0, 3] {
            let dir = tmp_dir();
            let session = create_test_session(dir.path());
            start_session(dir.path(), &session.id).expect("start session");

            let error = checkpoint_session(dir.path(), &session.id, wave)
                .expect_err("invalid wave must be rejected");

            assert!(error.to_string().contains("wave"));
            assert_eq!(
                load_session(dir.path(), &session.id)
                    .expect("load unchanged session")
                    .status,
                SessionStatus::Running
            );
        }
    }

    #[test]
    fn checkpoint_rejects_future_wave() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");

        let error =
            checkpoint_session(dir.path(), &session.id, 2).expect_err("future wave must reject");

        assert!(error.to_string().contains("current wave"));
        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load unchanged session")
                .current_wave,
            1
        );
    }

    #[test]
    fn checkpoint_rejects_incomplete_current_wave() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");

        let error = checkpoint_session(dir.path(), &session.id, 1)
            .expect_err("incomplete current wave must reject checkpoint");

        assert!(error.to_string().contains("not complete"));
    }

    #[test]
    fn complete_session_rejects_incomplete_tickets() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");

        let error = complete_session(dir.path(), &session.id)
            .expect_err("incomplete tickets must reject session completion");

        assert!(error.to_string().contains("not complete"));
    }

    #[test]
    fn resume_requires_current_checkpoint_file() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");
        let mut ready = load_session(dir.path(), &session.id).unwrap();
        ready.ticket_states.get_mut("T1").unwrap().status = TicketStatus::Completed;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).unwrap(),
            &ready,
        )
        .expect("write ready session");
        checkpoint_session(dir.path(), &session.id, 1).expect("checkpoint session");
        std::fs::remove_file(checkpoint_path(dir.path(), &session.id, 1))
            .expect("remove checkpoint");

        resume_session(dir.path(), &session.id).expect_err("missing checkpoint must fail resume");

        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load checkpoint session")
                .status,
            SessionStatus::Checkpoint
        );
    }

    #[test]
    fn resume_rejects_corrupt_current_checkpoint() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");
        let mut ready = load_session(dir.path(), &session.id).unwrap();
        ready.ticket_states.get_mut("T1").unwrap().status = TicketStatus::Completed;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).unwrap(),
            &ready,
        )
        .expect("write ready session");
        checkpoint_session(dir.path(), &session.id, 1).expect("checkpoint session");
        std::fs::write(checkpoint_path(dir.path(), &session.id, 1), "not-json")
            .expect("corrupt checkpoint");

        resume_session(dir.path(), &session.id).expect_err("corrupt checkpoint must fail resume");

        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load checkpoint session")
                .status,
            SessionStatus::Checkpoint
        );
    }

    #[test]
    fn start_event_failure_does_not_commit_transition() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        block_event_log(dir.path(), &session.id);

        start_session(dir.path(), &session.id).expect_err("blocked event append must fail start");

        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load unchanged session")
                .status,
            SessionStatus::Created
        );
    }

    #[test]
    fn create_event_failure_does_not_publish_session() {
        let dir = tmp_dir();
        let orquestra_dir = dir.path().join(".orquestra");
        std::fs::create_dir(&orquestra_dir).expect("create Orquestra directory");
        std::fs::write(orquestra_dir.join("events"), "blocked").expect("block events directory");

        create_session(dir.path(), &test_plan(), &test_waves())
            .expect_err("blocked event append must fail create");

        assert!(
            storage::list_sessions(dir.path())
                .expect("list sessions")
                .is_empty()
        );
    }

    #[test]
    fn cancel_event_failure_does_not_commit_transition() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");
        block_event_log(dir.path(), &session.id);

        cancel_session(dir.path(), &session.id).expect_err("blocked event append must fail cancel");

        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load unchanged session")
                .status,
            SessionStatus::Running
        );
    }

    #[test]
    fn complete_event_failure_does_not_commit_transition() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");
        let mut ready = load_session(dir.path(), &session.id).unwrap();
        ready.ticket_states.get_mut("T1").unwrap().status = TicketStatus::Completed;
        ready.ticket_states.get_mut("T2").unwrap().status = TicketStatus::Completed;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).unwrap(),
            &ready,
        )
        .expect("write ready session");
        block_event_log(dir.path(), &session.id);

        complete_session(dir.path(), &session.id)
            .expect_err("blocked event append must fail completion");

        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load unchanged session")
                .status,
            SessionStatus::Running
        );
    }

    #[test]
    fn checkpoint_event_failure_persists_checkpoint_before_session_state() {
        let dir = tmp_dir();
        let session = create_test_session(dir.path());
        start_session(dir.path(), &session.id).expect("start session");
        let mut ready = load_session(dir.path(), &session.id).unwrap();
        ready.ticket_states.get_mut("T1").unwrap().status = TicketStatus::Completed;
        storage::atomic_write_json(
            &storage::session_file(dir.path(), &session.id).unwrap(),
            &ready,
        )
        .expect("write ready session");
        block_event_log(dir.path(), &session.id);

        checkpoint_session(dir.path(), &session.id, 1)
            .expect_err("blocked event append must fail checkpoint");

        assert!(checkpoint_path(dir.path(), &session.id, 1).exists());
        assert_eq!(
            load_session(dir.path(), &session.id)
                .expect("load unchanged session")
                .status,
            SessionStatus::Running
        );
    }
}
