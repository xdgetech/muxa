//! Agent states live in pane options and in the agents' own files, and tmux
//! tells no one when they change. This task looks every 2 s and sends
//! `sessions-changed` when an agent pane's role or state changed, so clients
//! list again.

use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::Message;
use muxa_tmux::{AgentState, Client, PaneId, Role, Session};
use tokio::sync::mpsc;

use crate::agents::Registry;
use crate::sessions;

const TICK: Duration = Duration::from_secs(2);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

/// What clients would see change: each pane's role and agent state. The time
/// of a state is left out; it moves without the state changing.
type Signature = Vec<(PaneId, Role, Option<AgentState>)>;

pub async fn run(client: Arc<Client>, out: mpsc::Sender<Message>, agents: Arc<Registry>) {
    let mut last: Option<Signature> = None;
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let Some(now) = look(&client, &agents).await else {
            continue;
        };
        let changed = last.as_ref().is_some_and(|l| *l != now);
        last = Some(now);
        if changed && out.send(Message::SessionsChanged).await.is_err() {
            return;
        }
    }
}

async fn look(client: &Client, agents: &Arc<Registry>) -> Option<Signature> {
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
    tokio::task::spawn_blocking(move || {
        sessions::fill_agents(&mut sessions, &agents);
        signature(&sessions)
    })
    .await
    .ok()
}

fn signature(sessions: &[Session]) -> Signature {
    sessions
        .iter()
        .flat_map(|s| &s.panes)
        .map(|p| (p.id, p.role, p.agent.map(|a| a.state)))
        .collect()
}
