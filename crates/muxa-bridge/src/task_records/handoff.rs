//! How work reaches the agent (ADR 0015, 0018): one fixed prompt typed once
//! the agent is idle, and the run it belongs to filled in from what the
//! adapter reports. No user text goes on a command line or into a log.

use std::sync::Arc;
use std::time::Duration;

use muxa_tmux::{AgentState, AgentStatus, Client, PaneId, Role, SessionId};

use super::{Config, blocking, link};
use crate::agents::{Processes, Registry};
use crate::records::{PendingPrompt, PromptKind, Record, Seen, StoreError};

/// How long a new agent gets to report `idle`, in seconds.
pub const PROMPT_WAIT: u64 = 30;
const PROMPT_POLL: Duration = Duration::from_secs(1);
/// Between the text and Enter, so a TUI does not take both as one paste.
const KEY_GAP: Duration = Duration::from_millis(200);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_ATTEMPTS: usize = 3;

/// A fixed prompt, and the note that replaces it when the agent never became idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prompt {
    pub kind: PromptKind,
    pub text: &'static str,
    pub not_sent: &'static str,
    /// Only an `idle` or `done` since this time counts, in epoch seconds. A
    /// state tag survives `respawn-pane`, and an old one would type into a
    /// starting agent.
    pub idle_since: Option<u64>,
}

pub const START: Prompt = Prompt {
    kind: PromptKind::Start,
    text: "Read .muxa/TASK.md and start.",
    not_sent: "The agent did not become idle within 30 seconds, so the start prompt \
               was not sent. In its pane, type: Read .muxa/TASK.md and start.",
    idle_since: None,
};

pub fn restart(at: u64) -> Prompt {
    Prompt {
        kind: PromptKind::Restart,
        text: "Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.",
        not_sent: "The restarted agent did not become idle within 30 seconds, so the \
                   prompt was not sent. In its pane, type: Read .muxa/TASK.md and \
                   .muxa/HANDOFF.md, then continue.",
        idle_since: Some(at),
    }
}

impl Prompt {
    /// The prompt a recorded wait is for. A restart's wait began at the restart.
    pub fn of(wait: PendingPrompt) -> Self {
        match wait.kind {
            PromptKind::Start => START,
            PromptKind::Restart => restart(wait.since),
        }
    }

    /// A finished turn (ADR 0019): `idle`, or a `done` no older than the
    /// wait that `began` (epoch seconds). tmux-agents keeps a finished turn
    /// `done` while no person watches the window.
    fn ready(&self, status: Option<AgentStatus>, began: u64) -> bool {
        let Some(s) = status else { return false };
        match (s.state, self.idle_since) {
            (AgentState::Idle, None) => true,
            (AgentState::Idle, Some(at)) => s.since.is_some_and(|since| since >= at),
            (AgentState::Done, at) => s.since.is_some_and(|since| since >= at.unwrap_or(began)),
            _ => false,
        }
    }
}

/// The agent pane of a Task's session, as tags and adapters describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPane {
    pub session: SessionId,
    pub pane: PaneId,
    pub status: Option<AgentStatus>,
    /// What an adapter reports about the agent there.
    pub seen: Option<Seen>,
}

/// Which session to look in.
#[derive(Debug, Clone)]
pub enum Linked {
    Session(SessionId),
    /// The session that runs this record, by tag or by `dir` (ADR 0015).
    Task(String),
}

/// A prompt to type, as the record's `pendingPrompt` holds it.
#[derive(Debug, Clone)]
pub struct Wait {
    pub task: String,
    pub which: Linked,
    pub pending: PendingPrompt,
}

/// Types the waited-for prompt into the agent pane once the agent is ready,
/// and fills in the run from what the adapter reports. Not on `waiting`:
/// that includes a permission menu, where the text would pick an option.
/// [`PROMPT_WAIT`] seconds after the wait began, the record's note says the
/// prompt was not sent. The record's `pendingPrompt` is cleared either way;
/// a wait whose `pendingPrompt` is gone was ended by another process.
pub async fn send_prompt(
    client: Arc<Client>,
    agents: Arc<Registry>,
    config: Arc<Config>,
    wait: Wait,
) {
    let Wait {
        task,
        which,
        pending,
    } = wait;
    let prompt = Prompt::of(pending);
    let left = (pending.since + PROMPT_WAIT).saturating_sub(crate::records::now());
    let deadline = tokio::time::Instant::now() + Duration::from_secs(left);
    let mut seen = None;
    loop {
        let found = agent_pane(&client, &agents, &config, which.clone()).await;
        if let Some(found) = found {
            seen = found.seen.or(seen);
            if prompt.ready(found.status, pending.since) {
                if !still_pending(&config, &task, pending).await {
                    tracing::info!(task, "the prompt wait ended elsewhere");
                    return;
                }
                match type_prompt(&client, found.pane, prompt.text).await {
                    Ok(()) => tracing::info!(task, pane = %found.pane, "prompt sent"),
                    Err(e) => {
                        tracing::warn!(task, pane = %found.pane, error = %e, "cannot send the prompt")
                    }
                }
                end_wait(&config, task, pending, seen, None).await;
                return;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(PROMPT_POLL).await;
    }
    tracing::info!(task, "agent not idle; prompt not sent");
    end_wait(&config, task, pending, seen, Some(prompt.not_sent)).await;
}

async fn still_pending(config: &Config, task: &str, pending: PendingPrompt) -> bool {
    let (config, task) = (config.clone(), task.to_owned());
    blocking(move || {
        let r = config
            .store()?
            .get(&task)
            .map_err(|e| super::store_error(&e))?;
        Ok(r.pending_prompt() == Some(pending))
    })
    .await
    .unwrap_or(false)
}

/// Clears the wait, if it is still this one, with the run's facts and a note.
pub async fn end_wait(
    config: &Config,
    task: String,
    pending: PendingPrompt,
    seen: Option<Seen>,
    note: Option<&'static str>,
) {
    write(config, task, move |r| {
        if r.pending_prompt() == Some(pending) {
            r.set_pending_prompt(None);
        }
        if let Some(note) = note {
            r.note = Some(note.to_owned());
        }
        if let (Some(seen), Some(run)) = (&seen, r.run_mut(pending.run)) {
            run.fill(seen);
        }
    })
    .await;
}

async fn type_prompt(client: &Client, pane: PaneId, text: &str) -> Result<(), muxa_tmux::Error> {
    muxa_tmux::send_text(client, pane, text).await?;
    tokio::time::sleep(KEY_GAP).await;
    muxa_tmux::send_keys(client, pane, &["Enter".to_owned()]).await
}

/// The first live agent pane of the linked session, with its state and what
/// its adapter reports.
pub async fn agent_pane(
    client: &Client,
    agents: &Arc<Registry>,
    config: &Arc<Config>,
    which: Linked,
) -> Option<AgentPane> {
    let listed = tokio::time::timeout(LIST_TIMEOUT, muxa_tmux::list_sessions(client))
        .await
        .ok()?
        .ok()?;
    let (agents, config) = (Arc::clone(agents), Arc::clone(config));
    tokio::task::spawn_blocking(move || {
        let mut sessions = listed;
        if matches!(which, Linked::Task(_)) {
            link(&mut sessions, &config);
        }
        let mut mine: Vec<_> = sessions
            .into_iter()
            .filter(|s| match &which {
                Linked::Session(id) => s.id == *id,
                Linked::Task(task) => s.task_id.as_deref() == Some(task.as_str()),
            })
            .take(1)
            .collect();
        crate::sessions::fill_agents(&mut mine, &agents);
        let session = mine.into_iter().next()?;
        let pane = session
            .panes
            .iter()
            .find(|p| p.role == Role::Agent && !p.dead)?;
        let procs = Processes::list().ok()?;
        let seen = agents
            .identify(&crate::sessions::info(&session.name, pane), &procs)
            .map(|(agent, found)| Seen {
                agent: agent.to_owned(),
                session: found.session.as_ref().map(|s| s.id.clone()),
                transcript: found.session.and_then(|s| s.transcript),
            });
        Some(AgentPane {
            session: session.id,
            pane: pane.id,
            status: pane.agent,
            seen,
        })
    })
    .await
    .ok()
    .flatten()
}

/// Applies `change` to the current record, retrying when another bridge
/// wrote in between. Returns the record written.
pub async fn write(
    config: &Config,
    task: String,
    change: impl Fn(&mut Record) + Send + 'static,
) -> Option<Record> {
    let config = config.clone();
    let done = blocking(move || {
        let store = config.store()?;
        for _ in 0..WRITE_ATTEMPTS {
            let rev = store.get(&task).map_err(|e| super::store_error(&e))?.rev;
            match store.update(&task, rev, &change) {
                Ok(r) => return Ok(r),
                Err(StoreError::Conflict { .. }) => continue,
                Err(e) => return Err(super::store_error(&e)),
            }
        }
        Err(super::store_error(&StoreError::Busy(task)))
    })
    .await;
    match done {
        Ok(r) => Some(r),
        Err(e) => {
            tracing::warn!(error = %e.message, "cannot update a task record");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: AgentState, since: Option<u64>) -> Option<AgentStatus> {
        Some(AgentStatus { state, since })
    }

    fn idle(since: Option<u64>) -> Option<AgentStatus> {
        status(AgentState::Idle, since)
    }

    fn done(since: Option<u64>) -> Option<AgentStatus> {
        status(AgentState::Done, since)
    }

    #[test]
    fn a_restart_prompt_waits_for_an_idle_newer_than_the_restart() {
        assert!(START.ready(idle(None), 50));
        assert!(!START.ready(None, 50));
        let p = restart(100);
        assert!(
            !p.ready(idle(Some(99)), 150),
            "an idle from before the restart"
        );
        assert!(!p.ready(idle(None), 150), "an idle of unknown age");
        assert!(p.ready(idle(Some(100)), 150));
        assert!(!p.ready(status(AgentState::Running, Some(200)), 150));
    }

    #[test]
    fn a_fresh_done_is_a_finished_turn() {
        assert!(START.ready(done(Some(50)), 50));
        assert!(
            !START.ready(done(Some(49)), 50),
            "a done from before the wait"
        );
        assert!(!START.ready(done(None), 50), "a done of unknown age");
        let p = restart(100);
        assert!(
            p.ready(done(Some(100)), 150),
            "since the restart, not the wait"
        );
        assert!(!p.ready(done(Some(99)), 150));
        for state in [AgentState::Waiting, AgentState::Error, AgentState::Running] {
            assert!(!START.ready(status(state, Some(60)), 50), "{state:?}");
        }
    }
}
