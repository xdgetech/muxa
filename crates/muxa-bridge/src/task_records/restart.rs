//! Restart in one Task claim, with durable intent before dispatch (ADR 0024).
use std::path::Path;
use std::sync::Arc;

use muxa_protocol::{ErrorBody, ErrorCode, RestartRecoveryAction};
use muxa_tmux::{AgentState, PaneId, SessionId};

use super::create::Env;
use super::handoff::{self, AgentPane, Linked};
use super::{blocking, brief, facts, store_error};
use crate::convert::error_body;
use crate::records::lifecycle::{FIELD, Operation, Phase, ProcessIdentity};
use crate::records::store::ClaimedTask;
use crate::records::{
    AgentSettings, PendingPrompt, PromptKind, Record, RunReason, Seen, now, restart_files,
};

pub async fn restart(env: &Env, task: String, force: bool) -> Result<Record, ErrorBody> {
    let config = Arc::clone(&env.records);
    let id = task.clone();
    let mut claim = blocking(move || {
        let store = config.store()?;
        let record = store.get(&id).map_err(|e| store_error(&e))?;
        store.claim(&id, record.rev).map_err(|e| store_error(&e))
    })
    .await?;
    let record = claim.record().clone();
    if record.extra.contains_key(FIELD) {
        return Err(conflict("task restart requires recovery"));
    }
    if record.worker().is_some() {
        return Err(error_body(
            ErrorCode::Unsupported,
            "workers are not implemented on this host",
        ));
    }
    let dir = record
        .dir
        .clone()
        .ok_or_else(|| failed("task never started; no directory"))?;
    let found = handoff::agent_pane(
        &env.client,
        &env.agents,
        &env.records,
        Linked::Task(record.id.clone()),
    )
    .await
    .ok_or_else(|| failed("no session runs this task with an agent pane"))?;
    check_running(&found, force)?;
    let source = identity(&env.client, found.session, found.pane).await?;
    let agent = workspace_agent(env, &record.workspace).await?;
    let command = agent
        .as_ref()
        .and_then(|a| a.command.clone())
        .unwrap_or_else(|| env.tasks.command.clone());
    let vars = agent.as_ref().map(|a| a.env.clone()).unwrap_or_default();
    let operation = format!(
        "restart-{:016x}",
        getrandom::u64().map_err(|_| failed("cannot allocate restart id"))?
    );
    let since = now();
    let op_id = operation.clone();
    let seen = found.seen.clone();
    let session = found.session.number();
    let pane = found.pane.number();
    let (next_claim, mut op) = blocking(move || {
        let files = prepare_files(&record, &dir, &op_id, seen.clone(), since)?;
        let reserved_run = record
            .runs
            .last()
            .map_or(Some(1), |r| r.id.checked_add(1))
            .ok_or_else(|| failed("task run ids exhausted"))?;
        let op = Operation {
            version: 1,
            id: op_id,
            phase: Phase::Prepared,
            source_run: record.open_run().map(|r| r.id),
            reserved_run,
            pane,
            source_pid: source.pid,
            worker: None,
            session: Some(session),
            source_started: Some(source.started),
            replacement: None,
            files: Some(files),
            since,
            seen,
        };
        claim.prepare_restart(op.clone()).map_err(lifecycle_error)?;
        Ok((claim, op))
    })
    .await?;
    claim = next_claim;
    // This is a cooperative muxa boundary, not atomic with manual tmux commands.
    let current = identity(&env.client, found.session, found.pane).await;
    let unchanged = current
        .as_ref()
        .is_ok_and(|p| p.pid == op.source_pid && Some(&p.started) == op.source_started.as_ref());
    let current_found = handoff::agent_pane(
        &env.client,
        &env.agents,
        &env.records,
        Linked::Task(task.clone()),
    )
    .await;
    let ready = current_found
        .as_ref()
        .is_some_and(|p| p.pane == found.pane && check_running(p, force).is_ok());
    let verify_dir = claim
        .record()
        .dir
        .clone()
        .ok_or_else(|| failed("task directory is missing"))?;
    let files = op
        .files
        .clone()
        .ok_or_else(|| failed("restart stage is missing"))?;
    let verified =
        blocking(move || restart_files::verify(&verify_dir, &files).map_err(file_error)).await;
    if !unchanged || !ready || verified.is_err() {
        cancel_prepared(claim, &op).await?;
        return Err(conflict(
            "task process or managed files changed before restart",
        ));
    }
    let id = op.id.clone();
    claim = blocking(move || {
        claim
            .advance_restart(&id, Phase::Dispatching)
            .map_err(lifecycle_error)?;
        Ok(claim)
    })
    .await?;
    let dir = claim
        .record()
        .dir
        .clone()
        .ok_or_else(|| failed("task directory is missing"))?;
    if muxa_tmux::respawn_pane_command(
        &env.client,
        found.pane,
        &dir.to_string_lossy(),
        &vars,
        &[command],
    )
    .await
    .is_err()
    {
        return Err(uncertain(&operation));
    }
    let replacement = identity(&env.client, found.session, found.pane)
        .await
        .map_err(|_| uncertain(&operation))?;
    if replacement.pid == op.source_pid {
        return Err(uncertain(&operation));
    }
    op.phase = Phase::Replaced;
    op.replacement = Some(replacement);
    let saved = blocking(move || {
        claim
            .write_operation(Some(op.clone()))
            .map_err(lifecycle_error)?;
        restart_files::reconcile(
            &dir,
            op.files
                .as_ref()
                .ok_or_else(|| failed("restart stage missing"))?,
        )
        .map_err(file_error)?;
        finalize(&mut claim, &op, true, None)?;
        Ok(claim.record().clone())
    })
    .await?;
    if let Some(pending) = saved.pending_prompt() {
        env.pending.spawn(handoff::send_prompt(
            Arc::clone(&env.client),
            Arc::clone(&env.agents),
            Arc::clone(&env.records),
            handoff::Wait {
                task: saved.id.clone(),
                which: Linked::Session(found.session),
                pending,
            },
        ));
    }
    Ok(saved)
}

fn check_running(found: &AgentPane, force: bool) -> Result<(), ErrorBody> {
    if !force && found.status.is_some_and(|s| s.state == AgentState::Running) {
        Err(error_body(
            ErrorCode::BadRequest,
            "the agent is running; wait or restart with force",
        ))
    } else {
        Ok(())
    }
}

pub(crate) async fn identity(
    client: &muxa_tmux::Client,
    session: SessionId,
    pane: PaneId,
) -> Result<ProcessIdentity, ErrorBody> {
    let sessions = muxa_tmux::list_sessions(client)
        .await
        .map_err(|_| conflict("cannot verify task process"))?;
    let pid = sessions
        .iter()
        .find(|s| s.id == session)
        .and_then(|s| s.panes.iter().find(|p| p.id == pane && !p.dead))
        .map(|p| p.pid)
        .filter(|p| *p != 0)
        .ok_or_else(|| conflict("task process changed"))?;
    blocking(move || {
        let out = std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "lstart="])
            .env("LC_ALL", "C")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|_| conflict("cannot verify process start identity"))?;
        let started = String::from_utf8(out.stdout)
            .map_err(|_| conflict("cannot verify process start identity"))?;
        let started = started.trim().to_owned();
        if !out.status.success() || started.is_empty() {
            return Err(conflict("task process exited"));
        }
        Ok(ProcessIdentity { pid, started })
    })
    .await
}

fn prepare_files(
    record: &Record,
    dir: &Path,
    operation: &str,
    seen: Option<Seen>,
    at: u64,
) -> Result<restart_files::Manifest, ErrorBody> {
    let old_task = restart_files::read_current(dir, "TASK.md").map_err(file_error)?;
    let old_handoff = restart_files::read_current(dir, facts::FILE).map_err(file_error)?;
    let existing = old_task
        .as_deref()
        .map(std::str::from_utf8)
        .transpose()
        .map_err(|_| failed("TASK.md is not UTF-8"))?;
    let mut next = record.clone();
    let had_open = next.open_run().is_some();
    advance(&mut next, seen.as_ref(), at);
    let earlier = &next.runs[..next.runs.len().saturating_sub(1)];
    let since = record
        .open_run()
        .map_or(record.created_at, |r| r.started_at);
    let work = facts::work(record, dir, since);
    let text = facts::render(
        record,
        earlier,
        seen.as_ref().filter(|_| !had_open),
        &work,
        at,
    );
    let task = brief::merge(record, existing);
    restart_files::stage_from(
        dir,
        operation,
        old_task.as_deref(),
        old_handoff.as_deref(),
        task.as_bytes(),
        text.as_bytes(),
    )
    .map_err(file_error)
}

async fn cancel_prepared(mut claim: ClaimedTask, op: &Operation) -> Result<Record, ErrorBody> {
    let op = op.clone();
    blocking(move || {
        // Clear only pre-dispatch intent. Cleanup is best-effort after the durable
        // resolution; edited live notes must not trap a cancelled operation.
        claim
            .cancel_prepared_restart(&op.id)
            .map_err(lifecycle_error)?;
        cleanup_cancelled(claim.record(), &op);
        Ok(claim.record().clone())
    })
    .await
}

fn cleanup_cancelled(record: &Record, op: &Operation) {
    if let (Some(dir), Some(files)) = (&record.dir, &op.files) {
        if restart_files::cleanup_prepared(dir, files).is_err() {
            tracing::debug!(operation = %op.id, "cancelled restart stage retained");
        }
    }
}

fn finalize(
    claim: &mut ClaimedTask,
    op: &Operation,
    prompt: bool,
    recovery: Option<(u64, RestartRecoveryAction)>,
) -> Result<(), ErrorBody> {
    if claim.record().open_run().map(|r| r.id) != op.source_run
        || claim
            .record()
            .runs
            .last()
            .map_or(Some(1), |r| r.id.checked_add(1))
            != Some(op.reserved_run)
    {
        return Err(conflict("task run changed during restart"));
    }
    let final_rev = claim.record().rev + 1;
    let op = op.clone();
    claim
        .update(move |r| {
            let run = advance(r, op.seen.as_ref(), op.since);
            if let Some(current) = r.run_mut(run) {
                current
                    .extra
                    .insert("assignment".into(), assignment(&op.worker));
            }
            set_worker(r, &op.worker);
            r.extra.remove(FIELD);
            r.set_pending_prompt(prompt.then_some(PendingPrompt {
                kind: PromptKind::Restart,
                since: op.since,
                run,
            }));
            if let Some((rev, action)) = recovery {
                stop_automatic_work(r);
                set_receipt(r, &op, rev, final_rev, action);
            }
        })
        .map_err(|e| store_error(&e))?;
    Ok(())
}

pub async fn recover(
    env: &Env,
    task: String,
    rev: u64,
    operation: String,
    action: RestartRecoveryAction,
) -> Result<Record, ErrorBody> {
    let config = Arc::clone(&env.records);
    let requested = operation.clone();
    let (mut claim, repeated) = blocking(move || {
        let store = config.store()?;
        let current = store.get(&task).map_err(|e| store_error(&e))?;
        let claim = store
            .claim(&task, current.rev)
            .map_err(|e| store_error(&e))?;
        let receipt = claim.record().extra.get("restartReceipt");
        let repeated = receipt.is_some_and(|r| {
            r["operation"] == requested
                && r["action"] == action.as_str()
                && r["requestedRev"] == rev
                && r["finalRev"] == claim.record().rev
        });
        if !repeated && claim.record().rev != rev {
            return Err(conflict("task changed; refresh recovery state"));
        }
        if !repeated && !claim.record().extra.contains_key(FIELD) {
            return Err(conflict("no matching restart operation"));
        }
        Ok((claim, repeated))
    })
    .await?;
    if repeated {
        return Ok(claim.record().clone());
    }
    let op = claim
        .record()
        .restart_operation()
        .map_err(lifecycle_error)?
        .ok_or_else(|| conflict("no restart operation"))?;
    if op.id != operation {
        return Err(conflict("restart operation changed"));
    }
    if !summary(claim.record()).is_some_and(|s| s.allows(action)) {
        return Err(conflict("recovery action is not available"));
    }
    match action {
        RestartRecoveryAction::CancelPrepared => {
            return blocking(move || {
                let final_rev = claim.record().rev + 1;
                claim
                    .update(|r| {
                        r.extra.remove(FIELD);
                        stop_automatic_work(r);
                        set_receipt(r, &op, rev, final_rev, action);
                    })
                    .map_err(|e| store_error(&e))?;
                cleanup_cancelled(claim.record(), &op);
                Ok(claim.record().clone())
            })
            .await;
        }
        RestartRecoveryAction::FinishReplacement => {
            let expected = op
                .replacement
                .as_ref()
                .ok_or_else(|| conflict("replacement identity unconfirmed"))?;
            let observed = identity(
                &env.client,
                SessionId::new(
                    op.session
                        .ok_or_else(|| conflict("session identity missing"))?,
                ),
                PaneId::new(op.pane),
            )
            .await?;
            if &observed != expected {
                return Err(conflict("replacement process changed"));
            }
            return blocking(move || {
                let dir = claim
                    .record()
                    .dir
                    .clone()
                    .ok_or_else(|| failed("task directory missing"))?;
                restart_files::reconcile(
                    &dir,
                    op.files.as_ref().ok_or_else(|| conflict("stage missing"))?,
                )
                .map_err(file_error)?;
                finalize(&mut claim, &op, false, Some((rev, action)))?;
                Ok(claim.record().clone())
            })
            .await;
        }
        RestartRecoveryAction::LeaveUnconfirmed => {}
    }
    blocking(move || {
        if op.phase != Phase::Delivering
            && (claim.record().open_run().map(|r| r.id) != op.source_run
                || claim
                    .record()
                    .runs
                    .last()
                    .map_or(Some(1), |r| r.id.checked_add(1))
                    != Some(op.reserved_run))
        {
            return Err(conflict("task run changed during restart"));
        }
        if op.phase == Phase::Delivering && claim.record().open_run().map(|r| r.id) != op.source_run
        {
            return Err(conflict("task run changed during delivery"));
        }
        let final_rev = claim.record().rev + 1;
        claim
            .update(|r| {
                if op.phase != Phase::Delivering {
                    let run = advance(r, op.seen.as_ref(), now());
                    if let Some(run) = r.run_mut(run) {
                        run.extra
                            .insert("assignment".into(), assignment(&op.worker));
                        run.extra
                            .insert("outcome".into(), serde_json::json!("unconfirmed"));
                    }
                    set_worker(r, &op.worker);
                } else if let Some(run) = r.run_mut(op.reserved_run) {
                    run.extra
                        .insert("outcome".into(), serde_json::json!("unconfirmed"));
                } else {
                    r.note =
                        Some("Prompt delivery was not confirmed. Continue in Terminal.".into());
                }
                r.extra.remove(FIELD);
                stop_automatic_work(r);
                set_receipt(r, &op, rev, final_rev, action);
            })
            .map_err(|e| store_error(&e))?;
        Ok(claim.record().clone())
    })
    .await
}

fn stop_automatic_work(record: &mut Record) {
    if let Some(lp) = record.task_loop().ok().flatten().filter(|l| l.active()) {
        let _ = record.set_loop(Some(&lp.stopped(crate::records::Reason::User, now())));
    }
    record.set_pending_prompt(None);
}

fn set_receipt(
    record: &mut Record,
    op: &Operation,
    rev: u64,
    final_rev: u64,
    action: RestartRecoveryAction,
) {
    record.extra.insert(
        "restartReceipt".into(),
        serde_json::json!({
            "operation":op.id,"action":action.as_str(),"requestedRev":rev,"finalRev":final_rev,
            "phase":op.phase,"files":op.files,
        }),
    );
}

pub fn summary(record: &Record) -> Option<muxa_protocol::RestartOperation> {
    if !record.extra.contains_key(FIELD) {
        return None;
    }
    let Ok(Some(op)) = record.restart_operation() else {
        return Some(muxa_protocol::RestartOperation {
            id: String::new(),
            phase: "unknown".into(),
            assignment: None,
            actions: vec![],
        });
    };
    let valid = op.session.is_some()
        && op.source_started.as_ref().is_some_and(|s| !s.is_empty())
        && op.source_pid != 0
        && op.since != 0;
    let actions = if !valid {
        vec![]
    } else {
        match op.phase {
            Phase::Prepared if op.files.is_some() => vec!["cancel-prepared".into()],
            Phase::Dispatching | Phase::Delivering => vec!["leave-unconfirmed".into()],
            Phase::Replaced
                if op.files.is_some()
                    && op.replacement.as_ref().is_some_and(|p| {
                        p.pid != 0 && p.pid != op.source_pid && !p.started.is_empty()
                    }) =>
            {
                vec!["finish-replacement".into(), "leave-unconfirmed".into()]
            }
            _ => vec![],
        }
    };
    Some(muxa_protocol::RestartOperation {
        id: op.id,
        phase: match op.phase {
            Phase::Prepared => "prepared",
            Phase::Dispatching => "dispatching",
            Phase::Replaced => "replaced",
            Phase::Delivering => "delivering",
        }
        .into(),
        assignment: serde_json::from_value(assignment(&op.worker)).ok(),
        actions,
    })
}

pub(super) fn advance(r: &mut Record, seen: Option<&Seen>, at: u64) -> u32 {
    if let (Some(seen), Some(id)) = (seen, r.open_run().map(|run| run.id)) {
        if let Some(run) = r.run_mut(id) {
            run.fill(seen);
        }
    }
    r.start_run(RunReason::Restart, at)
}

fn set_worker(record: &mut Record, worker: &Option<String>) {
    if let Some(name) = worker {
        record
            .extra
            .insert("worker".into(), serde_json::json!(name));
    } else {
        record.extra.remove("worker");
    }
}
fn assignment(worker: &Option<String>) -> serde_json::Value {
    match worker {
        Some(name) => serde_json::json!({"kind":"worker", "name":name}),
        None => serde_json::json!({"kind":"workspace-default"}),
    }
}
async fn workspace_agent(env: &Env, name: &str) -> Result<Option<AgentSettings>, ErrorBody> {
    let config = Arc::clone(&env.records);
    let name = name.to_owned();
    blocking(move || {
        Ok(config
            .load_workspaces()?
            .get(&name)
            .and_then(|w| w.agent.clone()))
    })
    .await
}
fn lifecycle_error(_: crate::records::lifecycle::Error) -> ErrorBody {
    conflict("task restart requires recovery")
}
fn file_error(e: restart_files::Error) -> ErrorBody {
    let code = match &e {
        restart_files::Error::Conflict(_) | restart_files::Error::DirectoryChanged => {
            ErrorCode::Conflict
        }
        _ => ErrorCode::TaskFailed,
    };
    error_body(code, e.to_string())
}
fn failed(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::TaskFailed, message)
}
fn conflict(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::Conflict, message)
}
fn uncertain(operation: &str) -> ErrorBody {
    failed(format!(
        "restart outcome is uncertain; inspect operation {operation}"
    ))
}
