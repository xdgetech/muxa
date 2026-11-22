//! The Workspaces as `workspaces` shows them: names, repos and choices a
//! client offers when it starts a Task. Never an agent's arguments or
//! environment, and never a connection's variable, account or secret.

use std::path::Path;

use muxa_protocol::v2::{self, LoopChecks, WorkspaceConnection, WorkspaceLoop, WorkspaceMcp};

use crate::launch::program;
use crate::records::Workspace;
use crate::records::loops::Checks;
use crate::records::workers::LaunchCommand;

/// `default_command` is the agent of a Workspace that names none. Blocking:
/// it scans the root for repos.
pub fn to_v2(ws: &Workspace, default_command: &str) -> v2::Workspace {
    let command = ws
        .agent
        .as_ref()
        .and_then(|a| a.command.clone())
        .unwrap_or_else(|| default_command.to_owned());
    let mut repos: Vec<String> = ws
        .repo_list()
        .iter()
        .filter_map(|r| repo_name(&ws.root, r))
        .collect();
    repos.sort();
    repos.dedup();
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
                default: c.default,
            })
            .collect(),
        mcp: ws
            .mcp
            .iter()
            .map(|m| WorkspaceMcp {
                name: m.name.clone(),
                default: m.default,
            })
            .collect(),
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
                default: true,
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
                default: false,
            }],
        }
    }

    #[test]
    fn a_workspace_shows_names_and_choices_but_nothing_secret() {
        let root = std::env::temp_dir().join(format!("muxa-ws-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for repo in ["app", "group/lib"] {
            std::fs::create_dir_all(root.join(repo).join(".git")).unwrap();
        }
        let root = root.canonicalize().unwrap();
        let shown = to_v2(&workspace(&root), "codex");
        assert_eq!(shown.repos, ["app", "group/lib", "tools"]);
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
                default: false
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
        let shown = to_v2(&plain, "codex --full-auto");
        assert_eq!(
            (shown.agent.as_deref(), shown.task_loop),
            (Some("codex"), None)
        );
        std::fs::remove_dir_all(&root).unwrap();
    }
}
