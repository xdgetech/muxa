//! `restart-agent` (ADR 0018): a fresh agent in the same pane, after the
//! bridge wrote `TASK.md`'s header and `HANDOFF.md` from facts.

use std::path::Path;
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode};
use muxa_tmux::AgentState;

use super::create::Env;
use super::handoff::{self, AgentPane, Linked};
use super::{blocking, brief, facts, store_error};
use crate::convert::error_body;
use crate::records::{AgentSettings, Record, RunReason, Seen, now};

/// The record after the restart, or why there was none.
pub async fn restart(env: &Env, task: String, force: bool) -> Result<Record, ErrorBody> {
    let config = Arc::clone(&env.records);
    let id = task.clone();
    let record = blocking(move || config.store()?.get(&id).map_err(|e| store_error(&e))).await?;
    let dir = record.dir.clone().ok_or_else(|| {
        failed(format!(
            "task {} has no directory yet; it never started",
            record.id
        ))
    })?;
    let found = handoff::agent_pane(
        &env.client,
        &env.agents,
        &env.records,
        Linked::Task(record.id.clone()),
    )
    .await
    .ok_or_else(|| {
        failed(format!(
            "no session runs task {} with an agent pane. Start its session again.",
            record.id
        ))
    })?;
    if !force && found.status.is_some_and(|s| s.state == AgentState::Running) {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!(
                "the agent of {} is running. Wait until it stops, or restart it with force.",
                record.id
            ),
        ));
    }
    let agent = workspace_agent(env, &record.workspace).await?;
    let now = now();
    write_files(&record, &dir, found.seen.clone(), now).await?;
    respawn(env, &found, &dir, agent.as_ref()).await?;
    let seen = found.seen.clone();
    let saved = handoff::write(&env.records, record.id.clone(), move |r| {
        advance(r, seen.as_ref(), now);
    })
    .await
    .ok_or_else(|| {
        error_body(
            ErrorCode::Internal,
            "the agent restarted, but the task record could not be saved",
        )
    })?;
    tracing::info!(task = %saved.id, pane = %found.pane, force, "agent restarted");
    let run = saved.runs.last().map_or(0, |r| r.id);
    env.pending.spawn(handoff::send_prompt(
        Arc::clone(&env.client),
        Arc::clone(&env.agents),
        Arc::clone(&env.records),
        saved.id.clone(),
        found.session,
        run,
        handoff::restart(now),
    ));
    Ok(saved)
}

/// Ends the open run with what the adapter saw, and adds the restart's run.
pub(super) fn advance(r: &mut Record, seen: Option<&Seen>, now: u64) -> u32 {
    if let (Some(seen), Some(id)) = (seen, r.open_run().map(|run| run.id)) {
        if let Some(run) = r.run_mut(id) {
            run.fill(seen);
        }
    }
    r.start_run(RunReason::Restart, now)
}

/// The Task workspace's agent settings, read on every request (ADR 0016).
async fn workspace_agent(env: &Env, name: &str) -> Result<Option<AgentSettings>, ErrorBody> {
    let config = Arc::clone(&env.records);
    let name = name.to_owned();
    blocking(move || {
        let workspaces = config.load_workspaces()?;
        Ok(workspaces.get(&name).and_then(|w| w.agent.clone()))
    })
    .await
}

/// `TASK.md`'s header and `HANDOFF.md`, from the record as the restart will
/// leave it.
async fn write_files(
    record: &Record,
    dir: &Path,
    seen: Option<Seen>,
    now: u64,
) -> Result<(), ErrorBody> {
    let (record, dir) = (record.clone(), dir.to_path_buf());
    blocking(move || {
        let mut next = record.clone();
        let had_open = next.open_run().is_some();
        advance(&mut next, seen.as_ref(), now);
        let earlier = &next.runs[..next.runs.len().saturating_sub(1)];
        let since = record
            .open_run()
            .map_or(record.created_at, |r| r.started_at);
        let work = facts::work(&record, &dir, since);
        let replaced = seen.as_ref().filter(|_| !had_open);
        let text = facts::render(&record, earlier, replaced, &work, now);
        brief::refresh(&dir, &record).map_err(|m| error_body(ErrorCode::Internal, m))?;
        brief::write_file(&dir, facts::FILE, &text)
            .map_err(|m| error_body(ErrorCode::Internal, m))?;
        Ok(())
    })
    .await
}

/// `respawn-pane -k` with the workspace's env. A pane that started a shell
/// (tmux-agents types the agent into it) gets the agent command typed again.
async fn respawn(
    env: &Env,
    found: &AgentPane,
    dir: &std::path::Path,
    agent: Option<&AgentSettings>,
) -> Result<(), ErrorBody> {
    let client = &*env.client;
    let tmux = |e: muxa_tmux::Error| failed(format!("cannot restart the agent: {e}"));
    let start = muxa_tmux::pane_start_command(client, found.pane)
        .await
        .map_err(tmux)?;
    let vars = agent.map(|a| a.env.as_slice()).unwrap_or_default();
    if let Some(a) = agent {
        // Keys only: values may be tokens.
        tracing::info!(keys = ?a.keys(), "agent environment");
    }
    muxa_tmux::respawn_pane(client, found.pane, &dir.to_string_lossy(), vars)
        .await
        .map_err(tmux)?;
    if start.trim().is_empty() {
        let command = agent
            .and_then(|a| a.command.as_deref())
            .unwrap_or(&env.tasks.command);
        muxa_tmux::send_text(client, found.pane, command)
            .await
            .map_err(tmux)?;
        muxa_tmux::send_keys(client, found.pane, &["Enter".to_owned()])
            .await
            .map_err(tmux)?;
    }
    Ok(())
}

fn failed(message: String) -> ErrorBody {
    error_body(ErrorCode::TaskFailed, message)
}
