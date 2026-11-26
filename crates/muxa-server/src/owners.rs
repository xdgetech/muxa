//! Which muxa terminals a host resource report groups together (ADR 0103):
//! a Task is its agent and its shells. The host maps processes onto these.

use muxa_backend::TerminalInfo;
use muxa_protocol::v2::UsageKind;

use crate::shells::is_shell;
use crate::task_items::is_agent;
use crate::tasks::{DIR, KIND, TITLE, WORKSPACE};

/// The `muxa.kind` of a Routine run and of a team member.
const ROUTINE: &str = "routine";
const MEMBER: &str = "handoff";
/// Agent states that mean it waits for the user.
const IDLE_STATES: [&str; 2] = ["done", "idle"];

/// The terminals of one Task, talk, Routine run or team member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageOwner {
    pub kind: UsageKind,
    pub name: String,
    /// The agent's terminal id.
    pub terminal: String,
    pub workspace: Option<String>,
    /// When the agent last reported `done` or `idle`.
    pub idle_since_ms: Option<u64>,
    /// The program pids of its terminals, the agent's first.
    pub pids: Vec<u32>,
}

/// The owners among the live terminals. A shell joins the agent with the
/// same `muxa.dir`; a shell with no live agent is left out.
pub fn owners(terminals: &[TerminalInfo]) -> Vec<UsageOwner> {
    let live: Vec<&TerminalInfo> = terminals.iter().filter(|t| t.live).collect();
    let mut out: Vec<UsageOwner> = live.iter().filter_map(|t| owner(t)).collect();
    for shell in live.iter().filter(|t| is_shell(t)) {
        let dir = shell.labels.get(DIR);
        let agent = live
            .iter()
            .find(|t| is_agent(t) && t.labels.get(DIR) == dir)
            .map(|t| t.id.to_string());
        let found = out.iter_mut().find(|o| Some(&o.terminal) == agent.as_ref());
        if let (Some(o), Some(pid)) = (found, shell.pid) {
            o.pids.push(pid);
        }
    }
    out
}

fn owner(t: &TerminalInfo) -> Option<UsageOwner> {
    let kind = match t.labels.get(KIND).map(String::as_str) {
        _ if is_agent(t) => UsageKind::Task,
        Some(ROUTINE) => UsageKind::Routine,
        Some(MEMBER) => UsageKind::Member,
        _ => return None,
    };
    let idle = t
        .status
        .as_ref()
        .is_some_and(|s| IDLE_STATES.contains(&s.state.as_str()));
    Some(UsageOwner {
        kind,
        name: t
            .labels
            .get(TITLE)
            .cloned()
            .or_else(|| t.name.clone())
            .unwrap_or_else(|| t.id.to_string()),
        terminal: t.id.to_string(),
        workspace: t.labels.get(WORKSPACE).cloned(),
        idle_since_ms: t.status_since_ms.filter(|_| idle),
        pids: t.pid.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_backend::{Labels, ProgramStatus, Size, TerminalId};

    fn term(id: &str, pid: u32, labels: &[(&str, &str)]) -> TerminalInfo {
        TerminalInfo {
            id: TerminalId::new(id),
            name: Some(id.to_owned()),
            live: true,
            status: None,
            labels: labels
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect::<Labels>(),
            size: Size { cols: 80, rows: 24 },
            exit: None,
            pid: Some(pid),
            status_since_ms: None,
        }
    }

    const AGENT: [(&str, &str); 5] = [
        ("muxa.kind", "task"),
        ("muxa.role", "agent"),
        ("muxa.dir", "/w/a"),
        ("muxa.title", "Fix it"),
        ("muxa.workspace", "xdge"),
    ];

    #[test]
    fn a_shell_joins_its_agent_and_idle_time_shows_only_when_idle() {
        let mut agent = term("t1", 100, &AGENT);
        agent.status = Some(ProgramStatus {
            state: "done".into(),
            app: None,
        });
        agent.status_since_ms = Some(42);
        let shell = term(
            "t2",
            200,
            &[
                ("muxa.kind", "task"),
                ("muxa.role", "shell"),
                ("muxa.dir", "/w/a"),
            ],
        );
        let lone = term(
            "t3",
            300,
            &[
                ("muxa.kind", "task"),
                ("muxa.role", "shell"),
                ("muxa.dir", "/w/b"),
            ],
        );
        let routine = term("t4", 400, &[("muxa.kind", "routine")]);
        let other = term("t5", 500, &[]);
        let got = owners(&[agent, shell, lone, routine, other]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].kind, UsageKind::Task);
        assert_eq!(got[0].name, "Fix it");
        assert_eq!(got[0].workspace.as_deref(), Some("xdge"));
        assert_eq!(got[0].pids, vec![100, 200]);
        assert_eq!(got[0].idle_since_ms, Some(42));
        assert_eq!(got[1].kind, UsageKind::Routine);

        let mut working = term("t1", 100, &AGENT);
        working.status = Some(ProgramStatus {
            state: "working".into(),
            app: None,
        });
        working.status_since_ms = Some(42);
        assert_eq!(owners(&[working])[0].idle_since_ms, None);
    }
}
