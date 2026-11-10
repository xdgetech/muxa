//! A Codex thread muxa created, seen by its id (ADR 0028 §5). The state is
//! the App Server's status; its age comes from the rollout at the stored path.
//! No cwd or time matching: another Codex in the same directory never binds.
use std::path::{Path, PathBuf};

use muxa_protocol::{Agent, AgentState};

use super::app_server::{Endpoint, Status, Thread, ThreadId};
use super::last_turn;
use crate::agents::{AgentSession, Found, PaneInfo};

/// What the run stores about its thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub id: ThreadId,
    pub path: Option<PathBuf>,
}

/// The agent in an owned run's pane. `process_bound` is true here only:
/// turns go to the stored id, not to whatever the pane shows.
pub fn find(endpoint: &Endpoint, stored: &Stored, pane: &PaneInfo) -> Option<Found> {
    if pane.shell_foreground {
        return None;
    }
    let read = endpoint.read_thread(&stored.id);
    if let Err(e) = &read {
        tracing::debug!(thread = %stored.id, error = %e, "codex thread unread");
    }
    let thread = read.ok();
    let path = stored
        .path
        .clone()
        .or_else(|| thread.as_ref().and_then(|t| t.path.clone()));
    let status = thread.and_then(|t| state(&t, path.as_deref()));
    Some(Found {
        process_bound: true,
        status,
        session: Some(AgentSession {
            id: stored.id.to_string(),
            transcript: path,
        }),
    })
}

/// The thread's status as an agent state. `notLoaded` and `systemError` are
/// unknown. A time is given only when the rollout's last turn event agrees.
pub fn state(thread: &Thread, rollout: Option<&Path>) -> Option<Agent> {
    let last = rollout.filter(|p| p.is_file()).and_then(last_turn);
    let since = |want: AgentState| {
        last.as_ref()
            .filter(|a| a.state == want)
            .and_then(|a| a.since)
    };
    let (state, since) = match thread.status {
        Status::Idle => (AgentState::Idle, since(AgentState::Idle)),
        Status::Active { waiting: true } => (AgentState::Waiting, None),
        Status::Active { waiting: false } => (AgentState::Running, since(AgentState::Running)),
        Status::SystemError | Status::NotLoaded => return None,
    };
    Some(Agent { state, since })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "01a1c48e-945c-7353-a880-253b964832f6";

    fn thread(status: Status) -> Thread {
        Thread {
            id: ThreadId::parse(ID).unwrap(),
            status,
            path: None,
        }
    }

    fn rollout(lines: &[&str]) -> (PathBuf, impl Drop) {
        struct Gone(PathBuf);
        impl Drop for Gone {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.0);
            }
        }
        let path = std::env::temp_dir().join(format!(
            "muxa-owned-{}-{:08x}.jsonl",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        (path.clone(), Gone(path))
    }

    const STARTED: &str =
        r#"{"type":"event_msg","payload":{"type":"task_started","started_at":100}}"#;
    const COMPLETE: &str =
        r#"{"type":"event_msg","payload":{"type":"task_complete","completed_at":120}}"#;

    #[test]
    fn the_status_is_the_state_and_the_rollout_only_dates_it() {
        let at = |state, since| Some(Agent { state, since });
        assert_eq!(
            state(&thread(Status::Idle), None),
            at(AgentState::Idle, None)
        );
        let (done, _gone) = rollout(&[STARTED, COMPLETE]);
        assert_eq!(
            state(&thread(Status::Idle), Some(&done)),
            at(AgentState::Idle, Some(120))
        );
        assert_eq!(
            state(&thread(Status::Active { waiting: false }), Some(&done)),
            at(AgentState::Running, None),
            "a rollout that disagrees gives no time"
        );
        let (running, _gone2) = rollout(&[STARTED]);
        assert_eq!(
            state(&thread(Status::Active { waiting: false }), Some(&running)),
            at(AgentState::Running, Some(100))
        );
        assert_eq!(
            state(&thread(Status::Active { waiting: true }), Some(&running)),
            at(AgentState::Waiting, None)
        );
        assert_eq!(
            state(&thread(Status::Idle), Some(&running)),
            at(AgentState::Idle, None)
        );
        assert_eq!(state(&thread(Status::NotLoaded), Some(&done)), None);
        assert_eq!(state(&thread(Status::SystemError), Some(&done)), None);
    }

    #[test]
    fn an_unreadable_thread_has_no_state_and_a_shell_no_agent() {
        let stored = Stored {
            id: ThreadId::parse(ID).unwrap(),
            path: Some("/nowhere/rollout.jsonl".into()),
        };
        let mut pane = PaneInfo {
            pane: "%1".into(),
            session: "s".into(),
            pid: 1,
            path: "/w".into(),
            shell_foreground: false,
        };
        let missing = Endpoint::new("/nowhere/codex", None);
        let found = find(&missing, &stored, &pane).unwrap();
        assert!(found.process_bound);
        assert_eq!(found.status, None);
        let session = found.session.unwrap();
        assert_eq!(session.id, ID);
        assert_eq!(session.transcript, stored.path);
        pane.shell_foreground = true;
        assert_eq!(find(&missing, &stored, &pane), None);
    }
}
