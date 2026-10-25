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

pub fn error_body(code: wire::ErrorCode, message: impl Into<String>) -> wire::ErrorBody {
    wire::ErrorBody {
        code,
        message: message.into(),
    }
}
