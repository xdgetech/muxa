//! Maps muxa-tmux types to wire types. The protocol crate stays independent
//! of tmux; this is the only place the two meet.

use muxa_protocol as wire;
use muxa_tmux as tmux;

pub fn sessions(sessions: Vec<tmux::Session>) -> Vec<wire::Session> {
    sessions.into_iter().map(session).collect()
}

fn session(s: tmux::Session) -> wire::Session {
    wire::Session {
        id: s.id.to_string(),
        name: s.name,
        repo: s.repo,
        repo_dir: s.repo_dir,
        branch: s.branch,
        worktree: s.worktree,
        task: s.task,
        task_id: s.task_id,
        ask: s.ask,
        panes: s.panes.into_iter().map(pane).collect(),
    }
}

fn pane(p: tmux::Pane) -> wire::Pane {
    wire::Pane {
        id: p.id.to_string(),
        window: p.window.to_string(),
        index: p.index,
        role: match p.role {
            tmux::Role::Agent => wire::Role::Agent,
            tmux::Role::Shell => wire::Role::Shell,
            tmux::Role::Hidden => wire::Role::Hidden,
            tmux::Role::Unknown => wire::Role::Unknown,
        },
        cols: p.cols,
        rows: p.rows,
        command: p.command,
        path: p.path,
        dead: p.dead,
        shell_foreground: p.shell_foreground,
        agent: p.agent.map(|a| wire::Agent {
            state: match a.state {
                tmux::AgentState::Running => wire::AgentState::Running,
                tmux::AgentState::Waiting => wire::AgentState::Waiting,
                tmux::AgentState::Idle => wire::AgentState::Idle,
                tmux::AgentState::Done => wire::AgentState::Done,
                tmux::AgentState::Error => wire::AgentState::Error,
            },
            since: a.since,
        }),
    }
}

/// The store checks that paths are UTF-8, so nothing is lost here.
pub fn task_record(r: crate::records::Record) -> wire::TaskRecord {
    use crate::records::Status;
    let path = |p: Option<std::path::PathBuf>| p.map(|p| p.to_string_lossy().into_owned());
    let pending_prompt = r.pending_prompt().map(pending_prompt);
    let task_loop = loop_of(&r);
    let worker = r.worker().map(str::to_owned);
    let work = r.work().map(str::to_owned);
    let read = r.is_read();
    let restart_operation = crate::task_records::restart::summary(&r).map(Box::new);
    wire::TaskRecord {
        id: r.id,
        rev: r.rev,
        workspace: r.workspace,
        title: r.title,
        goal: r.goal,
        status: match r.status {
            Status::Open => wire::TaskStatus::Open,
            Status::Review => wire::TaskStatus::Review,
            Status::Done => wire::TaskStatus::Done,
            Status::Dropped => wire::TaskStatus::Dropped,
        },
        created_at: r.created_at,
        updated_at: r.updated_at,
        project: r.project,
        repo: path(r.repo),
        branch: r.branch,
        read,
        dir: path(r.dir),
        session: r.session,
        note: r.note,
        runs: r.runs.into_iter().map(agent_run).collect(),
        r#loop: task_loop.map(Box::new),
        pending_prompt,
        worker,
        restart_operation,
        work,
    }
}

fn pending_prompt(p: crate::records::PendingPrompt) -> wire::PendingPrompt {
    use crate::records::PromptKind;
    wire::PendingPrompt {
        kind: match p.kind {
            PromptKind::Start => wire::PendingPromptKind::Start,
            PromptKind::Restart => wire::PendingPromptKind::Restart,
        },
        since: p.since,
        run: p.run,
    }
}

/// `checks` is absent when the workspace has no `[workspace.loop]` (ADR 0019).
/// Blocking: it scans the root for repos (ADR 0031).
pub fn workspace(w: &crate::records::Workspace) -> wire::WorkspaceInfo {
    use crate::records::Checks;
    wire::WorkspaceInfo {
        name: w.name.clone(),
        checks: w.loop_checks.as_ref().map(|c| match c {
            Checks::Any => wire::WorkspaceChecks::Any,
            Checks::List(list) => wire::WorkspaceChecks::List(list.clone()),
        }),
        root: Some(w.root.to_string_lossy().into_owned()),
        repos: w
            .repo_list()
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
    }
}

/// A worker's public metadata. Args, instructions and env stay on the host.
pub fn worker(w: &crate::records::workers::Worker) -> wire::WorkerInfo {
    wire::WorkerInfo {
        name: w.name.clone(),
        agent: w.agent.id().to_owned(),
        description: w.description.clone(),
        workspaces: w.workspaces.clone(),
        r#loop: w.loop_defaults.as_ref().map(|l| wire::LoopSpec {
            check: l.check.clone(),
            max_attempts: l.max_attempts,
            timeout: l.timeout,
        }),
    }
}

/// The record's loop. A state or reason from a newer muxa has no wire value,
/// so the loop is left out rather than shown with an invented one (invariant 6).
fn loop_of(r: &crate::records::Record) -> Option<wire::TaskLoop> {
    use crate::records::{LoopState, Reason};
    let lp = match r.task_loop() {
        Ok(lp) => lp?,
        Err(e) => {
            tracing::warn!(task = %r.id, error = %e, "unreadable loop; not sent");
            return None;
        }
    };
    let state = match lp.state {
        LoopState::Armed => wire::LoopState::Armed,
        LoopState::Checking => wire::LoopState::Checking,
        LoopState::Paused => wire::LoopState::Paused,
        LoopState::Passed => wire::LoopState::Passed,
        LoopState::Stopped => wire::LoopState::Stopped,
        LoopState::Unknown(_) => {
            tracing::warn!(task = %r.id, "a loop state from a newer muxa; not sent");
            return None;
        }
    };
    let reason = match lp.reason {
        None => None,
        Some(reason) => Some(match reason {
            Reason::Waiting => wire::LoopReason::Waiting,
            Reason::Limit => wire::LoopReason::Limit,
            Reason::Timeout => wire::LoopReason::Timeout,
            Reason::Error => wire::LoopReason::Error,
            Reason::NoAgent => wire::LoopReason::NoAgent,
            Reason::NotSent => wire::LoopReason::NotSent,
            Reason::Stalled => wire::LoopReason::Stalled,
            Reason::Status => wire::LoopReason::Status,
            Reason::User => wire::LoopReason::User,
            Reason::Unknown(_) => {
                tracing::warn!(task = %r.id, "a loop reason from a newer muxa; not sent");
                return None;
            }
        }),
    };
    Some(wire::TaskLoop {
        check: lp.check,
        max_attempts: lp.max_attempts,
        timeout: lp.timeout,
        state,
        attempts: lp.attempts,
        armed_at: lp.armed_at,
        since: lp.since,
        reason,
        last_check: lp.last_check.map(|c| wire::LoopCheck {
            attempt: c.attempt,
            exit: c.exit,
            seconds: c.seconds,
            at: c.at,
        }),
    })
}

/// A client's loop request, as the records keep it.
pub fn loop_spec(s: wire::LoopSpec) -> crate::records::LoopSpec {
    crate::records::LoopSpec {
        check: s.check,
        max_attempts: s.max_attempts,
        timeout: s.timeout,
    }
}

fn agent_run(r: crate::records::Run) -> wire::AgentRun {
    use crate::records::RunReason;
    let worker = r.worker();
    let outcome = r
        .extra
        .get("outcome")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    wire::AgentRun {
        id: r.id,
        agent: r.agent,
        started_at: r.started_at,
        ended_at: r.ended_at,
        agent_session: r.agent_session,
        transcript: r.transcript.map(|p| p.to_string_lossy().into_owned()),
        worker,
        outcome,
        reason: match r.reason {
            RunReason::Start => wire::RunReason::Start,
            RunReason::Restart => wire::RunReason::Restart,
        },
    }
}

pub fn error(e: &tmux::Error) -> wire::ErrorBody {
    let code = match e {
        tmux::Error::InvalidId(_) => wire::ErrorCode::InvalidPane,
        tmux::Error::NoPane(_) => wire::ErrorCode::NoPane,
        tmux::Error::UnsupportedKey(_) => wire::ErrorCode::UnsupportedKey,
        tmux::Error::Tmux { .. } | tmux::Error::Skipped { .. } | tmux::Error::Exited(_) => {
            wire::ErrorCode::Tmux
        }
        _ => wire::ErrorCode::Internal,
    };
    wire::ErrorBody {
        code,
        message: e.to_string(),
    }
}

pub fn task_error(e: &crate::tasks::TaskError) -> wire::ErrorBody {
    use crate::tasks::TaskError;
    match e {
        TaskError::Invalid(m) => error_body(wire::ErrorCode::BadRequest, m.clone()),
        TaskError::Failed(m) => error_body(wire::ErrorCode::TaskFailed, m.clone()),
        TaskError::Tmux(e) => error(e),
    }
}

pub fn error_body(code: wire::ErrorCode, message: impl Into<String>) -> wire::ErrorBody {
    wire::ErrorBody {
        code,
        message: message.into(),
    }
}

/// An adapter's state, in tmux's terms, so it travels with the tag-made ones.
pub fn agent_status(a: &wire::Agent) -> tmux::AgentStatus {
    tmux::AgentStatus {
        state: match a.state {
            wire::AgentState::Running => tmux::AgentState::Running,
            wire::AgentState::Waiting => tmux::AgentState::Waiting,
            wire::AgentState::Idle => tmux::AgentState::Idle,
            wire::AgentState::Done => tmux::AgentState::Done,
            wire::AgentState::Error => tmux::AgentState::Error,
        },
        since: a.since,
    }
}
