//! Lists sessions and panes, and resolves what each pane is for.

use crate::client::Client;
use crate::escape::quote;
use crate::roles::{self, PaneTags};
use crate::{Error, PaneId, SessionId, WindowId};

/// What a pane is for. Only tags decide this; see [`crate::roles`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Agent,
    Shell,
    /// Not a work pane, e.g. a runner's sidebar. Clients do not show it.
    Hidden,
    Unknown,
}

/// Agent state as reported by an agent runner's hooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentState {
    Running,
    Waiting,
    Idle,
    Done,
    Error,
}

impl AgentState {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "running" => Self::Running,
            "waiting" => Self::Waiting,
            "idle" => Self::Idle,
            "done" => Self::Done,
            "error" => Self::Error,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentStatus {
    pub state: AgentState,
    /// Epoch seconds of the last state change, when the runner records it.
    pub since: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub id: PaneId,
    pub window: WindowId,
    pub index: u32,
    pub role: Role,
    pub cols: u16,
    pub rows: u16,
    pub command: String,
    pub path: String,
    pub dead: bool,
    /// The pane's first process (`pane_pid`); 0 when tmux did not report one.
    pub pid: u32,
    /// The foreground process is a shell. An agent state on such a pane is
    /// likely stale: the agent exited without telling its runner.
    pub shell_foreground: bool,
    pub agent: Option<AgentStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: SessionId,
    pub name: String,
    pub repo: Option<String>,
    /// The repo's main checkout, absolute.
    pub repo_dir: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub task: Option<String>,
    pub panes: Vec<Pane>,
}

const SEP: char = '\u{1f}';

/// The list-panes format, one entry per field, joined by \x1f.
const FIELDS: [&str; 23] = [
    "session_id",
    "session_name",
    "window_id",
    "pane_index",
    "pane_id",
    "pane_width",
    "pane_height",
    "pane_current_command",
    "pane_current_path",
    "pane_dead",
    "@muxa_role",
    // tmux-agents tags
    "@agents_sidebar_pane",
    "@agents_role",
    "@agent_session_id",
    "@agent_state",
    "@agent_state_at",
    "@agent_repo",
    "@agent_repo_dir",
    "@agent_branch",
    "@agent_worktree",
    "@agent_task",
    "window_index",
    "pane_pid",
];

fn format(fields: &[&str]) -> String {
    let joined: Vec<String> = fields.iter().map(|f| format!("#{{{f}}}")).collect();
    quote(&joined.join(&SEP.to_string()))
}

/// Every session with its panes, in tmux's order. Hidden panes are included
/// with [`Role::Hidden`].
pub async fn list_sessions(client: &Client) -> Result<Vec<Session>, Error> {
    let reply = client
        .command(&format!("list-panes -a -F {}", format(&FIELDS)))
        .await?;
    Ok(parse_sessions(&reply.lines))
}

/// One pane and the name of its session. Roles of a single pane are resolved
/// without its window's other panes, so legacy agent inference does not apply.
pub async fn pane(client: &Client, pane: PaneId) -> Result<(String, Pane), Error> {
    let filter = quote(&format!("#{{==:#{{pane_id}},{pane}}}"));
    let reply = client
        .command(&format!("list-panes -a -f {filter} -F {}", format(&FIELDS)))
        .await?;
    parse_sessions(&reply.lines)
        .into_iter()
        .find_map(|s| {
            let name = s.name;
            s.panes
                .into_iter()
                .find(|p| p.id == pane)
                .map(|p| (name, p))
        })
        .ok_or(Error::NoPane(pane))
}

/// The session and window that hold `pane`.
pub async fn locate(client: &Client, pane: PaneId) -> Result<(SessionId, WindowId), Error> {
    let filter = quote(&format!("#{{==:#{{pane_id}},{pane}}}"));
    let reply = client
        .command(&format!(
            "list-panes -a -f {filter} -F {}",
            format(&["session_id", "window_id"])
        ))
        .await?;
    let line = reply.lines.first().ok_or(Error::NoPane(pane))?;
    let text = String::from_utf8_lossy(line);
    let (s, w) = text
        .split_once(SEP)
        .ok_or_else(|| Error::Unexpected("pane location".into()))?;
    Ok((s.parse()?, w.parse()?))
}

struct Row<'a>(Vec<&'a str>);

impl Row<'_> {
    fn get(&self, name: &str) -> &str {
        FIELDS
            .iter()
            .position(|f| *f == name)
            .and_then(|i| self.0.get(i).copied())
            .unwrap_or("")
    }

    fn opt(&self, name: &str) -> Option<String> {
        Some(self.get(name))
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    }
}

pub(crate) fn parse_sessions(lines: &[Vec<u8>]) -> Vec<Session> {
    let texts: Vec<String> = lines
        .iter()
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect();
    let mut sessions: Vec<Session> = Vec::new();
    let mut tags: Vec<Vec<PaneTags>> = Vec::new();
    for text in &texts {
        let row = Row(text.split(SEP).collect());
        // A name or path containing \x1f shifts the fields; skip that row.
        if row.0.len() != FIELDS.len() {
            tracing::warn!("skipping a list-panes row with {} fields", row.0.len());
            continue;
        }
        let (Ok(session), Ok(pane), Ok(window)) = (
            row.get("session_id").parse::<SessionId>(),
            row.get("pane_id").parse(),
            row.get("window_id").parse(),
        ) else {
            continue;
        };
        let i = match sessions.iter().position(|s| s.id == session) {
            Some(i) => i,
            None => {
                sessions.push(Session {
                    id: session,
                    name: row.get("session_name").to_owned(),
                    repo: row.opt("@agent_repo"),
                    repo_dir: row.opt("@agent_repo_dir"),
                    branch: row.opt("@agent_branch"),
                    worktree: row.opt("@agent_worktree"),
                    task: row.opt("@agent_task"),
                    panes: Vec::new(),
                });
                tags.push(Vec::new());
                sessions.len() - 1
            }
        };
        let command = row.get("pane_current_command").to_owned();
        sessions[i].panes.push(Pane {
            id: pane,
            window,
            index: row.get("pane_index").parse().unwrap_or(0),
            role: Role::Unknown,
            cols: row.get("pane_width").parse().unwrap_or(0),
            rows: row.get("pane_height").parse().unwrap_or(0),
            shell_foreground: roles::is_shell(&command),
            command,
            path: row.get("pane_current_path").to_owned(),
            dead: row.get("pane_dead") == "1",
            pid: row.get("pane_pid").parse().unwrap_or(0),
            agent: None,
        });
        tags[i].push(PaneTags {
            muxa_role: row.opt("@muxa_role"),
            sidebar: row.get("@agents_sidebar_pane") == "1",
            agents_role: row.opt("@agents_role"),
            agent_session_id: row.opt("@agent_session_id"),
            agent_state: row.opt("@agent_state"),
            agent_state_at: row.get("@agent_state_at").parse().ok(),
        });
    }
    for (session, tags) in sessions.iter_mut().zip(&tags) {
        roles::assign(&mut session.panes, tags);
    }
    sessions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(fields: &[(&str, &str)]) -> Vec<u8> {
        FIELDS
            .iter()
            .map(|f| fields.iter().find(|(k, _)| k == f).map_or("", |(_, v)| *v))
            .collect::<Vec<_>>()
            .join(&SEP.to_string())
            .into_bytes()
    }

    #[test]
    fn groups_panes_by_session_with_metadata() {
        let lines = vec![
            row(&[
                ("session_id", "$1"),
                ("session_name", "feat"),
                ("window_id", "@1"),
                ("pane_id", "%1"),
                ("pane_width", "100"),
                ("pane_height", "40"),
                ("pane_current_command", "zsh"),
                ("pane_pid", "4242"),
                ("@agent_repo", "app"),
                ("@agent_repo_dir", "/src/app"),
                ("@agent_branch", "feat/x"),
            ]),
            row(&[
                ("session_id", "$1"),
                ("session_name", "feat"),
                ("window_id", "@1"),
                ("pane_id", "%2"),
                ("pane_current_command", "node"),
            ]),
            row(&[
                ("session_id", "$2"),
                ("session_name", "other"),
                ("window_id", "@2"),
                ("pane_id", "%3"),
            ]),
        ];
        let sessions = parse_sessions(&lines);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].name, "feat");
        assert_eq!(sessions[0].repo.as_deref(), Some("app"));
        assert_eq!(sessions[0].repo_dir.as_deref(), Some("/src/app"));
        assert_eq!(sessions[0].panes.len(), 2);
        assert_eq!(
            (sessions[0].panes[0].cols, sessions[0].panes[0].rows),
            (100, 40)
        );
        assert!(sessions[0].panes[0].shell_foreground);
        assert_eq!(sessions[0].panes[0].pid, 4242);
        assert_eq!(sessions[0].panes[1].pid, 0);
        assert!(!sessions[0].panes[1].shell_foreground);
        assert_eq!(sessions[1].repo, None);
    }

    #[test]
    fn skips_rows_with_shifted_fields() {
        let mut bad = row(&[("session_id", "$1"), ("window_id", "@1"), ("pane_id", "%1")]);
        bad.extend_from_slice("\u{1f}extra".as_bytes());
        assert!(parse_sessions(&[bad]).is_empty());
    }
}
