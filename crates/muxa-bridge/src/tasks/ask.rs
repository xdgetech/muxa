//! Ask sessions (ADR 0031): the workspace's agent in a repo's main checkout,
//! to ask about its code, or at the workspace root (ADR 0041). No Task, no
//! worktree and no record, so the session lists as a manual one. `close-task`
//! closes it and leaves the checkout (ADR 0039).

use std::path::Path;

use muxa_tmux::Client;

use crate::records::Workspaces;
use crate::records::workspaces::slug;

use super::builtin::{Builtin, main_checkout};
use super::{Launched, TaskError, end_session, session_name};

/// Marks the session as an ask session; the value is its workspace. It is
/// never `@muxa_task` or `@muxa_device`.
pub const TAG: &str = "@muxa_ask";
/// Marks the workspace's own ask session at its root (ADR 0041).
pub const ROOT_TAG: &str = "@muxa_ask_root";
/// Names tried before giving up.
const MAX_NAMES: u32 = 100;

/// `ask-<repo>-<n>`.
pub fn name(main: &Path, n: u32) -> String {
    let repo = main
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    session_name(&format!("ask-{}-{n}", slug(&repo)))
}

/// Starts a detached session in the main checkout of `repo`, running the
/// workspace's agent command and environment. `command` is the agent when
/// the workspace sets none. No other session, window or pane changes.
pub async fn start(
    client: &Client,
    command: &str,
    workspaces: &Workspaces,
    workspace: &str,
    repo: &str,
) -> Result<Launched, TaskError> {
    let ws = workspaces.get(workspace).ok_or_else(|| {
        TaskError::Invalid(format!(
            "no workspace named {workspace:?}; list-tasks shows the host's workspaces"
        ))
    })?;
    let path = ws
        .repo_path(repo)
        .map_err(|e| TaskError::Invalid(e.to_string()))?;
    if !path.is_dir() {
        return Err(TaskError::Invalid(format!(
            "{} is not a directory",
            path.display()
        )));
    }
    let main = main_checkout(&path).await?;
    if let Some(owner) = workspaces.owner_elsewhere(&main, workspace) {
        return Err(TaskError::Invalid(format!(
            "{} belongs to workspace {:?}; ask there",
            main.display(),
            owner.name
        )));
    }
    let taken = muxa_tmux::session_names(client).await?;
    let name = (1..=MAX_NAMES)
        .map(|n| name(&main, n))
        .find(|n| !taken.contains(n))
        .ok_or_else(|| {
            TaskError::Failed(format!(
                "{MAX_NAMES} ask sessions for this repo are open; close one first"
            ))
        })?;
    let session = Builtin::new(command.to_owned())
        .start(client, &name, &main, ws.agent.as_ref())
        .await?;
    muxa_tmux::set_session_option(client, session, TAG, &ws.name).await?;
    tracing::info!(session = %session, dir = %main.display(), "ask session started");
    Ok(Launched {
        id: session,
        name,
        dir: main,
    })
}

/// The workspace's ask session at its root, if one is open.
pub fn root_session<'a>(
    sessions: &'a [muxa_tmux::Session],
    workspace: &str,
) -> Option<&'a muxa_tmux::Session> {
    sessions
        .iter()
        .find(|s| s.ask_root && s.ask.as_deref() == Some(workspace) && is_ask(s))
}

/// The workspace's ask session at its root: the open one, or a new one
/// (`true`). Any workspace has one, the overall assistant's too (ADR 0054).
pub async fn open_root(
    client: &Client,
    command: &str,
    workspaces: &Workspaces,
    workspace: &str,
) -> Result<(Launched, bool), TaskError> {
    let ws = workspaces.get(workspace).ok_or_else(|| {
        TaskError::Invalid(format!(
            "no workspace named {workspace:?}; list-tasks shows the host's workspaces"
        ))
    })?;
    let sessions = muxa_tmux::list_sessions(client).await?;
    if let Some(s) = root_session(&sessions, workspace) {
        let open = Launched {
            id: s.id,
            name: s.name.clone(),
            dir: ws.root.clone(),
        };
        return Ok((open, false));
    }
    let launched = start_root(client, command, workspaces, workspace).await?;
    Ok((launched, true))
}

/// Starts the workspace's ask session at its root: `ask-<workspace>`, or
/// `ask-<workspace>-<n>` when that name is taken. As `start`, nothing else changes.
pub async fn start_root(
    client: &Client,
    command: &str,
    workspaces: &Workspaces,
    workspace: &str,
) -> Result<Launched, TaskError> {
    let ws = workspaces.get(workspace).ok_or_else(|| {
        TaskError::Invalid(format!(
            "no workspace named {workspace:?}; list-tasks shows the host's workspaces"
        ))
    })?;
    if !ws.root.is_dir() {
        return Err(TaskError::Invalid(format!(
            "{} is not a directory",
            ws.root.display()
        )));
    }
    let taken = muxa_tmux::session_names(client).await?;
    let base = session_name(&format!("ask-{}", slug(&ws.name)));
    let name = std::iter::once(base.clone())
        .chain((2..=MAX_NAMES).map(|n| session_name(&format!("{base}-{n}"))))
        .find(|n| !taken.contains(n))
        .ok_or_else(|| {
            TaskError::Failed(format!(
                "{MAX_NAMES} sessions named {base} are open; close one first"
            ))
        })?;
    let session = Builtin::new(command.to_owned())
        .start(client, &name, &ws.root, ws.agent.as_ref())
        .await?;
    muxa_tmux::set_session_option(client, session, TAG, &ws.name).await?;
    muxa_tmux::set_session_option(client, session, ROOT_TAG, "1").await?;
    tracing::info!(session = %session, workspace = %ws.name, "workspace ask session started");
    Ok(Launched {
        id: session,
        name,
        dir: ws.root.clone(),
    })
}

/// Whether `close-task` may close `session` as an ask session: it has
/// [`TAG`], and no worktree or Task that a launcher or a record owns.
pub fn is_ask(session: &muxa_tmux::Session) -> bool {
    session.ask.is_some() && session.worktree.is_none() && session.task_id.is_none()
}

/// Closes an ask session. Its device sessions go first and its clients move
/// to another session. The main checkout and its files are never touched.
pub async fn close(client: &Client, session: &muxa_tmux::Session) -> Result<(), TaskError> {
    if !is_ask(session) {
        return Err(TaskError::Invalid(format!(
            "{} is not an ask session",
            session.name
        )));
    }
    end_session(client, session.id, &session.name).await?;
    tracing::info!(session = %session.id, "ask session closed");
    Ok(())
}
