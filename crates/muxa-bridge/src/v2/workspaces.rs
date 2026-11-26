//! The Workspaces as `workspaces` shows them: names, repos and choices a
//! client offers when it starts a Task. Never an agent's arguments or
//! environment, and never a connection's variable, account or secret.

use std::path::Path;

use muxa_protocol::v2::{
    self, LoopChecks, UsedBy, WorkspaceConnection, WorkspaceLoop, WorkspaceMcp,
};

use crate::launch::program;
use crate::records::Workspace;
use crate::records::loops::Checks;
use crate::records::routines::Routine;
use crate::records::workers::LaunchCommand;
use crate::usage::WorkspaceUsage;

/// `default_command` is the agent of a Workspace that names none. Blocking:
/// it scans the root for repos. `routines` are the Workspace's, and `usage`
/// its last starts; the server adds the live Tasks to `used_by`.
pub fn to_v2(
    ws: &Workspace,
    default_command: &str,
    routines: &[&Routine],
    usage: &WorkspaceUsage,
) -> v2::Workspace {
    let command = ws
        .agent
        .as_ref()
        .and_then(|a| a.command.clone())
        .unwrap_or_else(|| default_command.to_owned());
    let listed = ws.repo_list();
    let mut repos: Vec<String> = listed
        .iter()
        .filter_map(|r| repo_name(&ws.root, r))
        .collect();
    repos.sort();
    repos.dedup();
    let branches = listed
        .iter()
        .filter_map(|r| Some((repo_name(&ws.root, r)?, head_branch(r)?)))
        .collect();
    v2::Workspace {
        name: ws.name.clone(),
        root: ws.root.to_string_lossy().into_owned(),
        repos,
        agent: program(&LaunchCommand::Shell(command)).map(str::to_owned),
        task_loop: ws.loop_checks.as_ref().map(|c| WorkspaceLoop {
            checks: match c {
                Checks::Any => LoopChecks::Any,
                Checks::List(list) => LoopChecks::List(list.clone()),
            },
        }),
        connections: ws
            .connections
            .iter()
            .map(|c| WorkspaceConnection {
                name: c.name.clone(),
                default: true,
                used_by: every_routine(routines),
                last_used: usage.connections.get(&c.name).copied(),
            })
            .collect(),
        mcp: ws
            .mcp
            .iter()
            .map(|m| WorkspaceMcp {
                name: m.name.clone(),
                default: true,
                used_by: every_routine(routines),
                last_used: usage.mcp.get(&m.name).copied(),
            })
            .collect(),
        branches,
    }
}

/// The branch `repo`'s own checkout is on, read from `.git/HEAD` without
/// running git: one request lists every repo. None on a detached HEAD.
fn head_branch(repo: &Path) -> Option<String> {
    let head = std::fs::read_to_string(repo.join(".git").join("HEAD")).ok()?;
    head.trim()
        .strip_prefix("ref: refs/heads/")
        .map(str::to_owned)
}

/// Every Routine of the Workspace gets every connection and MCP server
/// (ADR 0075); the server adds the live Tasks.
fn every_routine(routines: &[&Routine]) -> UsedBy {
    UsedBy {
        routines: routines.iter().map(|r| r.name.clone()).collect(),
        tasks: Vec::new(),
    }
}

/// What `start-task` takes for `repo`: the path under the root, else the
/// folder name of a listed repo.
fn repo_name(root: &Path, repo: &Path) -> Option<String> {
    let name = match repo.strip_prefix(root) {
        Ok(under) if !under.as_os_str().is_empty() => under,
        _ => Path::new(repo.file_name()?),
    };
    Some(name.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::records::agent_env::AgentSettings;
    use crate::records::connections::{Access, Connection};
    use crate::records::mcp::{McpServer, Transport};

    fn workspace(root: &Path) -> Workspace {
        Workspace {
            name: "app".into(),
            root: root.to_owned(),
            repos: vec![PathBuf::from("/elsewhere/tools")],
            missing_repos: Vec::new(),
            hidden_repos: Vec::new(),
            agent: Some(AgentSettings {
                command: Some("FOO=1 /opt/bin/claude --model x".into()),
                env: vec![("TOKEN".into(), "secret".into())],
            }),
            loop_checks: Some(Checks::List(vec!["make test".into()])),
            park_after: None,
            routines: None,
            connections: vec![Connection {
                name: "github".into(),
                env: "GH_TOKEN".into(),
                account: Some("me".into()),
                access: Access::default(),
            }],
            mcp: vec![McpServer {
                name: "docs".into(),
                transport: Transport::Stdio {
                    command: "docs-mcp".into(),
                    args: vec!["--key-file".into()],
                },
                env: [("DOCS_HOST".into(), "internal.example".into())].into(),
                headers: Default::default(),
                connections: vec!["github".into()],
            }],
            claude_connectors: false,
            bot_rules: Default::default(),
            subscriptions: Vec::new(),
            switch_subscriptions: true,
            build: Default::default(),
        }
    }

    #[test]
    fn a_workspace_shows_names_and_choices_but_nothing_secret() {
        let root = std::env::temp_dir().join(format!("muxa-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for repo in ["app", "group/lib"] {
            std::fs::create_dir_all(root.join(repo).join(".git")).unwrap();
        }
        std::fs::write(root.join("app/.git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(root.join("group/lib/.git/HEAD"), "4f2a9c0\n").unwrap();
        let root = root.canonicalize().unwrap();
        let shown = to_v2(&workspace(&root), "codex", &[], &WorkspaceUsage::default());
        assert_eq!(shown.repos, ["app", "group/lib", "tools"]);
        assert_eq!(
            shown.branches,
            [("app".to_owned(), "main".to_owned())].into(),
            "a detached HEAD and a missing repo have no branch"
        );
        assert_eq!(shown.agent.as_deref(), Some("claude"));
        assert_eq!(
            shown.task_loop,
            Some(WorkspaceLoop {
                checks: LoopChecks::List(vec!["make test".into()])
            })
        );
        let wire = serde_json::to_string(&shown).unwrap();
        assert_eq!(
            shown.mcp,
            [WorkspaceMcp {
                name: "docs".into(),
                default: true,
                used_by: UsedBy::default(),
                last_used: None,
            }]
        );
        for secret in [
            "secret",
            "TOKEN",
            "GH_TOKEN",
            "--model",
            "\"me\"",
            "docs-mcp",
            "--key-file",
            "internal.example",
        ] {
            assert!(!wire.contains(secret), "{secret} leaked: {wire}");
        }
        let plain = Workspace {
            agent: None,
            loop_checks: None,
            ..workspace(&root)
        };
        let shown = to_v2(&plain, "codex --full-auto", &[], &WorkspaceUsage::default());
        assert_eq!(
            (shown.agent.as_deref(), shown.task_loop),
            (Some("codex"), None)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn routine(name: &str) -> Routine {
        Routine {
            workspace: "app".into(),
            name: name.into(),
            trigger: None,
            worker: None,
            instructions: "/i.md".into(),
            dir: "/".into(),
            items: 5,
            direct_writes: false,
            project: None,
            enabled: true,
            bot: None,
            read_only: false,
            on: None,
        }
    }

    #[test]
    fn every_routine_uses_every_connection_and_server_with_its_last_use() {
        let mut ws = workspace(Path::new("/nonexistent"));
        ws.connections.push(Connection {
            name: "sentry".into(),
            env: "SENTRY_TOKEN".into(),
            account: None,
            access: Access::default(),
        });
        let (triage, digest) = (routine("triage"), routine("digest"));
        let usage = WorkspaceUsage {
            connections: [("github".into(), 20)].into(),
            mcp: [("docs".into(), 10)].into(),
        };
        let shown = to_v2(&ws, "claude", &[&triage, &digest], &usage);
        for c in &shown.connections {
            assert_eq!(c.used_by.routines, ["triage", "digest"]);
            assert!(c.default, "default is always true (ADR 0075)");
        }
        assert_eq!(shown.connections[0].last_used, Some(20));
        assert_eq!(shown.connections[1].last_used, None);
        assert_eq!(shown.mcp[0].used_by.routines, ["triage", "digest"]);
        assert_eq!(shown.mcp[0].last_used, Some(10));
        assert!(shown.mcp[0].used_by.tasks.is_empty());
    }
}
