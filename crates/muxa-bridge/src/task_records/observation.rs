//! Managed restart states must come from the replacement's adapter, not pane tags.
use muxa_tmux::{Pane, PaneId, Role, Session};

use crate::agents::{Found, Processes, Registry};
use crate::convert;
use crate::records::{Record, Seen};

use super::Config;

/// Blocking, like session enrichment. Manual and legacy runs keep their tag behavior.
pub(crate) fn refresh(sessions: &mut [Session], config: &Config, agents: &Registry) {
    if !config.enabled() {
        return;
    }
    let records = config.records_if_any();
    super::link_records(sessions, &records);
    let mut processes = None;
    for session in sessions {
        let Some(record) = records
            .iter()
            .find(|r| Some(r.id.as_str()) == session.task_id.as_deref())
        else {
            if session.task_id.is_some() {
                for pane in session.panes.iter_mut().filter(|p| p.role == Role::Agent) {
                    pane.agent = None;
                }
            }
            continue;
        };
        if !guarded(record) {
            continue;
        }
        let procs = processes.get_or_insert_with(|| Processes::list().ok());
        apply(session, record, agents, procs.as_ref());
    }
}

fn guarded(record: &Record) -> bool {
    record.worker().is_some()
        || record.extra.contains_key(crate::records::lifecycle::FIELD)
        || record.open_run().is_some_and(|run| {
            run.extra.contains_key(crate::records::observation::FIELD)
                || run
                    .extra
                    .get("outcome")
                    .is_some_and(|value| value == "unconfirmed")
        })
}

fn uncertain(record: &Record) -> bool {
    record.extra.contains_key(crate::records::lifecycle::FIELD)
        || record.open_run().is_some_and(|run| {
            run.extra
                .get("outcome")
                .is_some_and(|value| value == "unconfirmed")
        })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChatBinding {
    operation: String,
    pub seen: Seen,
}

#[derive(Debug, thiserror::Error)]
#[error("replacement agent observation is unconfirmed")]
pub(crate) struct Unconfirmed;

/// Chat uses the same identity proof as states, independently of whether a state exists.
pub(crate) fn chat_binding(
    sessions: &mut [Session],
    pane: PaneId,
    config: &Config,
    agents: &Registry,
) -> Result<Option<ChatBinding>, Unconfirmed> {
    let records = config.records_if_any();
    super::link_records(sessions, &records);
    let Some(session) = sessions
        .iter()
        .find(|session| session.panes.iter().any(|p| p.id == pane))
    else {
        return Ok(None);
    };
    let Some(task) = &session.task_id else {
        return Ok(None);
    };
    let Some(record) = records.iter().find(|record| record.id == *task) else {
        return Err(Unconfirmed);
    };
    if uncertain(record) {
        return Err(Unconfirmed);
    }
    if !guarded(record) {
        return Ok(None);
    }
    let candidate = session
        .panes
        .iter()
        .find(|candidate| candidate.id == pane)
        .ok_or(Unconfirmed)?;
    let procs = Processes::list().map_err(|_| Unconfirmed)?;
    let (agent, found) = accepted(session, candidate, record, agents, &procs).ok_or(Unconfirmed)?;
    let baseline = record
        .open_run()
        .ok_or(Unconfirmed)?
        .restart_observation()
        .map_err(|_| Unconfirmed)?
        .ok_or(Unconfirmed)?;
    Ok(Some(ChatBinding {
        operation: baseline.operation,
        seen: Seen {
            agent: agent.into(),
            session: found.session.as_ref().map(|session| session.id.clone()),
            transcript: found.session.and_then(|session| session.transcript),
        },
    }))
}

fn accepted(
    session: &Session,
    pane: &Pane,
    record: &Record,
    agents: &Registry,
    procs: &Processes,
) -> Option<(&'static str, Found)> {
    if uncertain(record) || (record.worker().is_some() && !record.worker_run_bound()) {
        return None;
    }
    let baseline = record.open_run()?.restart_observation().ok()??;
    if !super::restart::process_identity(pane.pid)
        .is_ok_and(|process| process == baseline.replacement)
    {
        return None;
    }
    let info = crate::sessions::info(&session.name, pane);
    let (agent, found) = agents.identify(&info, procs)?;
    let seen = Seen {
        agent: agent.into(),
        session: found.session.as_ref().map(|session| session.id.clone()),
        transcript: found
            .session
            .as_ref()
            .and_then(|session| session.transcript.clone()),
    };
    baseline
        .accepts(
            session.id.number(),
            pane.id.number(),
            pane.pid,
            pane.shell_foreground,
            found.process_bound,
            Some(&seen),
        )
        .then_some((agent, found))
}

fn apply(session: &mut Session, record: &Record, agents: &Registry, procs: Option<&Processes>) {
    let statuses: Vec<_> = session
        .panes
        .iter()
        .map(|pane| {
            if pane.role != Role::Agent {
                return pane.agent;
            }
            procs
                .and_then(|procs| accepted(session, pane, record, agents, procs))
                .and_then(|(_, found)| found.status.as_ref().and_then(convert::agent_status))
        })
        .collect();
    for (pane, status) in session.panes.iter_mut().zip(statuses) {
        pane.agent = status;
    }
}
