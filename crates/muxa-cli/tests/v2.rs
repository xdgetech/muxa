//! `muxa-cli v2` end to end: the binary, a relay like `muxa-bridge connect`,
//! and the real server on the fake backend.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::Duration;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Launch, SessionBackend, TerminalId};
use muxa_inbox::NewItem;
use muxa_protocol::settings::{Settings, SettingsOp};
use muxa_protocol::v2::{
    ChatUpdate, ItemKind, LoopChecks, Place, PlaceKind, ReaderContent, ReaderEntry, ReaderKind,
    ReaderList, RoutineRef, UsedBy, Workspace, WorkspaceConnection, WorkspaceLoop, WorkspaceMcp,
    WorktreeMode,
};
use muxa_server::memory_settings::MemorySettings;
use muxa_server::{
    Config, Host, Inbox, PARK, Prepared, ReaderOpen, Services, Sessions, TaskError, TaskSpec, serve,
};
use serde_json::Value;
use tokio::net::UnixListener;

/// Copies stdin to the socket and the socket to stdout, as the bridge does.
const RELAY: &str = r#"
import socket, sys, threading
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
def up():
    while True:
        b = sys.stdin.buffer.read1(65536)
        if not b:
            break
        s.sendall(b)
    s.shutdown(socket.SHUT_WR)
threading.Thread(target=up, daemon=True).start()
while True:
    b = s.recv(65536)
    if not b:
        break
    sys.stdout.buffer.write(b)
    sys.stdout.buffer.flush()
"#;

/// Agents in `/work/<branch>`; a worktree under `/work/dirty` holds work.
struct WorkTasks {
    settings: MemorySettings,
}

/// One Workspace whose `github` connection has its secret.
fn start_settings() -> Settings {
    serde_json::from_value(serde_json::json!({
        "revision": "1", "read_only": false, "problem": null, "workers": [],
        "workspaces": [{"name": "w", "root": "~/w", "repos": [], "park_after": null,
            "agent": {"command": null, "env": [{"key": "CLAUDE_CONFIG_DIR", "set": true}]},
            "loop": null, "routines_enabled": false, "direct_writes": false,
            "connections": [{"name": "github", "env": "GH_TOKEN", "account": null,
                "access": null, "default": false, "secret": "set"}],
            "mcp": [], "routines": [], "routines_problem": null}]
    }))
    .unwrap()
}

impl Host for WorkTasks {
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
            checkout: None,
            argv: vec!["agent".into()],
            env: Default::default(),
            places: vec![Place {
                kind: PlaceKind::Branch,
                path: format!("/work/{branch}"),
                branch: Some(branch.to_owned()),
            }],
            connections: spec.connections.clone(),
            mcp: spec.mcp.clone(),
        })
    }

    async fn workspace_list(&self) -> Result<Vec<Workspace>, TaskError> {
        Ok(vec![Workspace {
            name: "w".into(),
            root: "/work".into(),
            repos: vec!["app".into()],
            agent: Some("claude".into()),
            task_loop: Some(WorkspaceLoop {
                checks: LoopChecks::Any,
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

    async fn check_close(&self, places: &[Place], mode: WorktreeMode) -> Result<(), TaskError> {
        if mode == WorktreeMode::Remove && places.iter().any(|p| p.path == "/work/dirty") {
            return Err(TaskError::WouldLoseWork(
                "dirty has uncommitted changes".into(),
            ));
        }
        Ok(())
    }

    async fn run_routine<B: SessionBackend>(
        &self,
        _backend: &B,
        _workspace: &str,
        _name: &str,
    ) -> Result<(), TaskError> {
        Ok(())
    }

    async fn chat_open(&self, _dir: &Path) -> Result<(u64, ChatUpdate), TaskError> {
        let entry = |id: &str, kind, text: &str| muxa_protocol::ChatEntry {
            id: id.into(),
            kind,
            text: text.into(),
            tool: None,
        };
        Ok((
            1,
            ChatUpdate {
                reset: true,
                entries: vec![
                    entry("u1", muxa_protocol::ChatKind::User, "Clean it"),
                    entry("a1", muxa_protocol::ChatKind::Assistant, "Done."),
                ],
                ..ChatUpdate::default()
            },
        ))
    }

    async fn chat_poll(&self, _handle: u64) -> Result<Option<ChatUpdate>, TaskError> {
        Ok(None)
    }

    /// The Markdown files in `dir`, with a title.
    async fn reader_list(
        &self,
        dir: &Path,
        _kind: ReaderKind,
        _html: bool,
    ) -> Result<ReaderList, TaskError> {
        let mut entries: Vec<ReaderEntry> = std::fs::read_dir(dir)
            .map_err(|e| TaskError::Failed(e.to_string()))?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.ends_with(".md"))
            .map(|path| ReaderEntry {
                title: Some(format!("About {path}")),
                path,
                ..ReaderEntry::default()
            })
            .collect();
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(ReaderList {
            entries,
            ..ReaderList::default()
        })
    }

    async fn reader_read(&self, dir: &Path, open: &ReaderOpen) -> Result<ReaderContent, TaskError> {
        let text = std::fs::read_to_string(dir.join(&open.path))
            .map_err(|_| TaskError::Invalid(format!("{} is gone", open.path)))?;
        Ok(ReaderContent {
            path: open.path.clone(),
            text: Some(text),
            ..ReaderContent::default()
        })
    }
}

struct TestHost {
    dir: PathBuf,
    backend: FakeBackend,
    inbox: Arc<Inbox>,
}

impl TestHost {
    async fn start(name: &str) -> TestHost {
        let dir = std::env::temp_dir().join(format!("muxa-cli-v2-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("relay.py"), RELAY).unwrap();
        let listener = UnixListener::bind(dir.join("s")).unwrap();
        let backend = FakeBackend::new();
        let served = backend.clone();
        let sessions = Arc::new(Sessions::new());
        let inbox = Arc::new(Inbox::memory());
        let shared = Arc::clone(&inbox);
        let host = Arc::new(WorkTasks {
            settings: MemorySettings::new(start_settings()),
        });
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let (backend, sessions) = (served.clone(), Arc::clone(&sessions));
                let (inbox, host) = (Arc::clone(&inbox), Arc::clone(&host));
                tokio::spawn(async move {
                    let (r, w) = stream.into_split();
                    let config = Config {
                        server: "test".into(),
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
        });
        TestHost {
            dir,
            backend,
            inbox: shared,
        }
    }

    /// Runs `muxa-cli v2 <args>` without a terminal; it reads no keys.
    async fn cli(&self, args: &[&str]) -> Output {
        let bridge = format!(
            "python3 -u {} {}",
            self.dir.join("relay.py").display(),
            self.dir.join("s").display()
        );
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_muxa-cli"));
        cmd.args(["v2", "--bridge", &bridge])
            .args(args)
            .stdin(Stdio::null());
        tokio::task::spawn_blocking(move || cmd.output().unwrap())
            .await
            .unwrap()
    }

    async fn terminal(&self) -> TerminalId {
        self.backend.run(&Launch::default()).await.unwrap().id
    }

    /// A Task's agent on `branch` with a Review item; the agent and the item.
    async fn review(&self, branch: &str) -> (String, String) {
        let place = format!("/r:{branch}");
        let started: Value = serde_json::from_str(&stdout(
            &self
                .cli(&[
                    "start-task",
                    "--workspace",
                    "w",
                    "--title",
                    branch,
                    "--branch",
                    &place,
                    "--json",
                ])
                .await,
        ))
        .unwrap();
        let agent = started["terminal"]["id"].as_str().unwrap().to_owned();
        let row = stdout(&self.cli(&["review", &agent]).await);
        let item = row.split('\t').next().unwrap().to_owned();
        (agent, item)
    }
}

impl Drop for TestHost {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn terminals_and_start_task_through_the_relay() {
    let host = TestHost::start("tasks").await;
    let started = stdout(
        &host
            .cli(&[
                "start-task",
                "--workspace",
                "w",
                "--title",
                "fix",
                "--branch",
                "/r:fix",
                "--json",
            ])
            .await,
    );
    let reply: Value = serde_json::from_str(&started).unwrap();
    assert_eq!(reply["existing"], false);
    let again = stdout(
        &host
            .cli(&[
                "start-task",
                "--workspace",
                "w",
                "--title",
                "fix",
                "--branch",
                "/r:fix",
            ])
            .await,
    );
    assert!(again.starts_with("running "), "{again}");
    assert!(again.trim_end().ends_with("/work/fix"), "{again}");

    let list: Value =
        serde_json::from_str(&stdout(&host.cli(&["terminals", "--json"]).await)).unwrap();
    assert_eq!(list[0]["labels"]["muxa.branch"], "fix");
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_prints_the_screen_and_new_output() {
    let host = TestHost::start("attach").await;
    let id = host.terminal().await;
    host.backend.emit(&id, b"first ");
    let backend = host.backend.clone();
    let later = id.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        backend.emit(&later, b"second");
    });
    let out = stdout(&host.cli(&["attach", id.as_str(), "--seconds", "1"]).await);
    assert!(out.ends_with("first second"), "{out:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_prints_state_changes() {
    let host = TestHost::start("watch").await;
    let id = host.terminal().await;
    let backend = host.backend.clone();
    let changed = id.clone();
    // Again and again: the watch starts when the relay is up, not at once.
    let reporting = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            backend.set_status(&changed, "blocked");
        }
    });
    let out = stdout(&host.cli(&["watch", "--seconds", "2"]).await);
    reporting.abort();
    let first: Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
    assert_eq!(
        (&first["type"], &first["terminal"]["status"]["state"]),
        (&Value::from("changed"), &Value::from("blocked"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn capture_lists_and_finishes_an_item() {
    let host = TestHost::start("inbox").await;
    let added = stdout(
        &host
            .cli(&["capture", "read", "the", "RFC", "due", "2030-01-02"])
            .await,
    );
    let row = added.lines().next().unwrap().to_owned();
    assert!(row.ends_with("\tread\tthe RFC\tdue 2030-01-02"), "{row:?}");
    let id = row.split('\t').next().unwrap().to_owned();
    host.cli(&["capture", "buy", "milk"]).await;
    let list = stdout(&host.cli(&["inbox"]).await);
    let titles: Vec<&str> = list
        .lines()
        .map(|l| l.split('\t').nth(2).unwrap())
        .collect();
    assert_eq!(titles, ["buy milk", "the RFC"]);
    let after: Value =
        serde_json::from_str(&stdout(&host.cli(&["inbox", "done", &id, "--json"]).await)).unwrap();
    assert_eq!(after["done"][0]["id"], id.as_str());
    let refused = host.cli(&["inbox", "later", &id]).await;
    assert!(!refused.status.success());
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_says_its_task_is_ready_for_review() {
    let host = TestHost::start("review").await;
    let started: Value = serde_json::from_str(&stdout(
        &host
            .cli(&[
                "start-task",
                "--workspace",
                "w",
                "--title",
                "fix",
                "--branch",
                "/r:fix",
                "--json",
            ])
            .await,
    ))
    .unwrap();
    let agent = started["terminal"]["id"].as_str().unwrap();
    let out = stdout(&host.cli(&["review", agent, "--note", "draft PR #7"]).await);
    assert!(out.ends_with("\treview\tReview fix\n"), "{out:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn reader_lists_and_follows_a_tasks_file() {
    let host = TestHost::start("reader").await;
    let folder = host.dir.join("task");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("PLAN.md"), "# one\n").unwrap();
    let agent = host
        .backend
        .run(&Launch {
            labels: [("muxa.dir".to_owned(), folder.display().to_string())].into(),
            ..Launch::default()
        })
        .await
        .unwrap()
        .id;
    let listed = stdout(&host.cli(&["reader", agent.as_str(), "docs"]).await);
    assert_eq!(listed, "PLAN.md  About PLAN.md\n");
    let changer = {
        let file = folder.join("PLAN.md");
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            std::fs::write(file, "# two\n").unwrap();
        })
    };
    let read = stdout(
        &host
            .cli(&["read", agent.as_str(), "docs", "PLAN.md", "--seconds", "5"])
            .await,
    );
    changer.await.unwrap();
    assert_eq!(read, "# one\n# two\n");
    let shell = host.terminal().await;
    let refused = host.cli(&["reader", shell.as_str(), "docs"]).await;
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("no folder"),
        "{refused:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn chat_prints_a_tasks_conversation() {
    let host = TestHost::start("chat").await;
    let out = host
        .cli(&[
            "start-task",
            "--workspace",
            "w",
            "--title",
            "fix",
            "--connection",
            "github",
            "--json",
        ])
        .await;
    let note = String::from_utf8_lossy(&out.stderr);
    assert!(
        note.contains("--connection and --mcp are deprecated"),
        "{note}"
    );
    let started: Value = serde_json::from_str(&stdout(&out)).unwrap();
    let agent = started["terminal"]["id"].as_str().unwrap();
    let out = stdout(&host.cli(&["chat", agent]).await);
    assert_eq!(out, "user: Clean it\nassistant: Done.\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn workspaces_lists_what_a_task_may_use_and_who_uses_it() {
    let host = TestHost::start("workspaces").await;
    let rows = stdout(&host.cli(&["workspaces"]).await);
    assert_eq!(
        rows,
        "w\t/work\tclaude\trepos app\tloop any\tconnections github\n"
    );
    let started: Value = serde_json::from_str(&stdout(
        &host
            .cli(&["start-task", "--workspace", "w", "--title", "fix", "--json"])
            .await,
    ))
    .unwrap();
    let agent = started["terminal"]["id"].as_str().unwrap();
    let listed: Value =
        serde_json::from_str(&stdout(&host.cli(&["workspaces", "--json"]).await)).unwrap();
    // Every Task agent of the Workspace uses every connection and server (ADR 0075).
    assert_eq!(
        listed[0]["connections"][0],
        serde_json::json!({"name": "github", "default": true,
            "used_by": {"routines": ["digest"], "tasks": [agent]}, "last_used": 1_700_000_000})
    );
    assert_eq!(
        listed[0]["mcp"][0]["used_by"],
        serde_json::json!({"routines": [], "tasks": [agent]})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn approve_closes_a_task_and_asks_for_force_when_work_would_be_lost() {
    let host = TestHost::start("approve").await;
    let (agent, item) = host.review("dirty").await;
    let refused = host.cli(&["approve", &item]).await;
    assert!(!refused.status.success());
    let why = String::from_utf8_lossy(&refused.stderr);
    assert!(why.contains("dirty has uncommitted changes"), "{why}");
    let agent = TerminalId::new(&agent);
    assert!(host.backend.get(&agent).await.unwrap().live);

    let forced: Value = serde_json::from_str(&stdout(
        &host.cli(&["approve", &item, "--force", "--json"]).await,
    ))
    .unwrap();
    assert_eq!(forced["done"][0]["id"], item.as_str(), "{forced}");
    assert!(host.backend.get(&agent).await.is_err(), "the agent is gone");

    let (_, clean) = host.review("clean").await;
    let rows = stdout(&host.cli(&["approve", &clean]).await);
    assert_eq!(rows, "", "no open item is left");
}

#[tokio::test(flavor = "multi_thread")]
async fn request_changes_types_the_comment_into_the_agent() {
    let host = TestHost::start("changes").await;
    let (agent, item) = host.review("fix").await;
    let after: Value = serde_json::from_str(&stdout(
        &host
            .cli(&["request-changes", &item, "Rename", "it.", "--json"])
            .await,
    ))
    .unwrap();
    assert_eq!(after["done"][0]["id"], item.as_str(), "{after}");
    let typed = host.backend.inputs(&TerminalId::new(&agent)).concat();
    assert_eq!(typed, b"Rename it.\r");
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_runs_a_failed_routine_again() {
    let host = TestHost::start("retry").await;
    let failed = NewItem {
        kind: ItemKind::Failed,
        title: "digest failed".into(),
        note: None,
        workspace: None,
        due: None,
        source: "routine".into(),
        key: None,
        once: false,
        terminal: None,
        routine: Some(RoutineRef {
            workspace: "w".into(),
            name: "digest".into(),
        }),
        place: None,
    };
    let item = host
        .inbox
        .change(|b, now| b.add(failed, now))
        .unwrap()
        .unwrap();
    let after: Value =
        serde_json::from_str(&stdout(&host.cli(&["retry", &item, "--json"]).await)).unwrap();
    assert_eq!(after["done"][0]["id"], item.as_str(), "{after}");
    let again = host.cli(&["retry", &item]).await;
    assert!(!again.status.success(), "a done item is not retried twice");
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_and_settings_set_script_the_ops() {
    let host = TestHost::start("settings").await;
    let shown = stdout(&host.cli(&["settings"]).await);
    assert!(shown.starts_with("revision 1\n"), "{shown}");
    assert!(
        shown.contains("w\t~/w\tenv:CLAUDE_CONFIG_DIR\tconnections:github(set)"),
        "{shown}"
    );

    let ops = host.dir.join("ops.json");
    std::fs::write(
        &ops,
        r#"[{"op":"env-set","workspace":"w","key":"API_TOKEN","value":"v4lue"},
            {"op":"connection-add","workspace":"w","connection":{"name":"linear","env":"LINEAR_API_KEY"},"secret":"s3cret"}]"#,
    )
    .unwrap();
    let out = stdout(
        &host
            .cli(&[
                "settings-set",
                ops.to_str().unwrap(),
                "--revision",
                "1",
                "--json",
            ])
            .await,
    );
    assert!(!out.contains("v4lue") && !out.contains("s3cret"), "{out}");
    let after: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(after["revision"], "2");
    assert_eq!(
        after["workspaces"][0]["agent"]["env"][0]["key"],
        "API_TOKEN"
    );
    assert_eq!(after["workspaces"][0]["connections"][1]["secret"], "set");

    let stale = host
        .cli(&["settings-set", ops.to_str().unwrap(), "--revision", "1"])
        .await;
    assert!(!stale.status.success());
    let err = String::from_utf8_lossy(&stale.stderr);
    assert!(
        err.contains("(conflict)") && !err.contains("s3cret"),
        "{err}"
    );

    std::fs::write(
        &ops,
        r#"{"ops":[{"op":"workspace-remove","workspace":"nope"}]}"#,
    )
    .unwrap();
    let bad = host.cli(&["settings-set", ops.to_str().unwrap()]).await;
    let err = String::from_utf8_lossy(&bad.stderr);
    assert!(
        err.contains("ops[0] workspace-remove:") && err.contains("(bad-request)"),
        "{err}"
    );
}
