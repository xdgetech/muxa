//! Who uses which connection and MCP server (ADR 0069 §7, ADR 0075): every
//! live Task agent of the Workspace gets all of them. The host fills in the
//! Routines and the times.

use muxa_backend::TerminalInfo;
use muxa_protocol::v2::{UsedBy, Workspace};

use crate::task_items::is_agent;
use crate::tasks::WORKSPACE;

/// Adds each live Task agent of a Workspace to the `tasks` of every
/// connection and MCP server of that Workspace.
pub(crate) fn with_tasks(workspaces: Vec<Workspace>, terminals: &[TerminalInfo]) -> Vec<Workspace> {
    let agents: Vec<&TerminalInfo> = terminals.iter().filter(|t| t.live && is_agent(t)).collect();
    workspaces
        .into_iter()
        .map(|mut ws| {
            let mine: Vec<String> = agents
                .iter()
                .filter(|t| t.labels.get(WORKSPACE) == Some(&ws.name))
                .map(|t| t.id.as_str().to_owned())
                .collect();
            for c in &mut ws.connections {
                add(&mut c.used_by, &mine);
            }
            for m in &mut ws.mcp {
                add(&mut m.used_by, &mine);
            }
            ws
        })
        .collect()
}

fn add(used: &mut UsedBy, agents: &[String]) {
    for id in agents {
        if !used.tasks.contains(id) {
            used.tasks.push(id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use muxa_backend::{Labels, Size, TerminalId};
    use muxa_protocol::v2::{WorkspaceConnection, WorkspaceMcp};

    use super::*;
    use crate::tasks::{CONNECTIONS, MCP};

    fn agent(id: &str, ws: &str, live: bool, extra: &[(&str, &str)]) -> TerminalInfo {
        let mut labels = Labels::from([
            ("muxa.kind".to_owned(), "task".to_owned()),
            ("muxa.role".to_owned(), "agent".to_owned()),
            (WORKSPACE.to_owned(), ws.to_owned()),
        ]);
        labels.extend(
            extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        );
        TerminalInfo {
            id: TerminalId::new(id),
            name: None,
            live,
            status: None,
            labels,
            size: Size { cols: 80, rows: 24 },
            exit: None,
        }
    }

    fn workspace(name: &str) -> Workspace {
        let connection = |name: &str, default| WorkspaceConnection {
            name: name.into(),
            default,
            used_by: UsedBy {
                routines: vec!["triage".into()],
                tasks: Vec::new(),
            },
            last_used: Some(5),
        };
        Workspace {
            name: name.into(),
            root: "/src".into(),
            repos: Vec::new(),
            agent: None,
            task_loop: None,
            connections: vec![connection("github", false), connection("sentry", true)],
            mcp: vec![WorkspaceMcp {
                name: "docs".into(),
                default: false,
                used_by: UsedBy::default(),
                last_used: None,
            }],
            branches: Default::default(),
        }
    }

    #[test]
    fn every_live_task_agent_of_the_workspace_uses_everything_whatever_its_labels() {
        let terminals = [
            agent(
                "t1",
                "app",
                true,
                &[(CONNECTIONS, r#"["github"]"#), (MCP, r#"["docs"]"#)],
            ),
            agent("t2", "app", true, &[]),
            agent("t3", "app", false, &[(CONNECTIONS, r#"["github"]"#)]),
            agent("t4", "other", true, &[(CONNECTIONS, r#"["github"]"#)]),
            agent("t5", "app", true, &[(MCP, "not json")]),
        ];
        let shown = with_tasks(vec![workspace("app")], &terminals);
        let ws = &shown[0];
        for used in [
            &ws.connections[0].used_by,
            &ws.connections[1].used_by,
            &ws.mcp[0].used_by,
        ] {
            assert_eq!(used.tasks, ["t1", "t2", "t5"]);
        }
        assert_eq!(ws.connections[0].used_by.routines, ["triage"]);
        assert_eq!(ws.connections[0].last_used, Some(5));
    }
}
