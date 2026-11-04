//! The daemon makes Inbox items while no client is connected (ADR 0033 §2).
//! It looks at the sessions on every tick while an open Task has one, and
//! otherwise only every [`PROBE`], so its tmux client can detach.
use std::sync::Arc;
use std::time::Duration;

use muxa_tmux::Client;
use tokio::time::Instant;

use crate::agents::Registry;
use crate::records::Store;
use crate::task_records::Config;

/// Without an open Task in a session, the next look waits this long.
const PROBE: Duration = Duration::from_secs(300);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub struct Watch {
    /// The last look found an open Task with a session.
    linked: bool,
    looked_at: Option<Instant>,
}

impl Watch {
    /// Whether this tick looks at the sessions. Reads the records: blocking work
    /// runs on the blocking pool.
    pub async fn due(&self, config: &Arc<Config>) -> bool {
        let probe = self.linked || self.looked_at.is_none_or(|t| t.elapsed() >= PROBE);
        probe && open_tasks(config).await
    }

    /// Lists the sessions, fills in agent states and makes or resolves items.
    /// Only reads tmux: nothing is sized or changed (invariant 2).
    pub async fn look(&mut self, client: &Client, config: &Arc<Config>, agents: &Arc<Registry>) {
        let listed = tokio::time::timeout(LIST_TIMEOUT, muxa_tmux::list_sessions(client)).await;
        let mut sessions = match listed {
            Ok(Ok(s)) => s,
            Ok(Err(e)) => {
                tracing::debug!(error = %e, "cannot list sessions for inbox items");
                return;
            }
            Err(_) => return,
        };
        let (config, agents) = (Arc::clone(config), Arc::clone(agents));
        let linked = tokio::task::spawn_blocking(move || {
            crate::sessions::fill_agents(&mut sessions, &agents);
            crate::task_records::observation::refresh(&mut sessions, &config, &agents);
            crate::items::produce::observe(&sessions, &config, &agents)
        })
        .await;
        match linked {
            Ok(linked) => {
                self.linked = linked;
                self.looked_at = Some(Instant::now());
            }
            Err(e) => tracing::warn!(error = %e, "looking for inbox items failed"),
        }
    }
}

/// Some Task is open or in review.
async fn open_tasks(config: &Arc<Config>) -> bool {
    let Some(dir) = config.state_dir.clone() else {
        return false;
    };
    let listed = tokio::task::spawn_blocking(move || {
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        Store::open(&dir)?.list()
    })
    .await;
    match listed {
        Ok(Ok(records)) => records.iter().any(crate::items::produce::watched),
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "inbox items cannot read task records");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "inbox items: the record listing stopped");
            false
        }
    }
}
