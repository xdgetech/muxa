//! The host's side of the v2 server: a Task's places (`places.rs`, ADR 0068),
//! the Workspace's agent command and environment (ADR 0016), and the
//! Workspace names for Capture.

use std::sync::Arc;

use muxa_server::{Host, Prepared, RoutineList, TaskError, TaskSpec};

use super::chat::Chats;
use super::mcp;
use super::places;
use super::runner::Runner;
use super::task_loop::TaskLoops;
use crate::records::workers::LaunchCommand;
use crate::routines::runs::{StartError, When};

pub struct BridgeHost {
    /// `--task-command`: the agent when the Workspace names none.
    command: String,
    records: Arc<crate::config::Config>,
    /// Runs Routines on the session backend, for Run now.
    runner: Option<Arc<Runner>>,
    loops: Option<Arc<TaskLoops>>,
    chats: Option<Arc<Chats>>,
}

impl BridgeHost {
    pub fn new(command: String, records: Arc<crate::config::Config>) -> Self {
        BridgeHost {
            command,
            records,
            runner: None,
            loops: None,
            chats: None,
        }
    }

    fn task_loops(&self) -> Result<&TaskLoops, TaskError> {
        self.loops
            .as_deref()
            .ok_or_else(|| TaskError::Invalid("this host runs no Task Loops".into()))
    }

    pub fn with_chats(self, chats: Arc<Chats>) -> Self {
        BridgeHost {
            chats: Some(chats),
            ..self
        }
    }

    pub fn with_loops(self, loops: Arc<TaskLoops>) -> Self {
        BridgeHost {
            loops: Some(loops),
            ..self
        }
    }

    pub fn with_runner(self, runner: Arc<Runner>) -> Self {
        BridgeHost {
            runner: Some(runner),
            ..self
        }
    }
}

impl Host for BridgeHost {
    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        let title = spec.title.trim();
        if title.is_empty() {
            return Err(TaskError::Invalid("a Task needs a title".into()));
        }
        let records = Arc::clone(&self.records);
        let ws = tokio::task::spawn_blocking(move || records.load_workspaces())
            .await
            .map_err(|e| TaskError::Failed(e.to_string()))?
            .map_err(TaskError::Failed)?
            .get(&spec.workspace)
            .cloned()
            .ok_or_else(|| {
                TaskError::Invalid(format!("no Workspace named {:?}", spec.workspace))
            })?;
        let checked = places::check(&ws, &spec.places).await?;
        let command = ws
            .agent
            .as_ref()
            .and_then(|a| a.command.clone())
            .unwrap_or_else(|| self.command.clone());
        let launch = LaunchCommand::Shell(command.clone());
        let claude = crate::launch::is_claude_code(&launch);
        let servers = mcp::chosen(&ws, &spec.mcp).map_err(TaskError::Invalid)?;
        if !claude {
            mcp::only_claude(crate::launch::program(&launch), &servers)
                .map_err(TaskError::Invalid)?;
        }
        let managed = mcp::managed(&ws);
        let named = mcp::with_connections(&spec.connections, &servers);
        crate::connections::chosen(&ws, &named).map_err(TaskError::Invalid)?;
        // Before any place is made: a missing secret stops the start.
        let ws_owned = ws.clone();
        let given = tokio::task::spawn_blocking(move || crate::connections::env(&ws_owned, &named))
            .await
            .map_err(|e| TaskError::Failed(e.to_string()))?
            .map_err(TaskError::Failed)?;
        let made = places::make(&ws, title, checked).await?;
        let (title_owned, goal, known) =
            (title.to_owned(), spec.goal.clone(), ws.connections.clone());
        let (made, flags) = tokio::task::spawn_blocking(move || {
            places::brief(&made, &title_owned, goal.as_deref())?;
            let mut flags = Vec::new();
            if claude {
                flags.extend(places::claude_settings(&made)?);
                if managed {
                    let config = mcp::write(made.working(), &servers, &known)?;
                    flags.push(mcp::shell_flags(&config));
                    let names: Vec<&str> = servers.iter().map(|s| s.name.as_str()).collect();
                    tracing::info!(mcp = ?names, "mcp servers given");
                }
            }
            Ok::<_, String>((made, flags))
        })
        .await
        .map_err(|e| TaskError::Failed(e.to_string()))?
        .map_err(TaskError::Failed)?;
        let mut line = std::iter::once(command)
            .chain(flags)
            .collect::<Vec<_>>()
            .join(" ");
        if spec.plan {
            line = crate::launch::plan_line(&line);
        }
        if let Some(a) = &ws.agent {
            // Keys only: values may be tokens.
            tracing::info!(keys = ?a.keys(), "agent environment");
        }
        tracing::info!(workspace = %ws.name, places = made.places.len(), "task places ready");
        Ok(Prepared {
            dir: made.working().to_owned(),
            workspace: ws.name.clone(),
            title: title.to_owned(),
            repo: made.repo.clone(),
            branch: made.branch.clone(),
            places: made.places,
            argv: shell_line(line),
            env: {
                let mut env = ws.agent.map(|a| a.env).unwrap_or_default();
                crate::connections::merge(&mut env, given);
                env.into_iter().collect()
            },
        })
    }

    async fn routines(&self) -> Result<RoutineList, TaskError> {
        let records = Arc::clone(&self.records);
        let listed = tokio::task::spawn_blocking(move || crate::routines::list(&records))
            .await
            .map_err(|e| TaskError::Failed(e.to_string()))?;
        let (routines, problems) = listed.map_err(TaskError::Failed)?;
        Ok(RoutineList {
            routines: routines.into_iter().map(super::routines::to_v2).collect(),
            problems: problems.into_iter().map(super::routines::problem).collect(),
        })
    }

    async fn run_routine<B: muxa_backend::SessionBackend>(
        &self,
        backend: &B,
        workspace: &str,
        name: &str,
    ) -> Result<(), TaskError> {
        let runner = self
            .runner
            .as_ref()
            .ok_or_else(|| TaskError::Invalid("this host runs no Routines".into()))?;
        runner
            .start(backend, workspace, name, When::Now(crate::records::now()))
            .await
            .map(drop)
            .map_err(|e| match e {
                StartError::Bad(m) | StartError::Unsupported(m) => TaskError::Invalid(m),
                StartError::Busy(m) => TaskError::Busy(m),
                StartError::Handled => TaskError::Busy(e.to_string()),
                StartError::Failed(m) => TaskError::Failed(m),
            })
    }

    async fn loop_check(
        &self,
        workspace: &str,
        spec: &muxa_protocol::v2::LoopSpec,
    ) -> Result<(), TaskError> {
        self.task_loops()?.resolve(workspace, spec).await.map(drop)
    }

    async fn loop_start(
        &self,
        agent: &muxa_backend::TerminalInfo,
        spec: &muxa_protocol::v2::LoopSpec,
        reset: bool,
    ) -> Result<muxa_protocol::v2::LoopInfo, TaskError> {
        self.task_loops()?.start(agent, spec, reset).await
    }

    async fn loop_stop(&self, terminal: &str) -> Result<muxa_protocol::v2::LoopInfo, TaskError> {
        self.task_loops()?.stop(terminal).await
    }

    async fn loops(&self) -> Vec<muxa_protocol::v2::LoopInfo> {
        match &self.loops {
            Some(l) => l.list().await,
            None => Vec::new(),
        }
    }

    fn loop_changes(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        self.loops.as_ref().map(|l| l.subscribe())
    }

    async fn chat_open(
        &self,
        dir: &std::path::Path,
    ) -> Result<(u64, muxa_protocol::v2::ChatUpdate), TaskError> {
        match &self.chats {
            Some(c) => c.open(dir).await,
            None => Err(TaskError::Invalid("this host has no Chat".into())),
        }
    }

    async fn chat_poll(
        &self,
        handle: u64,
    ) -> Result<Option<muxa_protocol::v2::ChatUpdate>, TaskError> {
        match &self.chats {
            Some(c) => c.poll(handle).await,
            None => Err(TaskError::Invalid("this host has no Chat".into())),
        }
    }

    async fn chat_close(&self, handle: u64) {
        if let Some(c) = &self.chats {
            c.close(handle);
        }
    }

    async fn reader_list(
        &self,
        dir: &std::path::Path,
        kind: muxa_protocol::v2::ReaderKind,
        html: bool,
    ) -> Result<muxa_protocol::v2::ReaderList, TaskError> {
        super::reader::list(dir, kind, html).await
    }

    async fn reader_read(
        &self,
        dir: &std::path::Path,
        open: &muxa_server::ReaderOpen,
    ) -> Result<muxa_protocol::v2::ReaderContent, TaskError> {
        super::reader::read(dir, open).await
    }

    async fn workspaces(&self) -> Vec<String> {
        let records = Arc::clone(&self.records);
        let loaded = tokio::task::spawn_blocking(move || records.load_workspaces()).await;
        match loaded {
            Ok(Ok(w)) => w.all().iter().map(|w| w.name.clone()).collect(),
            Ok(Err(_)) | Err(_) => Vec::new(),
        }
    }
}

/// The agent command is a shell line, as tmux ran it.
fn shell_line(line: String) -> Vec<String> {
    vec!["/bin/sh".into(), "-c".into(), line]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_workspace_that_is_not_there_is_invalid() {
        let host = BridgeHost::new(
            "claude".into(),
            Arc::new(crate::config::Config {
                state_dir: None,
                workspaces: None,
                home: None,
            }),
        );
        let spec = TaskSpec {
            workspace: "nowhere".into(),
            title: "Fix".into(),
            goal: None,
            places: Vec::new(),
            connections: Vec::new(),
            mcp: Vec::new(),
            plan: false,
        };
        assert!(matches!(
            host.prepare(&spec).await,
            Err(TaskError::Invalid(_))
        ));
    }

    /// A host whose one Workspace `w` has `extra` in its table.
    fn host(name: &str, extra: &str) -> (BridgeHost, std::path::PathBuf) {
        let base = std::env::temp_dir().join(format!("muxa-tasks-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root")).unwrap();
        let base = base.canonicalize().unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n{extra}\
                 [[workspace.mcp]]\nname = \"docs\"\ncommand = \"docs-mcp\"\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        let config = crate::config::Config {
            state_dir: None,
            workspaces: Some(base.join("workspaces.toml")),
            home: None,
        };
        (BridgeHost::new("claude".into(), Arc::new(config)), base)
    }

    fn spec(mcp: &[&str]) -> TaskSpec {
        TaskSpec {
            workspace: "w".into(),
            title: "Fix".into(),
            goal: None,
            places: Vec::new(),
            connections: Vec::new(),
            mcp: mcp.iter().map(|m| (*m).to_owned()).collect(),
            plan: false,
        }
    }

    #[tokio::test]
    async fn claude_code_gets_only_the_named_mcp_servers_strictly() {
        let (host, base) = host("mcp", "");
        let ready = host.prepare(&spec(&["docs"])).await.unwrap();
        let config = ready.dir.join(".muxa/mcp.json");
        assert_eq!(
            ready.argv[2],
            format!(
                "claude --mcp-config '{}' --strict-mcp-config",
                config.display()
            )
        );
        let text = std::fs::read_to_string(&config).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["mcpServers"]["docs"]["command"], "docs-mcp");
        let unknown = host.prepare(&spec(&["web"])).await;
        assert!(
            matches!(&unknown, Err(TaskError::Invalid(m)) if m.contains("no MCP server \"web\"")),
            "{unknown:?}"
        );
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test]
    async fn a_codex_task_with_mcp_servers_is_refused_before_its_places() {
        let (host, base) = host("codex", "[workspace.agent]\ncommand = \"codex\"\n");
        let refused = host.prepare(&spec(&["docs"])).await;
        assert!(
            matches!(&refused, Err(TaskError::Invalid(m)) if m.contains("need Claude Code")),
            "{refused:?}"
        );
        assert!(!base.join("root/tasks").exists(), "no place was made");
        let ready = host.prepare(&spec(&[])).await.unwrap();
        assert_eq!(ready.argv[2], "codex");
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn the_command_runs_through_the_shell() {
        assert_eq!(
            shell_line("claude --x".into()),
            vec!["/bin/sh", "-c", "claude --x"]
        );
    }
}
