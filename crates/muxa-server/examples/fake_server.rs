//! The v2 server on the fake backend at a Unix socket, for client tests in
//! other languages (`make test-apple-v2`). One terminal, `demo`, shows
//! `hello ` and echoes input as `echo:<bytes>`. Its folder's Reader has one
//! document, one changed file and one image, all fixed. A Task Loop with the
//! check `make test` starts and stops, and runs nothing. A Task agent's Chat
//! has one fixed exchange and waits on a permission. `workspaces` lists one
//! fixed Workspace, `Personal`, whose `github` connection the Routine
//! `digest` uses and the live Tasks get by default. Approve closes a Task
//! with no worktree check. `settings` starts as the fixed view of
//! [`start_settings`], revision `1`, and `settings-update` applies ops in
//! memory (`muxa_server::memory_settings`).
//!
//! Usage: `fake_server SOCKET`. It runs until killed.

use std::path::Path;
use std::sync::Arc;

use muxa_backend::TerminalInfo;
use muxa_backend::fake::FakeBackend;
use muxa_backend::{Launch, SessionBackend};
use muxa_protocol::ChangeStatus;
use muxa_protocol::settings::{
    Access, ConnectionSpec, EnvKey, McpSpec, RoutineSpec, SecretState, Settings, SettingsAgent,
    SettingsConnection, SettingsLoop, SettingsOp, SettingsRoutine, SettingsWorkspace, WorkerSpec,
};
use muxa_protocol::v2::{
    ChatUpdate, LoopChecks, LoopInfo, LoopSpec, ReaderContent, ReaderEntry, ReaderKind, ReaderList,
    UsedBy, Workspace, WorkspaceConnection, WorkspaceLoop, WorkspaceMcp,
};
use muxa_server::memory_settings::MemorySettings;
use muxa_server::{
    Config, Host, Inbox, PARK, Prepared, ReaderOpen, RoutineList, Services, Sessions, TaskError,
    TaskSpec, serve,
};
use tokio::net::UnixListener;

struct FakeHost {
    settings: MemorySettings,
}

impl Host for FakeHost {
    async fn settings(&self) -> Result<Settings, TaskError> {
        Ok(self.settings.view())
    }

    async fn settings_update(
        &self,
        revision: &str,
        ops: Vec<SettingsOp>,
    ) -> Result<Settings, TaskError> {
        self.settings.update(revision, ops)
    }

    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        let (repo, branch) = spec.branch().unwrap_or(("", &spec.title));
        Ok(Prepared {
            dir: format!("/work/{branch}").into(),
            workspace: spec.workspace.clone(),
            title: spec.title.clone(),
            repo: Some(repo.into()),
            branch: Some(branch.to_owned()),
            argv: vec!["agent".into()],
            env: Default::default(),
            places: Vec::new(),
            connections: spec.connections.clone(),
            mcp: spec.mcp.clone(),
        })
    }

    async fn workspaces(&self) -> Vec<String> {
        vec!["Personal".into()]
    }

    async fn workspace_list(&self) -> Result<Vec<Workspace>, TaskError> {
        Ok(vec![Workspace {
            name: "Personal".into(),
            root: "/work".into(),
            repos: vec!["app".into(), "docs".into()],
            agent: Some("claude".into()),
            task_loop: Some(WorkspaceLoop {
                checks: LoopChecks::List(vec!["make test".into()]),
            }),
            connections: vec![WorkspaceConnection {
                name: "github".into(),
                default: true,
                used_by: UsedBy {
                    routines: vec!["digest".into()],
                    tasks: Vec::new(),
                },
                last_used: Some(1_700_000_000),
            }],
            mcp: vec![WorkspaceMcp {
                name: "docs".into(),
                default: true,
                used_by: UsedBy::default(),
                last_used: None,
            }],
        }])
    }

    async fn routines(&self) -> Result<RoutineList, TaskError> {
        let routine = |name: &str, at: Option<&str>| muxa_protocol::v2::Routine {
            workspace: "Personal".into(),
            name: name.into(),
            enabled: true,
            at: at.map(str::to_owned),
            timezone: at.map(|_| "Europe/Berlin".to_owned()),
            schedule: None,
            next_due: None,
            running_since: None,
            last: None,
        };
        Ok(RoutineList {
            routines: vec![routine("digest", Some("09:00")), routine("audit", None)],
            problems: vec![],
        })
    }

    async fn run_routine<B: muxa_backend::SessionBackend>(
        &self,
        _backend: &B,
        _workspace: &str,
        name: &str,
    ) -> Result<(), TaskError> {
        match name {
            "digest" => Err(TaskError::Busy("digest already runs".into())),
            _ => Ok(()),
        }
    }

    async fn loop_check(&self, _workspace: &str, spec: &LoopSpec) -> Result<(), TaskError> {
        match spec.check.as_deref() {
            Some("make test") | None => Ok(()),
            Some(_) => Err(TaskError::Invalid(
                "the workspace allows only make test".into(),
            )),
        }
    }

    async fn loop_start(
        &self,
        agent: &TerminalInfo,
        spec: &LoopSpec,
        _reset: bool,
    ) -> Result<LoopInfo, TaskError> {
        self.loop_check("", spec).await?;
        Ok(fake_loop(agent.id.as_str(), "armed", None))
    }

    async fn loop_stop(&self, terminal: &str) -> Result<LoopInfo, TaskError> {
        Ok(fake_loop(terminal, "stopped", Some("user")))
    }

    async fn chat_open(&self, _dir: &Path) -> Result<(u64, ChatUpdate), TaskError> {
        let entry = |id: &str, kind, text: &str| muxa_protocol::ChatEntry {
            id: id.into(),
            kind,
            text: text.into(),
            tool: None,
        };
        let update = ChatUpdate {
            reset: true,
            entries: vec![
                entry(
                    "u1",
                    muxa_protocol::ChatKind::User,
                    "Clean the build folder",
                ),
                entry("a1", muxa_protocol::ChatKind::Assistant, "Removing it."),
            ],
            truncated: false,
            prompt: Some(muxa_protocol::v2::ChatPrompt {
                entry: "a1".into(),
                kind: muxa_protocol::PromptKind::Permission,
                question: None,
                options: vec![
                    muxa_protocol::v2::ChatOption {
                        label: "Allow".into(),
                        input: "1".into(),
                        cancel: false,
                    },
                    muxa_protocol::v2::ChatOption {
                        label: "Deny".into(),
                        input: "\u{1b}".into(),
                        cancel: true,
                    },
                ],
            }),
        };
        Ok((1, update))
    }

    async fn chat_poll(&self, _handle: u64) -> Result<Option<ChatUpdate>, TaskError> {
        Ok(None)
    }

    async fn reader_list(
        &self,
        _dir: &Path,
        kind: ReaderKind,
        _html: bool,
    ) -> Result<ReaderList, TaskError> {
        let entry = match kind {
            ReaderKind::Docs => ReaderEntry {
                path: "PLAN.md".into(),
                ..ReaderEntry::default()
            },
            ReaderKind::Changes => ReaderEntry {
                path: "a.txt".into(),
                status: Some(ChangeStatus::Modified),
                added: Some(1),
                removed: Some(1),
                ..ReaderEntry::default()
            },
            ReaderKind::Images => ReaderEntry {
                path: "shot.png".into(),
                size: Some(4),
                ..ReaderEntry::default()
            },
        };
        Ok(ReaderList {
            root: Some("demo".into()),
            entries: vec![entry],
            ..ReaderList::default()
        })
    }

    async fn reader_read(
        &self,
        _dir: &Path,
        open: &ReaderOpen,
    ) -> Result<ReaderContent, TaskError> {
        let (text, data) = match (open.kind, open.path.as_str()) {
            (ReaderKind::Docs, "PLAN.md") => (Some("# Plan\n"), None),
            (ReaderKind::Changes, "a.txt") => (Some("@@ -1 +1 @@\n-one\n+two\n"), None),
            (ReaderKind::Images, "shot.png") => (None, Some("iVBORw==")),
            _ => {
                return Err(TaskError::Invalid(format!(
                    "{} is not a listed file",
                    open.path
                )));
            }
        };
        Ok(ReaderContent {
            path: open.path.clone(),
            text: text.map(str::to_owned),
            data: data.map(str::to_owned),
            ..ReaderContent::default()
        })
    }
}

fn fake_loop(terminal: &str, state: &str, reason: Option<&str>) -> LoopInfo {
    LoopInfo {
        terminal: terminal.into(),
        check: "make test".into(),
        max_attempts: 3,
        timeout: 600,
        state: state.into(),
        attempts: 0,
        armed_at: 1,
        since: 1,
        reason: reason.map(str::to_owned),
        last_check: None,
    }
}

/// The settings the client cross tests expect.
fn start_settings() -> Settings {
    Settings {
        revision: "1".into(),
        read_only: false,
        problem: None,
        workers: vec![WorkerSpec {
            name: "reviewer".into(),
            description: Some("Reviews diffs".into()),
            agent: "claude-code".into(),
            args: Vec::new(),
            instructions: None,
            workspaces: None,
            task_loop: None,
        }],
        workspaces: vec![SettingsWorkspace {
            name: "Personal".into(),
            root: "~/personal".into(),
            repos: vec!["~/src/app".into()],
            park_after: None,
            agent: SettingsAgent {
                command: Some("claude".into()),
                env: vec![EnvKey {
                    key: "CLAUDE_CONFIG_DIR".into(),
                    set: true,
                }],
            },
            task_loop: Some(SettingsLoop {
                checks: LoopChecks::List(vec!["make test".into()]),
            }),
            routines_enabled: true,
            direct_writes: false,
            connections: vec![SettingsConnection {
                spec: ConnectionSpec {
                    name: "github".into(),
                    env: "GITHUB_TOKEN".into(),
                    account: Some("me@example.com".into()),
                    access: Some(Access::Read),
                },
                secret: SecretState::Set,
            }],
            mcp: vec![McpSpec {
                name: "docs".into(),
                command: Some("npx".into()),
                args: Some(vec!["-y".into(), "docs-mcp".into()]),
                url: None,
                kind: None,
                env: Default::default(),
                headers: Default::default(),
                connections: Vec::new(),
            }],
            routines: vec![SettingsRoutine {
                spec: RoutineSpec {
                    name: "digest".into(),
                    at: Some("08:00".into()),
                    schedule: None,
                    timezone: Some("Europe/Berlin".into()),
                    worker: None,
                    instructions: "routines/digest.md".into(),
                    project: None,
                    enabled: true,
                    items: None,
                    direct_writes: false,
                },
                instructions_text: Some("Summarize the day.\n".into()),
            }],
            routines_problem: None,
        }],
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("usage: fake_server SOCKET")?;
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    let backend = FakeBackend::new();
    let demo = Launch {
        name: Some("demo".into()),
        labels: [("muxa.dir".to_owned(), "/work/demo".to_owned())].into(),
        ..Launch::default()
    };
    let id = backend.run(&demo).await?.id;
    backend.emit(&id, b"hello ");
    let sessions = Arc::new(Sessions::new());
    let inbox = Arc::new(Inbox::memory());
    let host = Arc::new(FakeHost {
        settings: MemorySettings::new(start_settings()),
    });
    println!("ready");
    loop {
        let (stream, _) = listener.accept().await?;
        let (backend, sessions, inbox, host) = (
            backend.clone(),
            Arc::clone(&sessions),
            Arc::clone(&inbox),
            Arc::clone(&host),
        );
        tokio::spawn(async move {
            let (r, w) = stream.into_split();
            let config = Config {
                server: "muxa fake".into(),
                backend: "fake".into(),
                park: PARK,
            };
            let services = Services {
                backend: &backend,
                host: host.as_ref(),
                sessions: &sessions,
                inbox: &inbox,
            };
            let _ = serve(&services, r, w, &config).await;
        });
    }
}
