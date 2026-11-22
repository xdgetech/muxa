//! The Inbox items a Task makes (ADR 0066 decision 4): Review when the Task
//! is ready, and Failed when its agent fails. Never from screen content.

use muxa_backend::{
    BackendError, Change, ChangeStream, Exit, Labels, SessionBackend, TerminalId, TerminalInfo,
};
use muxa_inbox::{Inbox, InboxError, NewItem};
use muxa_protocol::v2::{ErrorCode, ItemKind};

use crate::tasks::{BRANCH, KIND, ROLE};

fn agents() -> Labels {
    Labels::from([
        (KIND.to_owned(), "task".to_owned()),
        (ROLE.to_owned(), "agent".to_owned()),
    ])
}

pub(crate) fn is_agent(t: &TerminalInfo) -> bool {
    agents().iter().all(|(k, v)| t.labels.get(k) == Some(v))
}

/// What items call the Task: its branch, else its title (a folder Task).
fn branch(t: &TerminalInfo) -> &str {
    t.labels
        .get(BRANCH)
        .or(t.name.as_ref())
        .map_or("a Task", String::as_str)
}

/// Adds a Failed item for each Task agent that fails, once per terminal,
/// until the backend's change stream ends.
pub async fn watch_tasks<B: SessionBackend>(
    backend: &B,
    inbox: &Inbox,
) -> Result<(), BackendError> {
    let (now, mut changes) = backend.subscribe(&agents()).await?;
    // Agents that failed while nobody watched.
    for t in &now {
        report(inbox, t);
    }
    while let Some(change) = changes.next().await {
        match change? {
            Change::Ended { id, .. } => match backend.get(&id).await {
                Ok(t) => report(inbox, &t),
                Err(e) => tracing::info!(terminal = %id, error = %e, "an ended agent is gone"),
            },
            Change::Resync(all) => all.iter().for_each(|t| report(inbox, t)),
            _ => {}
        }
    }
    Ok(())
}

/// A non-zero exit, a crash on a signal, or a lost program. The user's own
/// kill and a clean exit are not failures.
fn failed(exit: &Exit) -> bool {
    exit.code.is_some_and(|c| c != 0)
        || (exit.signal.is_some() && exit.reason != "killed")
        || exit.reason == "lost"
}

fn report(inbox: &Inbox, t: &TerminalInfo) {
    let Some(exit) = t.exit.as_ref().filter(|e| !t.live && failed(e)) else {
        return;
    };
    let note = match (exit.code, exit.signal) {
        (Some(c), _) => format!("It exited with code {c}."),
        (None, Some(s)) => format!("It ended on signal {s}."),
        (None, None) => format!("It ended: {}.", exit.reason),
    };
    let new = NewItem {
        kind: ItemKind::Failed,
        title: format!("The agent of {} stopped", branch(t)),
        note: Some(note),
        workspace: None,
        due: None,
        source: "task".into(),
        key: Some(format!("failed:{}", t.id)),
        once: true,
        terminal: Some(t.id.as_str().to_owned()),
        routine: None,
    };
    if let Err(e) = inbox.change(|b, now| b.add(new, now).map(drop)) {
        tracing::warn!(terminal = %t.id, error = %e, "no Failed item");
    }
}

/// `task-review`: one open Review item for the Task of `terminal`.
pub(crate) async fn review<B: SessionBackend>(
    backend: &B,
    inbox: &Inbox,
    terminal: &str,
    note: Option<String>,
) -> Result<(), (ErrorCode, String)> {
    let t = backend
        .get(&TerminalId::new(terminal))
        .await
        .map_err(|e| match e {
            BackendError::NotFound(m) => (ErrorCode::NotFound, m),
            e => (ErrorCode::Backend, e.to_string()),
        })?;
    if !is_agent(&t) {
        return Err((
            ErrorCode::BadRequest,
            format!("{terminal} is not a Task's agent; start Tasks with start-task"),
        ));
    }
    let new = NewItem {
        kind: ItemKind::Review,
        title: format!("Review {}", branch(&t)),
        note,
        workspace: None,
        due: None,
        source: "task".into(),
        key: Some(format!("review:{terminal}")),
        once: false,
        terminal: Some(terminal.to_owned()),
        routine: None,
    };
    inbox
        .change(|b, now| b.add(new, now).map(drop))
        .map_err(|e| match e {
            InboxError::Invalid(m) => (ErrorCode::BadRequest, m),
            InboxError::NotFound(m) => (ErrorCode::NotFound, m),
        })
}
