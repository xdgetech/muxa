//! Agent states live in pane options and in the agents' own files, and tmux
//! tells no one when they change. This task looks every 2 s and sends
//! `sessions-changed` when an agent pane's role or state changed, so clients
//! list again.

use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::Message;
use muxa_tmux::{AgentStatus, Client, PaneId, Role, Session};
use tokio::sync::mpsc;

use crate::agents::Registry;
use crate::sessions;

const TICK: Duration = Duration::from_secs(2);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

/// A replacement process or fresh report must refresh clients even when its
/// state has the same name as the previous run's.
type Signature = Vec<(PaneId, u32, Role, Option<AgentStatus>)>;

pub async fn run(
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    agents: Arc<Registry>,
    records: Arc<crate::task_records::Config>,
) {
    let mut last: Option<Signature> = None;
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let Some(now) = look(&client, &agents, &records).await else {
            continue;
        };
        let changed = last.as_ref().is_some_and(|l| *l != now);
        last = Some(now);
        if changed && out.send(Message::SessionsChanged).await.is_err() {
            return;
        }
    }
}

async fn look(
    client: &Client,
    agents: &Arc<Registry>,
    records: &Arc<crate::task_records::Config>,
) -> Option<Signature> {
    let listed = tokio::time::timeout(LIST_TIMEOUT, muxa_tmux::list_sessions(client)).await;
    let mut sessions = match listed {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "cannot list sessions for agent states");
            return None;
        }
        Err(_) => return None,
    };
    let agents = Arc::clone(agents);
    let records = Arc::clone(records);
    tokio::task::spawn_blocking(move || {
        sessions::fill_agents(&mut sessions, &agents);
        crate::task_records::observation::refresh(&mut sessions, &records, &agents);
        crate::items::produce::observe(&sessions, &records, &agents);
        signature(&sessions)
    })
    .await
    .ok()
}

fn signature(sessions: &[Session]) -> Signature {
    sessions
        .iter()
        .flat_map(|s| &s.panes)
        .map(|p| (p.id, p.pid, p.role, p.agent))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_tmux::{AgentState, Pane, SessionId, WindowId};

    #[test]
    fn a_replacement_and_a_fresh_report_both_invalidate_the_signature() {
        let mut sessions = vec![Session {
            id: SessionId::new(1),
            name: "fixture".into(),
            repo: None,
            repo_dir: None,
            branch: None,
            worktree: None,
            task: None,
            task_id: None,
            panes: vec![Pane {
                id: PaneId::new(1),
                window: WindowId::new(1),
                index: 0,
                role: Role::Agent,
                cols: 80,
                rows: 24,
                command: "fixture".into(),
                path: "/fixture".into(),
                dead: false,
                pid: 100,
                shell_foreground: false,
                agent: Some(AgentStatus {
                    state: AgentState::Idle,
                    since: Some(10),
                }),
            }],
        }];
        let before = signature(&sessions);
        assert_eq!(before, signature(&sessions));
        sessions[0].panes[0].pid = 200;
        let replacement = signature(&sessions);
        assert_ne!(before, replacement);
        sessions[0].panes[0].agent.as_mut().unwrap().since = Some(20);
        assert_ne!(replacement, signature(&sessions));
    }
}
