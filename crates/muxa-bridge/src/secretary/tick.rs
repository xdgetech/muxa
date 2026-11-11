//! The daemon's part (ADR 0056 §2): every [`CHECK`] it looks at the
//! secretary's session. A gone session is started again through
//! `ask::start_root`, a dead agent pane is respawned in place, and a running
//! agent's Claude Code session id is kept for the next start.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use muxa_tmux::{Client, Pane, Role, Session};

use super::keeper::{Act, Keeper, Limits, Seen};
use super::{base_command, file, prepare};
use crate::agents::{Processes, Registry};
use crate::items::store::ItemStore;
use crate::records::workspaces::Workspace;
use crate::task_records::Context;

const CHECK: Duration = Duration::from_secs(10);
/// After tmux was not reachable: fewer tries, fewer log lines.
const NO_TMUX: Duration = Duration::from_secs(60);
/// Between two reads of the agent's session id: they run `ps`.
const OBSERVE: Duration = Duration::from_secs(30);
/// A resumed start that ends sooner did not find its conversation.
const QUICK_EXIT: u64 = 30;
/// The give-up item's file. Not a record id: those have 6 hex digits.
const ITEMS_FILE: &str = "t-0";
const ITEM_TITLE: &str = "Assistant chat";

/// The daemon's memory of the secretary between ticks.
pub struct Resident {
    keeper: Keeper,
    limits: Limits,
    every: Duration,
    checked: Option<Instant>,
    observed: Option<Instant>,
    /// No look before this: tmux was not reachable.
    quiet_until: Option<Instant>,
}

impl Default for Resident {
    fn default() -> Self {
        let (limits, every) = timing();
        Self {
            keeper: Keeper::default(),
            limits,
            every,
            checked: None,
            observed: None,
            quiet_until: None,
        }
    }
}

/// `MUXA_TEST_SECRETARY_FAST` makes a debug build check every second and
/// restart after 1 s, for the integration test. Release builds ignore it.
fn timing() -> (Limits, Duration) {
    if cfg!(debug_assertions) && std::env::var_os("MUXA_TEST_SECRETARY_FAST").is_some() {
        let fast = Limits {
            backoff: [1, 1, 1],
            ..Limits::DEFAULT
        };
        return (fast, Duration::from_secs(1));
    }
    (Limits::DEFAULT, CHECK)
}

pub async fn tick(cx: &Context, r: &mut Resident) {
    let now_at = Instant::now();
    if r.checked.is_some_and(|at| at.elapsed() < r.every)
        || r.quiet_until.is_some_and(|until| now_at < until)
    {
        return;
    }
    r.checked = Some(Instant::now());
    let Some(state_dir) = cx.records.state_dir.clone() else {
        return;
    };
    let config = Arc::clone(&cx.records);
    let loaded = tokio::task::spawn_blocking(move || config.load_workspaces()).await;
    let workspaces = match loaded {
        Ok(Ok(w)) => w,
        Ok(Err(e)) => {
            tracing::debug!(error = %e.message, "no workspaces for the secretary");
            return;
        }
        Err(e) => {
            tracing::warn!(error = %e, "reading the workspaces stopped");
            return;
        }
    };
    let Some(ws) = workspaces.secretary().cloned() else {
        return;
    };
    // Without a tmux server there is nothing to keep open; the daemon never starts one.
    let env = match cx.env().await {
        Ok(env) => env,
        Err(e) => {
            tracing::debug!(error = %e.message, "no tmux for the secretary");
            r.quiet_until = Some(now_at + NO_TMUX);
            return;
        }
    };
    let sessions = match muxa_tmux::list_sessions(&env.client).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot list sessions for the secretary");
            return;
        }
    };
    let session = crate::tasks::ask::root_session(&sessions, &ws.name).cloned();
    let agent = session.as_ref().and_then(agent_pane).cloned();
    // A session whose agent pane the user closed is theirs to keep as it is.
    let seen = match (&session, &agent) {
        (Some(_), Some(p)) if p.dead => Seen::Stopped,
        (Some(_), _) => Seen::Running,
        (None, _) => Seen::Stopped,
    };
    let now = crate::records::now();
    if seen == Seen::Running {
        r.keeper.decide(now, seen, &r.limits);
        if let Some(s) = session.filter(|_| r.observed.is_none_or(|at| at.elapsed() >= OBSERVE)) {
            r.observed = Some(Instant::now());
            observe(&state_dir, s, Arc::clone(&cx.agents)).await;
        }
        return;
    }
    note_failed_resume(&state_dir, now).await;
    match r.keeper.decide(now, seen, &r.limits) {
        Act::Nothing => {}
        Act::Start => match agent.filter(|p| p.dead) {
            Some(pane) => respawn(&env.client, &ws, &cx.tasks.command, &state_dir, &pane).await,
            None => {
                let started = crate::tasks::ask::start_root(
                    &env.client,
                    &cx.tasks.command,
                    &workspaces,
                    &ws.name,
                    Some(&state_dir),
                )
                .await;
                match started {
                    Ok(l) => tracing::info!(session = %l.id, "secretary session opened"),
                    Err(e) => tracing::warn!(error = %e, "cannot open the secretary's session"),
                }
            }
        },
        Act::GiveUp => give_up(&state_dir, &ws, &r.limits, now).await,
    }
}

/// The pane muxa started the agent in.
fn agent_pane(s: &Session) -> Option<&Pane> {
    s.panes.iter().find(|p| p.role == Role::Agent)
}

/// In place: the pane keeps its id, size and layout (`respawn-pane -k`).
async fn respawn(client: &Client, ws: &Workspace, default: &str, state_dir: &Path, pane: &Pane) {
    let base = base_command(ws, default).to_owned();
    let dir = state_dir.to_path_buf();
    let command =
        tokio::task::spawn_blocking(move || prepare(Some(&dir), &base, crate::records::now()))
            .await
            .unwrap_or_else(|_| base_command(ws, default).to_owned());
    let env = ws
        .agent
        .as_ref()
        .map(|a| a.env.as_slice())
        .unwrap_or_default();
    let root = ws.root.to_string_lossy();
    match muxa_tmux::respawn_pane_command(client, pane.id, &root, env, &[command]).await {
        Ok(()) => tracing::info!(pane = %pane.id, "secretary agent respawned"),
        Err(e) => {
            tracing::warn!(pane = %pane.id, error = %e, "cannot respawn the secretary's agent")
        }
    }
}

/// A resumed start that ended at once: the next start is fresh.
async fn note_failed_resume(state_dir: &Path, now: u64) {
    let dir = state_dir.to_path_buf();
    let done = tokio::task::spawn_blocking(move || {
        let state = file::read(&dir)?;
        if !state.resume_failed(now, QUICK_EXIT) {
            return Ok(false);
        }
        file::update(&dir, |s| s.drop_resume(now)).map(|_| true)
    })
    .await;
    match done {
        Ok(Ok(true)) => {
            tracing::warn!("the secretary's resumed agent exited at once; it starts fresh")
        }
        Ok(Ok(false)) => {}
        Ok(Err(e)) => tracing::warn!(error = %e, "cannot read the secretary's state"),
        Err(e) => tracing::warn!(error = %e, "the secretary's state check stopped"),
    }
}

/// Keeps the Claude Code session id that runs in the agent pane: only one
/// the adapter ties to the pane's process.
async fn observe(state_dir: &Path, session: Session, agents: Arc<Registry>) {
    let dir: PathBuf = state_dir.to_path_buf();
    let done = tokio::task::spawn_blocking(move || {
        let mut sessions = vec![session];
        crate::sessions::fill_agents(&mut sessions, &agents);
        let s = &sessions[0];
        let pane = agent_pane(s).filter(|p| !p.dead)?;
        let procs = Processes::list().ok()?;
        let (agent, found) = agents.identify(&crate::sessions::info(&s.name, pane), &procs)?;
        let id = found
            .session
            .filter(|_| agent == crate::agents::claude::ID && found.process_bound)?
            .id;
        if !super::command::valid_id(&id) {
            return None;
        }
        Some(file::update(&dir, |st| {
            if st.agent_session.as_deref() != Some(id.as_str()) {
                st.agent_session = Some(id.clone());
            }
        }))
    })
    .await;
    match done {
        Ok(Some(Err(e))) => tracing::warn!(error = %e, "cannot keep the secretary's session id"),
        Err(e) => tracing::warn!(error = %e, "reading the secretary's session id stopped"),
        _ => {}
    }
}

/// One `failure` item per give-up; the daemon starts the agent again only
/// after it runs again.
async fn give_up(state_dir: &Path, ws: &Workspace, limits: &Limits, now: u64) {
    let reason = format!(
        "The assistant's chat exited {} times in {} minutes, so muxa stopped restarting it. \
         Its session shows why. Fix the agent command, then close that session and open \
         the chat again, or run muxa-bridge daemon restart.",
        limits.restarts + 1,
        limits.window / 60
    );
    let (dir, workspace) = (state_dir.to_path_buf(), ws.name.clone());
    let made = tokio::task::spawn_blocking(move || {
        let store = ItemStore::open(&dir)?;
        let key = format!("failure:secretary:{now}");
        crate::assistant::failures::make_one(
            &store, ITEMS_FILE, &key, &workspace, ITEM_TITLE, &reason, now,
        )
    })
    .await;
    match made {
        Ok(Ok(item)) => {
            tracing::warn!(item = ?item, "the secretary keeps exiting; muxa stopped restarting it")
        }
        Ok(Err(e)) => tracing::warn!(error = %e, "cannot make the secretary's failure item"),
        Err(e) => tracing::warn!(error = %e, "the secretary's failure item stopped"),
    }
}
