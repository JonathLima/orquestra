use crate::cli::SessionAction;
use crate::output::{OutputData, print_output};
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use orquestra_runtime::{
    self as runtime, Session, SessionEvent, events::read_events, load_session,
    storage::list_sessions,
};
use serde::Serialize;

fn project_dir() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

fn display_timestamp(timestamp: &str) -> String {
    timestamp.chars().take(19).collect()
}

#[derive(Debug, Serialize)]
struct SessionListOutput {
    sessions: Vec<SessionSummary>,
}

#[derive(Debug, Serialize)]
struct SessionSummary {
    id: String,
    plan_title: String,
    status: String,
    current_wave: u32,
    total_waves: u32,
    created_at: String,
}

impl OutputData for SessionListOutput {
    fn render_human(&self) -> String {
        if self.sessions.is_empty() {
            return "No sessions found.".to_string();
        }

        let mut out = format!("Sessions ({}):\n\n", self.sessions.len());
        for session in &self.sessions {
            out.push_str(&format!(
                "  {}  {}  {}  Wave {}/{}\n",
                session.id.chars().take(8).collect::<String>(),
                session.status,
                session.plan_title,
                session.current_wave,
                session.total_waves
            ));
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct SessionShowOutput {
    session: Session,
    recent_events: Vec<SessionEvent>,
}

impl OutputData for SessionShowOutput {
    fn render_human(&self) -> String {
        let session = &self.session;
        let mut out = format!(
            "Session: {}\nPlan: {}\nStatus: {:?}\nCurrent Wave: {}/{}\n\nTickets:\n",
            session.id,
            session.plan_title,
            session.status,
            session.current_wave,
            session.total_waves
        );
        let mut ticket_states = session.ticket_states.values().collect::<Vec<_>>();
        ticket_states.sort_by(|left, right| left.id.cmp(&right.id));
        for ticket_state in ticket_states {
            out.push_str(&format!(
                "  {} [{:?}] wave={}\n",
                ticket_state.id, ticket_state.status, ticket_state.wave
            ));
        }
        if !self.recent_events.is_empty() {
            out.push_str("\nRecent Events:\n");
            for event in &self.recent_events {
                out.push_str(&format!(
                    "  {}  {}\n",
                    display_timestamp(&event.ts),
                    event.event
                ));
            }
        }
        out
    }
}

#[derive(Debug, Serialize)]
struct EventsOutput {
    events: Vec<SessionEvent>,
}

impl OutputData for EventsOutput {
    fn render_human(&self) -> String {
        if self.events.is_empty() {
            return "No events.".to_string();
        }

        let mut out = String::new();
        for event in &self.events {
            out.push_str(&format!(
                "{}  {}\n",
                display_timestamp(&event.ts),
                event.event
            ));
        }
        out
    }
}

pub fn run(action: &SessionAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        SessionAction::List => run_list(output),
        SessionAction::Show { session_id } => {
            crate::run::validate_session_id(session_id)?;
            run_show(session_id, output)
        }
        SessionAction::Resume { session_id } => {
            crate::run::validate_session_id(session_id)?;
            run_resume(session_id, output)
        }
        SessionAction::Events { session_id, tail } => {
            crate::run::validate_session_id(session_id)?;
            run_events(session_id, *tail, output)
        }
        SessionAction::Export {
            session_id,
            format,
            output_file,
        } => {
            crate::run::validate_session_id(session_id)?;
            run_export(session_id, format, output_file.clone(), output)
        }
    }
}

fn run_list(output: &OutputFormat) -> Result<(), OrquestraError> {
    let ids = list_sessions(&project_dir())
        .map_err(|error| OrquestraError::from(format!("Cannot list sessions: {error}")))?;
    let mut sessions = Vec::new();
    for id in &ids {
        let session = load_session(&project_dir(), id)
            .map_err(|error| OrquestraError::from(format!("Cannot load session {id}: {error}")))?;
        sessions.push(SessionSummary {
            id: session.id.clone(),
            plan_title: session.plan_title,
            status: format!("{:?}", session.status),
            current_wave: session.current_wave,
            total_waves: session.total_waves,
            created_at: session.created_at,
        });
    }
    print_output(&SessionListOutput { sessions }, output);
    Ok(())
}

fn run_show(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let session = load_session(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot load session: {error}")))?;
    let events_path = runtime::storage::event_log_file(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot locate events: {error}")))?;
    let events = read_events(&events_path)
        .map_err(|error| OrquestraError::from(format!("Cannot read events: {error}")))?;
    let recent_events = events.into_iter().rev().take(5).rev().collect();
    print_output(
        &SessionShowOutput {
            session,
            recent_events,
        },
        output,
    );
    Ok(())
}

fn run_resume(session_id: &str, output: &OutputFormat) -> Result<(), OrquestraError> {
    let session = runtime::resume_session(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot resume session: {error}")))?;
    print_output(&crate::run::SessionOutput { session }, output);
    Ok(())
}

fn run_events(
    session_id: &str,
    tail: Option<usize>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    let events_path = runtime::storage::event_log_file(&project_dir(), session_id)
        .map_err(|error| OrquestraError::from(format!("Cannot locate events: {error}")))?;
    let mut events = read_events(&events_path)
        .map_err(|error| OrquestraError::from(format!("Cannot read events: {error}")))?;
    if let Some(count) = tail {
        events = events.into_iter().rev().take(count).rev().collect();
    }
    print_output(&EventsOutput { events }, output);
    Ok(())
}

fn run_export(
    session_id: &str,
    format: &str,
    output_file: Option<String>,
    output: &OutputFormat,
) -> Result<(), OrquestraError> {
    crate::run::run(
        &crate::cli::RunAction::Export {
            session_id: session_id.to_string(),
            format: format.to_string(),
            output_file,
        },
        output,
    )
}
