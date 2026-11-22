//! The Task actions of an Inbox item: Approve closes the Task of a Review
//! item (ADR 0068 §7, ADR 0043), Request changes types the user's comment
//! into its agent, and Retry runs again what made a Failed item (ADR 0072).
//! The comment is the user's own input (invariant 2); it never reaches a log.

use std::time::Duration;

use muxa_backend::{BackendError, SessionBackend, TerminalId, TerminalInfo};
use muxa_inbox::{Inbox, InboxError};
use muxa_protocol::v2::{ErrorCode, InboxItem, ItemKind, LoopSpec, Place, WorktreeMode};

use crate::session::{code_of, task_code};
use crate::task_items::is_agent;
use crate::tasks::{self, Host, PLACES};

pub(crate) type Refusal = (ErrorCode, String);

/// The longest comment Request changes types, in bytes.
pub const MAX_COMMENT: usize = 8 * 1024;
/// Bracketed paste: a multi-line comment arrives as one paste, so its
/// newlines do not send it line by line.
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
/// Time for typed bytes to leave before the next write or the detach.
const SETTLE: Duration = Duration::from_millis(200);
/// A killed terminal is removed once its program has ended.
const REMOVE_TRIES: u32 = 20;
const REMOVE_WAIT: Duration = Duration::from_millis(100);

/// `inbox-approve`: ends the agent, removes the Task's branch worktrees,
/// then marks the item Done. Nothing changes when the host refuses the close.
pub(crate) async fn approve<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    inbox: &Inbox,
    id: &str,
    mode: WorktreeMode,
) -> Result<(), Refusal> {
    let item = linked(inbox, id, ItemKind::Review)?;
    let agent = task_agent(backend, &item).await?;
    let places = places_of(&agent);
    host.check_close(&places, mode).await.map_err(refused)?;
    stop_loop(host, &agent).await;
    end_agent(backend, &agent).await?;
    host.close_places(&places, mode).await.map_err(|e| {
        let message = format!("the agent ended, but its places were not all removed: {e}");
        (task_code(&e), message)
    })?;
    tracing::info!(terminal = %agent.id, places = places.len(), "task closed");
    done(inbox, id)
}

/// `inbox-request-changes`: types `comment` into the Task's agent as the
/// user's input, then marks the item Done.
pub(crate) async fn request_changes<B: SessionBackend>(
    backend: &B,
    inbox: &Inbox,
    id: &str,
    comment: &str,
) -> Result<(), Refusal> {
    let item = linked(inbox, id, ItemKind::Review)?;
    let comment = checked_comment(comment)?;
    let agent = task_agent(backend, &item).await?;
    if !agent.live {
        return Err((
            ErrorCode::BadRequest,
            format!(
                "the agent of {} has ended; start the Task again to send it changes",
                agent.id
            ),
        ));
    }
    type_comment(backend, &agent, &comment)
        .await
        .map_err(|e| (code_of(&e), format!("the comment was not typed: {e}")))?;
    tracing::info!(terminal = %agent.id, bytes = comment.len(), "changes requested");
    done(inbox, id)
}

/// `inbox-retry`: a Routine runs again; a Task's live agent restarts its
/// loop from 0; an ended agent starts again in its working place.
pub(crate) async fn retry<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    inbox: &Inbox,
    id: &str,
) -> Result<(), Refusal> {
    let item = inbox.item(id).map_err(inbox_refused)?;
    if item.kind != ItemKind::Failed {
        return Err((
            ErrorCode::BadRequest,
            format!("{id} is not a Failed item; only those can be retried"),
        ));
    }
    if let Some(r) = &item.routine {
        host.run_routine(backend, &r.workspace, &r.name)
            .await
            .map_err(refused)?;
    } else if item.terminal.is_some() {
        let agent = task_agent(backend, &item).await?;
        if agent.live {
            restart_loop(host, &agent).await?;
        } else {
            restart_agent(backend, host, &agent).await?;
        }
    } else {
        return Err((
            ErrorCode::BadRequest,
            format!("{id} was not made by a Routine or a Task; there is nothing to retry"),
        ));
    }
    done(inbox, id)
}

/// The open item `id` of `kind` that a Task made.
fn linked(inbox: &Inbox, id: &str, kind: ItemKind) -> Result<InboxItem, Refusal> {
    let item = inbox.item(id).map_err(inbox_refused)?;
    if item.kind != kind || item.terminal.is_none() {
        return Err((
            ErrorCode::BadRequest,
            format!("{id} is not a {kind:?} item of a Task; mark it Done instead"),
        ));
    }
    Ok(item)
}

/// The agent terminal of a Task's item, live or ended.
async fn task_agent<B: SessionBackend>(
    backend: &B,
    item: &InboxItem,
) -> Result<TerminalInfo, Refusal> {
    let terminal = item.terminal.as_deref().unwrap_or_default();
    let t = backend
        .get(&TerminalId::new(terminal))
        .await
        .map_err(|e| match e {
            BackendError::NotFound(_) => (
                ErrorCode::NotFound,
                format!(
                    "the Task's agent terminal {terminal} is gone, so muxa cannot find its places; mark the item Done instead"
                ),
            ),
            e => (code_of(&e), e.to_string()),
        })?;
    if is_agent(&t) {
        Ok(t)
    } else {
        Err((
            ErrorCode::BadRequest,
            format!("{terminal} is not a Task's agent"),
        ))
    }
}

/// The places on the agent's label; none when it has no readable label.
fn places_of(agent: &TerminalInfo) -> Vec<Place> {
    let Some(raw) = agent.labels.get(PLACES) else {
        return Vec::new();
    };
    serde_json::from_str(raw).unwrap_or_else(|e| {
        tracing::warn!(terminal = %agent.id, error = %e, "unreadable places label");
        Vec::new()
    })
}

/// Stops the agent's loop while it still works; a finished one stays as it is.
async fn stop_loop<H: Host>(host: &H, agent: &TerminalInfo) {
    let id = agent.id.as_str();
    let active =
        host.loops().await.into_iter().any(|l| {
            l.terminal == id && matches!(l.state.as_str(), "armed" | "checking" | "paused")
        });
    if active && let Err(e) = host.loop_stop(id).await {
        tracing::info!(terminal = %agent.id, error = %e, "the Task Loop did not stop");
    }
}

/// Kills a live agent and removes its terminal. A kill that fails stops the
/// close; a terminal that stays is only logged.
async fn end_agent<B: SessionBackend>(backend: &B, agent: &TerminalInfo) -> Result<(), Refusal> {
    if agent.live {
        backend.kill(&agent.id).await.map_err(|e| {
            let message = format!("the agent did not end, so nothing was removed: {e}");
            (code_of(&e), message)
        })?;
    }
    remove_terminal(backend, &agent.id).await;
    Ok(())
}

async fn remove_terminal<B: SessionBackend>(backend: &B, id: &TerminalId) {
    for _ in 0..REMOVE_TRIES {
        match backend.remove(id).await {
            Ok(()) | Err(BackendError::NotFound(_)) => return,
            Err(_) => tokio::time::sleep(REMOVE_WAIT).await,
        }
    }
    tracing::warn!(terminal = %id, "an ended agent's terminal was not removed");
}

/// The comment as it is typed: newlines as `\n`, no other control character.
fn checked_comment(comment: &str) -> Result<String, Refusal> {
    let text = comment.replace("\r\n", "\n");
    let bad = |m: String| Err((ErrorCode::BadRequest, m));
    if text.trim().is_empty() {
        return bad("the comment is empty; say what to change".into());
    }
    if text.len() > MAX_COMMENT {
        return bad(format!("a comment has at most {MAX_COMMENT} bytes"));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return bad(
            "the comment has a control character; send text, newlines and tabs only".into(),
        );
    }
    Ok(text.trim_end().to_owned())
}

/// Types the comment and Enter. One line goes as typed, as the Task Loop
/// types its prompt; several lines go as one bracketed paste.
async fn type_comment<B: SessionBackend>(
    backend: &B,
    agent: &TerminalInfo,
    comment: &str,
) -> Result<(), BackendError> {
    use muxa_backend::TerminalStream;
    // The agent's own size: attaching does not resize it.
    let stream = backend.attach(&agent.id, agent.size, 0).await?;
    let typed = if comment.contains('\n') {
        let mut paste = PASTE_START.to_vec();
        paste.extend_from_slice(comment.as_bytes());
        paste.extend_from_slice(PASTE_END);
        let pasted = stream.input(&paste);
        tokio::time::sleep(SETTLE).await;
        pasted.and_then(|()| stream.input(b"\r"))
    } else {
        let mut line = comment.as_bytes().to_vec();
        line.push(b'\r');
        stream.input(&line)
    };
    tokio::time::sleep(SETTLE).await;
    let _ = stream.detach();
    typed
}

/// Starts the live agent's loop again with the same check, its count at 0.
async fn restart_loop<H: Host>(host: &H, agent: &TerminalInfo) -> Result<(), Refusal> {
    let id = agent.id.as_str();
    let lp = host
        .loops()
        .await
        .into_iter()
        .find(|l| l.terminal == id)
        .ok_or_else(|| {
            (
                ErrorCode::BadRequest,
                format!(
                    "the agent of {id} still runs and has no Task Loop; there is nothing to retry"
                ),
            )
        })?;
    let spec = LoopSpec {
        check: Some(lp.check),
        max_attempts: Some(lp.max_attempts),
        timeout: Some(lp.timeout),
    };
    host.loop_start(agent, &spec, true)
        .await
        .map(drop)
        .map_err(refused)
}

/// Runs the Workspace's agent again where `ended` worked, then removes the
/// ended terminal.
async fn restart_agent<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    ended: &TerminalInfo,
) -> Result<(), Refusal> {
    let prepared = host.restart(ended).await.map_err(refused)?;
    let (started, existing) = tasks::start(backend, prepared)
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    tracing::info!(ended = %ended.id, terminal = %started.id, existing, "task agent restarted");
    remove_terminal(backend, &ended.id).await;
    Ok(())
}

fn done(inbox: &Inbox, id: &str) -> Result<(), Refusal> {
    inbox.change(|b, _| b.done(id)).map_err(inbox_refused)
}

fn refused(e: tasks::TaskError) -> Refusal {
    (task_code(&e), e.to_string())
}

fn inbox_refused(e: InboxError) -> Refusal {
    let code = match e {
        InboxError::Invalid(_) => ErrorCode::BadRequest,
        InboxError::NotFound(_) => ErrorCode::NotFound,
    };
    (code, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_comment_is_text_newlines_and_tabs_only() {
        assert_eq!(
            checked_comment("Rename it.\r\n\tAnd add a test.\n").unwrap(),
            "Rename it.\n\tAnd add a test."
        );
        assert!(checked_comment("  \n").is_err());
        assert!(checked_comment("stop\x1b[201~").is_err());
        assert!(checked_comment("a\rb").is_err());
        assert!(checked_comment(&"x".repeat(MAX_COMMENT + 1)).is_err());
        assert!(checked_comment(&"x".repeat(MAX_COMMENT)).is_ok());
    }
}
