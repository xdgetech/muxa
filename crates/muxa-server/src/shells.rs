//! A Task's shells (ADR 0079): terminals beside its agent, in its working
//! place, that every client lists under the Task. They get no connections:
//! secrets go only into an agent's environment (ADR 0069).

use muxa_backend::{BackendError, Labels, Launch, SessionBackend, TerminalId, TerminalInfo};
use muxa_protocol::v2::ErrorCode;

use crate::session::code_of;
use crate::task_actions::remove_terminal;
use crate::task_items::is_agent;
use crate::tasks::{DIR, Host, KIND, ROLE, TITLE, WORKSPACE};

/// Live shells per Task. With the agent, four panes fit side by side.
pub(crate) const MAX_SHELLS: usize = 3;
const SHELL: &str = "shell";

type Refusal = (ErrorCode, String);

pub(crate) fn is_shell(t: &TerminalInfo) -> bool {
    t.labels.get(KIND).map(String::as_str) == Some("task")
        && t.labels.get(ROLE).map(String::as_str) == Some(SHELL)
}

/// Starts a shell in the working place of the Task whose agent is `terminal`.
pub(crate) async fn start<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    terminal: &str,
) -> Result<TerminalInfo, Refusal> {
    let agent = backend
        .get(&TerminalId::new(terminal.to_owned()))
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    if !is_agent(&agent) || !agent.live {
        let message = format!("{terminal} is not a live Task's agent; a shell starts beside one");
        return Err((ErrorCode::BadRequest, message));
    }
    let Some(dir) = agent.labels.get(DIR).cloned() else {
        return Err((
            ErrorCode::BadRequest,
            format!("{terminal} has no working place"),
        ));
    };
    let all = backend
        .list()
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    let live = all.iter().filter(|t| is_shell_of(t, &dir)).count();
    if live >= MAX_SHELLS {
        let message = format!("a Task has at most {MAX_SHELLS} shells; close one first");
        return Err((ErrorCode::Conflict, message));
    }
    let mut labels = Labels::from([
        (KIND.to_owned(), "task".to_owned()),
        (ROLE.to_owned(), SHELL.to_owned()),
        (DIR.to_owned(), dir.clone()),
    ]);
    for key in [WORKSPACE, TITLE] {
        if let Some(v) = agent.labels.get(key) {
            labels.insert(key.to_owned(), v.clone());
        }
    }
    let launch = Launch {
        argv: host.shell(),
        cwd: Some(dir.into()),
        name: agent
            .name
            .as_ref()
            .map(|n| free_name(&format!("{n}-sh"), &all)),
        labels,
        env: Default::default(),
    };
    let started = backend
        .run(&launch)
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    tracing::info!(terminal = %started.id, agent = %agent.id, "task shell");
    Ok(started)
}

/// Ends a Task's shell and removes its terminal. Any other terminal is refused:
/// muxa never ends what it did not start this way.
pub(crate) async fn close<B: SessionBackend>(backend: &B, terminal: &str) -> Result<(), Refusal> {
    let id = TerminalId::new(terminal.to_owned());
    let t = backend
        .get(&id)
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    if !is_shell(&t) {
        let message = format!("{terminal} is not a Task's shell; only those close this way");
        return Err((ErrorCode::BadRequest, message));
    }
    if t.live {
        backend
            .kill(&id)
            .await
            .map_err(|e| (code_of(&e), e.to_string()))?;
    }
    remove_terminal(backend, &id).await;
    Ok(())
}

/// Ends every shell of the Task in `dir`, as when the Task closes. Failures are
/// only logged: the Task is already closing.
pub(crate) async fn close_all<B: SessionBackend>(backend: &B, dir: &str) {
    let shells = match of_dir(backend, dir).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "the Task's shells were not listed");
            return;
        }
    };
    for s in shells {
        if let Err(e) = backend.kill(&s.id).await {
            tracing::warn!(terminal = %s.id, error = %e, "a Task's shell did not end");
            continue;
        }
        remove_terminal(backend, &s.id).await;
    }
}

/// `base`, else `base` with the first number from 2 that no live terminal
/// has: herdr-core refuses a name a live terminal has.
fn free_name(base: &str, terminals: &[TerminalInfo]) -> String {
    let taken = |name: &str| {
        terminals
            .iter()
            .any(|t| t.live && t.name.as_deref() == Some(name))
    };
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_owned())
}

fn is_shell_of(t: &TerminalInfo, dir: &str) -> bool {
    t.live && is_shell(t) && t.labels.get(DIR).map(String::as_str) == Some(dir)
}

async fn of_dir<B: SessionBackend>(
    backend: &B,
    dir: &str,
) -> Result<Vec<TerminalInfo>, BackendError> {
    Ok(backend
        .list()
        .await?
        .into_iter()
        .filter(|t| is_shell_of(t, dir))
        .collect())
}
