//! A protocol v2 client against the server on the fake backend.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Exit, Launch, SessionBackend, TerminalId, TerminalInfo};
use muxa_inbox::NewItem;
use muxa_protocol::v2::{
    ChatUpdate, Frame, INITIAL_CREDIT, ItemKind, Kind, LoopChecks, LoopInfo, LoopSpec, Place,
    PlaceKind, ReaderContent, ReaderEntry, ReaderKind, ReaderList, RoutineRef, UsedBy, V1Status,
    V1Task, Workspace, WorkspaceConnection, WorkspaceLoop, WorkspaceMcp, WorktreeMode,
};
use muxa_server::{
    Config, Host, Inbox, Prepared, ReaderOpen, RoutineList, ServeError, Services, Sessions,
    TaskError, TaskSpec, serve, watch_tasks,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::task::JoinHandle;

/// Worktrees under `/work`; a branch named `bad` is refused.
struct FakeTasks;

impl Host for FakeTasks {
    async fn upload(
        &self,
        dir: &std::path::Path,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<String, TaskError> {
        UPLOADS.with(|u| {
            u.borrow_mut()
                .push((dir.to_owned(), name.to_owned(), bytes))
        });
        Ok(format!(".muxa/uploads/{name}"))
    }

    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        if spec.branch().is_some_and(|(_, b)| b == "bad") {
            return Err(TaskError::Invalid("bad is not a branch".into()));
        }
        if spec.existing() == Some("/work/busy") {
            return Err(TaskError::Busy("claude already works in /work/busy".into()));
        }
        let mut argv = vec!["agent".to_owned()];
        if spec.plan {
            argv.push("--plan".into());
        }
        let (repo, branch) = spec.branch().unwrap_or(("", &spec.title));
        Ok(Prepared {
            dir: format!("/work/{branch}").into(),
            workspace: spec.workspace.clone(),
            title: spec.title.clone(),
            repo: Some(repo.into()),
            branch: Some(branch.to_owned()),
            checkout: spec.checkout().map(str::to_owned),
            argv,
            env: [("K".to_owned(), "v".to_owned())].into(),
            places: Vec::new(),
            // Every agent gets all the Workspace's (ADR 0075); a choice is ignored.
            connections: vec!["github".into()],
            mcp: vec!["docs".into()],
            bot: None,
            subscription: None,
        })
    }

    async fn agent_started(&self, prepared: &Prepared) {
        STARTED.with(|s| s.borrow_mut().push(prepared.title.clone()));
    }

    async fn workspaces(&self) -> Vec<String> {
        vec!["Personal".into()]
    }

    async fn v1_tasks(&self) -> Result<Vec<V1Task>, TaskError> {
        let task = |id: &str, title: &str, status| V1Task {
            id: id.into(),
            workspace: "Personal".into(),
            title: title.into(),
            path: format!("/work/{title}"),
            exists: status != V1Status::Missing,
            status,
        };
        Ok(vec![
            task("t-000001", "Fix login", V1Status::New),
            task("t-000002", "Lost", V1Status::Missing),
            task("t-000003", "Away", V1Status::NoWorkspace),
        ])
    }

    async fn workspace_list(&self) -> Result<Vec<Workspace>, TaskError> {
        Ok(vec![Workspace {
            name: "Personal".into(),
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
            branches: [("app".into(), "main".into())].into(),
        }])
    }

    /// A place under `/work/dirty` holds work.
    async fn check_close(&self, places: &[Place], mode: WorktreeMode) -> Result<(), TaskError> {
        let dirty = places.iter().any(|p| p.path.starts_with("/work/dirty"));
        if dirty && mode == WorktreeMode::Remove {
            return Err(TaskError::WouldLoseWork(
                "dirty has uncommitted changes".into(),
            ));
        }
        Ok(())
    }

    async fn close_places(&self, places: &[Place], mode: WorktreeMode) -> Result<(), TaskError> {
        CLOSED.with(|c| c.borrow_mut().push((places.to_vec(), mode)));
        Ok(())
    }

    async fn restart(&self, ended: &TerminalInfo) -> Result<Prepared, TaskError> {
        let dir = ended.labels.get("muxa.dir").cloned().unwrap_or_default();
        Ok(Prepared {
            dir: dir.into(),
            workspace: "w".into(),
            title: ended.name.clone().unwrap_or_default(),
            argv: vec!["agent".into(), "--again".into()],
            ..Prepared::default()
        })
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
            bot: None,
            on: None,
        };
        Ok(RoutineList {
            routines: vec![routine("digest", Some("09:00")), routine("audit", None)],
            problems: vec![],
            off: vec![muxa_protocol::v2::RoutinesOff {
                workspace: "lab".into(),
                count: 2,
            }],
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
            Some("make test") => Ok(()),
            _ => Err(TaskError::Invalid("that check is not allowed".into())),
        }
    }

    /// Keeps the loop by the agent's own id, so `loops` finds it again.
    async fn loop_start(
        &self,
        agent: &TerminalInfo,
        spec: &LoopSpec,
        reset: bool,
    ) -> Result<LoopInfo, TaskError> {
        self.loop_check("", spec).await?;
        let id = agent.id.as_str().to_owned();
        let info = loop_info(&id, "armed");
        LOOPS.with(|l| l.borrow_mut().insert(id.clone(), info.clone()));
        if reset {
            RESETS.with(|r| r.borrow_mut().push(id));
        }
        bump_loops();
        Ok(info)
    }

    async fn loop_stop(&self, terminal: &str) -> Result<LoopInfo, TaskError> {
        let info = loop_info(terminal, "stopped");
        LOOPS.with(|l| l.borrow_mut().insert(terminal.to_owned(), info.clone()));
        Ok(info)
    }

    async fn loops(&self) -> Vec<LoopInfo> {
        LOOPS.with(|l| l.borrow().values().cloned().collect())
    }

    fn loop_changes(&self) -> Option<tokio::sync::watch::Receiver<u64>> {
        Some(LOOP_CHANGES.with(|c| c.subscribe()))
    }

    /// One user entry and a permission prompt; the agent in `/work/gone` has exited.
    async fn chat_open(&self, dir: &Path) -> Result<(u64, ChatUpdate), TaskError> {
        let handle = if dir.ends_with("gone") { 2 } else { 1 };
        Ok((handle, chat_update()))
    }

    async fn chat_poll(&self, handle: u64) -> Result<Option<ChatUpdate>, TaskError> {
        match handle {
            2 => Err(TaskError::Failed("Claude Code exited".into())),
            _ => Ok(None),
        }
    }

    /// The Markdown files directly in `dir`.
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

fn chat_update() -> ChatUpdate {
    ChatUpdate {
        reset: true,
        entries: vec![muxa_protocol::ChatEntry {
            id: "u1".into(),
            kind: muxa_protocol::ChatKind::User,
            text: "Clean the build".into(),
            tool: None,
        }],
        truncated: false,
        prompt: Some(muxa_protocol::v2::ChatPrompt {
            entry: "t:1".into(),
            kind: muxa_protocol::PromptKind::Permission,
            question: None,
            options: vec![muxa_protocol::v2::ChatOption {
                label: "Allow".into(),
                input: "1".into(),
                cancel: false,
            }],
        }),
    }
}

thread_local! {
    /// Per test: each test's runtime, server included, runs on its own thread.
    static LOOP_CHANGES: tokio::sync::watch::Sender<u64> = tokio::sync::watch::channel(0).0;
    /// What `upload` was asked to write.
    static UPLOADS: std::cell::RefCell<Vec<(std::path::PathBuf, String, Vec<u8>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// What `close_places` was asked to remove.
    static CLOSED: std::cell::RefCell<Vec<(Vec<Place>, WorktreeMode)>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The loops `loop_start` made, by agent terminal id.
    static LOOPS: std::cell::RefCell<std::collections::BTreeMap<String, LoopInfo>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
    /// The agents whose loop `loop_start` started over.
    static RESETS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    /// The titles of the agents `agent_started` heard of.
    static STARTED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn bump_loops() {
    LOOP_CHANGES.with(|c| c.send_modify(|n| *n += 1));
}

fn loop_info(terminal: &str, state: &str) -> LoopInfo {
    LoopInfo {
        terminal: terminal.into(),
        check: "make test".into(),
        max_attempts: 3,
        timeout: 600,
        state: state.into(),
        attempts: 0,
        armed_at: 1,
        since: 1,
        reason: None,
        last_check: None,
    }
}

const PARK: Duration = Duration::from_secs(60);

struct Client {
    io: DuplexStream,
    buf: Vec<u8>,
    server: JoinHandle<Result<(), ServeError>>,
}

impl Client {
    async fn start(backend: &FakeBackend, version: u32) -> Client {
        let hello = json!({"type": "hello", "version": version, "client": "test", "resume": null});
        Client::connect(backend, &Arc::new(Sessions::new()), PARK, hello).await
    }

    async fn connect(
        backend: &FakeBackend,
        sessions: &Arc<Sessions>,
        park: Duration,
        hello: Value,
    ) -> Client {
        Client::connect_with(backend, sessions, &Arc::new(Inbox::memory()), park, hello).await
    }

    async fn connect_with(
        backend: &FakeBackend,
        sessions: &Arc<Sessions>,
        inbox: &Arc<Inbox>,
        park: Duration,
        hello: Value,
    ) -> Client {
        let (client_end, server_end) = tokio::io::duplex(8 << 20);
        let backend = backend.clone();
        let sessions = Arc::clone(sessions);
        let inbox = Arc::clone(inbox);
        let server = tokio::spawn(async move {
            let (r, w) = tokio::io::split(server_end);
            let config = Config {
                server: "muxa test".into(),
                backend: "fake".into(),
                park,
            };
            let services = Services {
                backend: &backend,
                host: &FakeTasks,
                sessions: &sessions,
                inbox: &inbox,
            };
            serve(&services, r, w, &config).await
        });
        let mut client = Client {
            io: client_end,
            buf: Vec::new(),
            server,
        };
        client.control(hello).await;
        client
    }

    /// Ends the connection the way a lost network does, and waits for the
    /// server to notice.
    async fn drop_connection(self) {
        let Client { io, server, .. } = self;
        drop(io);
        server.await.unwrap().unwrap();
    }

    async fn send(&mut self, frame: Frame) {
        self.io.write_all(&frame.encode().unwrap()).await.unwrap();
    }

    async fn control(&mut self, msg: Value) {
        self.send(Frame::new(0, Kind::Data, msg.to_string().into_bytes()))
            .await;
    }

    async fn read(&mut self) -> Frame {
        self.read_for(Duration::from_secs(2)).await
    }

    async fn read_for(&mut self, wait: Duration) -> Frame {
        tokio::time::timeout(wait, async {
            loop {
                if let Some(f) = Frame::decode(&mut self.buf).unwrap() {
                    return f;
                }
                // On the heap: each future that holds it is on the test's stack.
                let mut chunk = vec![0u8; 65536];
                let n = self.io.read(&mut chunk).await.unwrap();
                assert!(n > 0, "server closed the connection");
                self.buf.extend_from_slice(&chunk[..n]);
            }
        })
        .await
        .expect("a frame in time")
    }

    async fn read_control(&mut self) -> Value {
        let f = self.read().await;
        assert_eq!(
            (f.channel, f.kind),
            (0, Kind::Data),
            "expected a control message"
        );
        serde_json::from_slice(&f.payload).unwrap()
    }

    async fn welcome(&mut self) -> Value {
        let w = self.read_control().await;
        assert_eq!(w["type"], "welcome");
        w
    }

    /// Attaches and returns the channel, after the reply and the paint.
    async fn attach(&mut self, id: u64, terminal: &TerminalId) -> u32 {
        self.control(
            json!({"type": "attach", "id": id, "terminal": terminal.as_str(),
                            "cols": 100, "rows": 30, "history": 50}),
        )
        .await;
        let reply = self.read_control().await;
        assert_eq!(
            (reply["type"].as_str(), reply["id"].as_u64()),
            (Some("reply"), Some(id)),
            "{reply}"
        );
        let channel = reply["channel"].as_u64().unwrap() as u32;
        assert_eq!(
            self.read_control().await,
            json!({"type": "paint", "channel": channel})
        );
        channel
    }

    /// Reads output on `channel` until `n` bytes arrived; returns them with
    /// the first offset.
    async fn output(&mut self, channel: u32, n: usize) -> (u64, Vec<u8>) {
        let mut first = None;
        let mut got = Vec::new();
        while got.len() < n {
            let f = self.read().await;
            assert_eq!(
                (f.channel, f.kind),
                (channel, Kind::Data),
                "expected output"
            );
            let (offset, bytes) = f.split_u64().unwrap();
            first.get_or_insert(offset);
            got.extend_from_slice(bytes);
        }
        (first.unwrap(), got)
    }
}

async fn terminal(backend: &FakeBackend) -> TerminalId {
    backend
        .run(&Launch {
            name: Some("one".into()),
            ..Launch::default()
        })
        .await
        .unwrap()
        .id
}

#[tokio::test]
async fn a_client_of_another_version_is_refused() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 1).await;
    let err = client.read_control().await;
    assert_eq!(
        (err["type"].as_str(), err["code"].as_str()),
        (Some("error"), Some("version"))
    );
    assert!(matches!(
        client.server.await.unwrap(),
        Err(ServeError::Version(1))
    ));
}

#[tokio::test]
async fn welcome_then_the_live_terminals_without_screens() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    backend.emit(&id, b"secret screen");
    let mut client = Client::start(&backend, 2).await;
    let welcome = client.welcome().await;
    assert_eq!(welcome["token"].as_str().map(str::len), Some(32));
    client.control(json!({"type": "terminals", "id": 1})).await;
    let reply = client.read_control().await;
    assert_eq!(reply["terminals"][0]["id"], json!(id.as_str()));
    assert_eq!(reply["terminals"][0]["name"], json!("one"));
    assert!(!reply.to_string().contains("secret"));
}

#[tokio::test]
async fn attach_paints_then_streams_output_with_offsets() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    backend.emit(&id, b"hello ");
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let ch = client.attach(1, &id).await;
    assert_eq!(client.output(ch, 6).await, (0, b"hello ".to_vec()));
    backend.emit(&id, b"world");
    assert_eq!(client.output(ch, 5).await, (6, b"world".to_vec()));
    assert_eq!(backend.size(&id).map(|s| (s.cols, s.rows)), Some((100, 30)));
}

#[tokio::test]
async fn input_is_applied_once_per_sequence_number() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let ch = client.attach(1, &id).await;
    for (seq, bytes) in [(1, &b"a"[..]), (1, b"a"), (2, b"b")] {
        client.send(Frame::input(ch, seq, bytes)).await;
    }
    let mut acks = Vec::new();
    while acks.len() < 3 {
        let f = client.read().await;
        if f.kind == Kind::Ack {
            acks.push(f.split_u64().unwrap().0);
        }
    }
    assert_eq!(acks, vec![1, 1, 2]);
    assert_eq!(backend.inputs(&id), vec![b"a".to_vec(), b"b".to_vec()]);
}

#[tokio::test]
async fn output_stops_at_the_credit_and_resumes_with_more() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let ch = client.attach(1, &id).await;
    let credit = INITIAL_CREDIT as usize;
    backend.emit(&id, &vec![b'x'; credit + 1000]);
    let (_, first) = client.output(ch, credit).await;
    assert_eq!(first.len(), credit);
    // Nothing more arrives without credit.
    let more = tokio::time::timeout(Duration::from_millis(200), client.read()).await;
    assert!(more.is_err(), "output past the credit");
    client.send(Frame::credit(ch, 4096)).await;
    assert_eq!(client.output(ch, 1000).await.0, credit as u64);
}

#[tokio::test]
async fn a_killed_program_closes_its_channel_with_the_exit() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let ch = client.attach(1, &id).await;
    client.send(Frame::resize(ch, 120, 40)).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(backend.size(&id).map(|s| (s.cols, s.rows)), Some((120, 40)));
    backend.kill(&id).await.unwrap();
    let close = client.read().await;
    assert_eq!((close.channel, close.kind), (ch, Kind::Close));
    let exit: Value = serde_json::from_slice(&close.payload).unwrap();
    assert_eq!(exit["reason"], "killed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    client
        .control(json!({"type": "detach", "id": 2, "channel": ch}))
        .await;
    assert_eq!(client.read_control().await["code"], "not-found");
}

#[tokio::test]
async fn an_unknown_terminal_is_not_found() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    client
        .control(json!({"type": "attach", "id": 9, "terminal": "nope", "cols": 80, "rows": 24}))
        .await;
    let err = client.read_control().await;
    assert_eq!(
        (err["id"].as_u64(), err["code"].as_str()),
        (Some(9), Some("not-found"))
    );
}

#[tokio::test]
async fn a_client_that_does_not_return_is_detached_and_programs_keep_running() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let sessions = Arc::new(Sessions::new());
    let mut client =
        Client::connect(&backend, &sessions, Duration::from_millis(100), hello(None)).await;
    client.welcome().await;
    client.attach(1, &id).await;
    client.drop_connection().await;
    // Parked: still attached, for a resume.
    assert_eq!(backend.viewers(&id), 1);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(backend.viewers(&id), 0);
    assert!(backend.get(&id).await.unwrap().live);
}

fn hello(resume: Option<Value>) -> Value {
    json!({"type": "hello", "version": 2, "client": "test", "resume": resume})
}

fn resume(token: &Value, channel: u32, id: &TerminalId, offset: u64, input_seq: u64) -> Value {
    json!({"token": token, "channels": [{"channel": channel, "terminal": id.as_str(),
        "offset": offset, "input_seq": input_seq, "cols": 100, "rows": 30}]})
}

/// A client that attached `id` and read its first output, `hello `.
async fn attached(
    backend: &FakeBackend,
    sessions: &Arc<Sessions>,
    id: &TerminalId,
) -> (Client, Value, u32) {
    backend.emit(id, b"hello ");
    let mut client = Client::connect(backend, sessions, PARK, hello(None)).await;
    let token = client.welcome().await["token"].clone();
    let ch = client.attach(1, id).await;
    assert_eq!(client.output(ch, 6).await, (0, b"hello ".to_vec()));
    (client, token, ch)
}

#[tokio::test]
async fn a_resume_sends_only_the_output_the_client_missed() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let sessions = Arc::new(Sessions::new());
    let (client, token, ch) = attached(&backend, &sessions, &id).await;
    client.drop_connection().await;
    backend.emit(&id, b"world");

    // The client lost the last bytes of `hello ` on the way.
    let mut client = Client::connect(
        &backend,
        &sessions,
        PARK,
        hello(Some(resume(&token, ch, &id, 3, 0))),
    )
    .await;
    let welcome = client.welcome().await;
    assert_eq!(welcome["token"], token);
    assert_eq!(welcome["resumed"], json!([{"channel": ch, "input_seq": 0}]));
    assert_eq!(client.output(ch, 8).await, (3, b"lo world".to_vec()));
    // The channel goes on as before.
    backend.emit(&id, b"!");
    assert_eq!(client.output(ch, 1).await, (11, b"!".to_vec()));
    assert_eq!(backend.viewers(&id), 1);
}

#[tokio::test]
async fn input_resent_after_a_resume_is_not_applied_twice() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let sessions = Arc::new(Sessions::new());
    let (mut client, token, ch) = attached(&backend, &sessions, &id).await;
    client.send(Frame::input(ch, 1, b"y")).await;
    // Applied, but the ACK is lost with the connection.
    while backend.inputs(&id).is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    client.drop_connection().await;

    let mut client = Client::connect(
        &backend,
        &sessions,
        PARK,
        hello(Some(resume(&token, ch, &id, 6, 0))),
    )
    .await;
    let welcome = client.welcome().await;
    assert_eq!(welcome["resumed"], json!([{"channel": ch, "input_seq": 1}]));
    client.send(Frame::input(ch, 1, b"y")).await;
    client.send(Frame::input(ch, 2, b"n")).await;
    let mut acks = Vec::new();
    while acks.len() < 2 {
        let f = client.read().await;
        if f.kind == Kind::Ack {
            acks.push(f.split_u64().unwrap().0);
        }
    }
    assert_eq!(acks, vec![1, 2]);
    assert_eq!(backend.inputs(&id), vec![b"y".to_vec(), b"n".to_vec()]);
}

#[tokio::test]
async fn output_past_the_buffer_gets_a_paint_and_keeps_the_input_count() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let sessions = Arc::new(Sessions::new());
    let (mut client, token, ch) = attached(&backend, &sessions, &id).await;
    client.send(Frame::input(ch, 1, b"y")).await;
    while backend.inputs(&id).is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    client.drop_connection().await;
    backend.emit(&id, &vec![b'x'; 300 * 1024]);
    tokio::time::sleep(Duration::from_millis(50)).await;
    // The parked viewer let go when its buffer could no longer cover the client.
    assert_eq!(backend.viewers(&id), 0);

    let mut client = Client::connect(
        &backend,
        &sessions,
        PARK,
        hello(Some(resume(&token, ch, &id, 6, 0))),
    )
    .await;
    let welcome = client.welcome().await;
    assert_eq!(welcome["resumed"], json!([{"channel": ch, "input_seq": 1}]));
    assert_eq!(
        client.read_control().await,
        json!({"type": "paint", "channel": ch})
    );
    let (offset, bytes) = client.output(ch, 6).await;
    assert_eq!((offset, &bytes[..6]), (0, &b"hello "[..]));
    client.send(Frame::input(ch, 1, b"y")).await;
    client.send(Frame::input(ch, 2, b"n")).await;
    while backend.inputs(&id).len() < 2 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(backend.inputs(&id), vec![b"y".to_vec(), b"n".to_vec()]);
}

#[tokio::test]
async fn an_unknown_token_repaints_each_live_terminal_and_drops_the_rest() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    backend.emit(&id, b"screen");
    let sessions = Arc::new(Sessions::new());
    let channels = json!({"token": "00", "channels": [
        {"channel": 4, "terminal": id.as_str(), "offset": 99, "input_seq": 7, "cols": 90, "rows": 20},
        {"channel": 5, "terminal": "gone", "offset": 0, "input_seq": 0, "cols": 90, "rows": 20}]});
    let mut client = Client::connect(&backend, &sessions, PARK, hello(Some(channels))).await;
    let welcome = client.welcome().await;
    assert_ne!(welcome["token"], "00");
    assert_eq!(
        welcome["resumed"],
        json!([{"channel": 4, "input_seq": null}])
    );
    assert_eq!(
        client.read_control().await,
        json!({"type": "paint", "channel": 4})
    );
    assert_eq!(client.output(4, 6).await, (0, b"screen".to_vec()));
    assert_eq!(backend.size(&id).map(|s| (s.cols, s.rows)), Some((90, 20)));
    // Old input is not applied again, and new channels do not reuse 4.
    client.send(Frame::input(4, 7, b"old")).await;
    client.send(Frame::input(4, 8, b"new")).await;
    while backend.inputs(&id).is_empty() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(backend.inputs(&id), vec![b"new".to_vec()]);
    let other = terminal(&backend).await;
    client
        .control(
            json!({"type": "attach", "id": 1, "terminal": other.as_str(), "cols": 80, "rows": 24}),
        )
        .await;
    let reply = loop {
        let f = client.read().await;
        if f.channel == 0 {
            break serde_json::from_slice::<Value>(&f.payload).unwrap();
        }
    };
    assert_eq!(reply["channel"], 6);
}

#[tokio::test]
async fn a_resume_takes_over_a_connection_that_is_still_open() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let sessions = Arc::new(Sessions::new());
    let (mut old, token, ch) = attached(&backend, &sessions, &id).await;
    let mut new = Client::connect(
        &backend,
        &sessions,
        PARK,
        hello(Some(resume(&token, ch, &id, 6, 0))),
    )
    .await;
    let welcome = new.welcome().await;
    assert_eq!(welcome["resumed"], json!([{"channel": ch, "input_seq": 0}]));
    backend.emit(&id, b"x");
    assert_eq!(new.output(ch, 1).await, (6, b"x".to_vec()));
    // The old connection was closed.
    let mut rest = Vec::new();
    let n = tokio::time::timeout(Duration::from_secs(2), old.io.read_to_end(&mut rest)).await;
    assert!(n.is_ok(), "the old connection is still open");
    assert_eq!(backend.viewers(&id), 1);
}

#[tokio::test]
async fn a_program_that_ended_while_parked_closes_after_the_missed_output() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let sessions = Arc::new(Sessions::new());
    let (client, token, ch) = attached(&backend, &sessions, &id).await;
    client.drop_connection().await;
    backend.emit(&id, b"bye");
    backend.kill(&id).await.unwrap();
    let mut client = Client::connect(
        &backend,
        &sessions,
        PARK,
        hello(Some(resume(&token, ch, &id, 6, 0))),
    )
    .await;
    client.welcome().await;
    assert_eq!(client.output(ch, 3).await, (6, b"bye".to_vec()));
    let close = client.read().await;
    assert_eq!((close.channel, close.kind), (ch, Kind::Close));
    let exit: Value = serde_json::from_slice(&close.payload).unwrap();
    assert_eq!(exit["reason"], "killed");
}

#[tokio::test]
async fn start_task_runs_the_agent_in_its_worktree_once() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let start = json!({"type": "start-task", "id": 1, "workspace": "w", "title": "Fix login", "places": [{"kind": "branch", "repo": "/src/app", "branch": "fix"}], "plan": true});
    client.control(start.clone()).await;
    let reply = client.read_control().await;
    assert_eq!(reply["existing"], false, "{reply}");
    let labels = &reply["terminal"]["labels"];
    assert_eq!(
        labels["muxa.title"], "Fix login",
        "the apps show the title as written"
    );
    assert_eq!(
        (
            &labels["muxa.kind"],
            &labels["muxa.role"],
            &labels["muxa.dir"],
            &labels["muxa.branch"]
        ),
        (
            &json!("task"),
            &json!("agent"),
            &json!("/work/fix"),
            &json!("fix")
        )
    );
    assert_eq!(labels["muxa.workspace"], "w");
    assert_eq!(labels["muxa.places"], "[]");
    assert_eq!(
        reply["terminal"]["name"], "Fix login",
        "the title names the terminal; herdr's backend slugs it"
    );
    let launches = backend.launches();
    assert_eq!(launches[0].argv, vec!["agent", "--plan"]);
    assert_eq!(
        launches[0].cwd.as_deref(),
        Some(std::path::Path::new("/work/fix"))
    );
    assert_eq!(launches[0].env.get("K").map(String::as_str), Some("v"));

    // The same Task again finds the running agent.
    client
        .control(json!({"type": "start-task", "id": 2, "workspace": "w", "title": "fix", "places": [{"kind": "branch", "repo": "/src/app", "branch": "fix"}]}))
        .await;
    let again = client.read_control().await;
    assert_eq!(again["existing"], true);
    assert_eq!(again["terminal"]["id"], reply["terminal"]["id"]);
    assert_eq!(backend.launches().len(), 1);

    // Once the agent ended, a start runs a new one.
    let id = TerminalId::new(reply["terminal"]["id"].as_str().unwrap());
    backend.kill(&id).await.unwrap();
    client
        .control(json!({"type": "start-task", "id": 3, "workspace": "w", "title": "fix", "places": [{"kind": "branch", "repo": "/src/app", "branch": "fix"}]}))
        .await;
    assert_eq!(client.read_control().await["existing"], false);
}

#[tokio::test]
async fn migrate_v1_adds_each_adoptable_task_once() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let statuses = |reply: &Value| -> Vec<String> {
        reply["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["status"].as_str().unwrap().to_owned())
            .collect()
    };
    let dry = request(
        &mut client,
        json!({"type": "migrate-v1", "id": 1, "dry_run": true}),
    )
    .await;
    assert_eq!(statuses(&dry), ["new", "missing", "no-workspace"], "{dry}");
    assert_eq!(dry["added"], 0);
    let inbox = request(&mut client, json!({"type": "inbox", "id": 2})).await;
    assert!(titles(&inbox["inbox"]).is_empty());

    let run = request(&mut client, json!({"type": "migrate-v1", "id": 3})).await;
    assert_eq!(statuses(&run), ["added", "missing", "no-workspace"]);
    assert_eq!(run["added"], 1);
    let inbox = request(&mut client, json!({"type": "inbox", "id": 4})).await;
    let item = &inbox["inbox"]["items"][0];
    assert_eq!(item["title"], "Resume: Fix login");
    assert_eq!(item["source"], "v1");
    assert_eq!(
        item["place"],
        json!({"workspace": "Personal", "path": "/work/Fix login"})
    );

    let again = request(&mut client, json!({"type": "migrate-v1", "id": 5})).await;
    assert_eq!(statuses(&again)[0], "in-inbox");
    assert_eq!(again["added"], 0);
}

#[tokio::test]
async fn resume_finds_muxas_agent_in_the_place_before_the_host_refuses_it() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let resume = |id: u64| {
        json!({"type": "start-task", "id": id, "workspace": "w", "title": "Fix",
               "places": [{"kind": "existing", "path": "/work/busy"}]})
    };
    let refused = request(&mut client, resume(1)).await;
    assert_eq!(refused["code"], "conflict", "{refused}");
    let agent = task_agent(&backend, "/work/busy").await;
    let found = request(&mut client, resume(2)).await;
    assert_eq!(
        (found["existing"].clone(), found["terminal"]["id"].clone()),
        (json!(true), json!(agent.as_str()))
    );
    assert_eq!(backend.launches().len(), 1, "only the agent the test ran");
}

#[tokio::test]
async fn a_second_checkout_start_finds_the_running_agent() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let start = |id: u64| {
        json!({"type": "start-task", "id": id, "workspace": "w", "title": "app main",
               "places": [{"kind": "checkout", "repo": "app"}]})
    };
    let first = request(&mut client, start(1)).await;
    assert_eq!(first["existing"], false, "{first}");
    assert_eq!(first["terminal"]["labels"]["muxa.checkout"], "app");
    let again = request(&mut client, start(2)).await;
    assert_eq!(
        (again["existing"].clone(), again["terminal"]["id"].clone()),
        (json!(true), first["terminal"]["id"].clone())
    );
}

#[tokio::test]
async fn a_task_the_host_refuses_is_a_bad_request() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    client
        .control(json!({"type": "start-task", "id": 4, "workspace": "w", "title": "bad", "places": [{"kind": "branch", "repo": "/src/app", "branch": "bad"}]}))
        .await;
    let err = client.read_control().await;
    assert_eq!(
        (err["code"].as_str(), err["message"].as_str()),
        (Some("bad-request"), Some("bad is not a branch"))
    );
    assert!(backend.launches().is_empty());
}

#[tokio::test]
async fn watch_sends_each_change_as_the_terminal_now_is() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    client.control(json!({"type": "watch", "id": 1})).await;
    assert_eq!(
        client.read_control().await,
        json!({"type": "reply", "id": 1})
    );

    let id = terminal(&backend).await;
    let created = client.read_control().await;
    assert_eq!(
        (created["type"].as_str(), created["terminal"]["id"].as_str()),
        (Some("changed"), Some(id.as_str()))
    );
    backend.set_status(&id, "blocked");
    let status = client.read_control().await;
    assert_eq!(
        status["terminal"]["status"],
        json!({"state": "blocked", "app": "fake"})
    );
    backend.kill(&id).await.unwrap();
    assert_eq!(client.read_control().await["terminal"]["live"], false);
    backend.remove(&id).await.unwrap();
    assert_eq!(
        client.read_control().await,
        json!({"type": "removed", "terminal": id.as_str()})
    );
}

async fn request(client: &mut Client, msg: Value) -> Value {
    client.control(msg).await;
    loop {
        let reply = client.read_control().await;
        if reply["type"] == "reply" || reply["type"] == "error" {
            return reply;
        }
    }
}

fn titles(inbox: &Value) -> Vec<&str> {
    inbox["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["title"].as_str().unwrap())
        .collect()
}

#[tokio::test]
async fn a_capture_line_becomes_fields_then_an_item() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let parsed = request(
        &mut client,
        json!({"type": "parse-capture", "id": 1,
        "text": "reply to the bank #personal due fri", "today": "2026-11-25"}),
    )
    .await;
    assert_eq!(
        parsed["capture"],
        json!({"kind": "reply", "title": "to the bank", "workspace": "Personal", "due": "2026-11-27"})
    );
    let mut add = parsed["capture"].clone();
    add["type"] = json!("inbox-add");
    add["id"] = json!(2);
    let added = request(&mut client, add).await;
    let item = &added["inbox"]["items"][0];
    assert_eq!(
        (item["kind"].as_str(), item["source"].as_str()),
        (Some("reply"), Some("capture"))
    );
    assert_eq!(added["inbox"]["revision"], 1);
}

#[tokio::test]
async fn the_user_orders_hides_and_finishes_items() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    for (id, title) in [(1, "a"), (2, "b"), (3, "c")] {
        request(
            &mut client,
            json!({"type": "inbox-add", "id": id, "kind": "todo", "title": title}),
        )
        .await;
    }
    let inbox = request(&mut client, json!({"type": "inbox", "id": 4})).await["inbox"].clone();
    assert_eq!(titles(&inbox), ["c", "b", "a"]);
    let a = inbox["items"][2]["id"].as_str().unwrap().to_owned();
    let moved = request(
        &mut client,
        json!({"type": "inbox-move", "id": 5, "item": a, "index": 0}),
    )
    .await;
    assert_eq!(titles(&moved["inbox"]), ["a", "c", "b"]);
    let later = request(
        &mut client,
        json!({"type": "inbox-later", "id": 6, "item": a, "until": 4_000_000_000_i64}),
    )
    .await;
    assert_eq!(later["inbox"]["items"][0]["later_until"], 4_000_000_000_i64);
    let done = request(
        &mut client,
        json!({"type": "inbox-done", "id": 7, "item": a}),
    )
    .await;
    assert_eq!(
        (
            titles(&done["inbox"]),
            done["inbox"]["done"][0]["id"].as_str()
        ),
        (vec!["c", "b"], Some(a.as_str()))
    );
    let reopened = request(
        &mut client,
        json!({"type": "inbox-reopen", "id": 8, "item": a}),
    )
    .await;
    assert_eq!(titles(&reopened["inbox"]), ["a", "c", "b"]);
    request(
        &mut client,
        json!({"type": "inbox-delete", "id": 9, "item": a}),
    )
    .await;
    let gone = request(
        &mut client,
        json!({"type": "inbox-done", "id": 10, "item": a}),
    )
    .await;
    assert_eq!(gone["code"], "not-found");
    let bad = request(
        &mut client,
        json!({"type": "inbox-add", "id": 11, "kind": "todo", "title": "x", "due": "fri"}),
    )
    .await;
    assert_eq!(bad["code"], "bad-request");
}

#[tokio::test]
async fn a_watching_client_gets_each_inbox_change() {
    let backend = FakeBackend::new();
    let (sessions, inbox) = (Arc::new(Sessions::new()), Arc::new(Inbox::memory()));
    let mut phone = Client::connect_with(&backend, &sessions, &inbox, PARK, hello(None)).await;
    phone.welcome().await;
    request(&mut phone, json!({"type": "watch", "id": 1})).await;
    let mut mac = Client::connect_with(&backend, &sessions, &inbox, PARK, hello(None)).await;
    mac.welcome().await;
    request(
        &mut mac,
        json!({"type": "inbox-add", "id": 1, "kind": "read", "title": "RFC 9000"}),
    )
    .await;
    let note = phone.read_control().await;
    assert_eq!(note["type"], "inbox");
    assert_eq!(titles(&note["inbox"]), ["RFC 9000"]);
}

#[tokio::test]
async fn a_task_ready_for_review_makes_one_review_item() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let started = request(
        &mut client,
        json!({"type": "start-task", "id": 1, "workspace": "w", "title": "fix", "places": [{"kind": "branch", "repo": "/r", "branch": "fix"}]}),
    )
    .await;
    let agent = started["terminal"]["id"].clone();
    for id in [2, 3] {
        let reply = request(
            &mut client,
            json!({"type": "task-review", "id": id, "terminal": agent, "note": "PR #4"}),
        )
        .await;
        assert_eq!(titles(&reply["inbox"]), ["Review fix"], "{reply}");
        assert_eq!(reply["inbox"]["items"][0]["kind"], "review");
    }
    let shell = terminal(&backend).await;
    let refused = request(
        &mut client,
        json!({"type": "task-review", "id": 4, "terminal": shell.as_str()}),
    )
    .await;
    assert_eq!(refused["code"], "bad-request");
}

#[tokio::test]
async fn a_failed_agent_makes_one_failed_item_and_a_kill_none() {
    let backend = FakeBackend::new();
    let inbox = Arc::new(Inbox::memory());
    let run = |branch: &str| Launch {
        labels: [
            ("muxa.kind".to_owned(), "task".to_owned()),
            ("muxa.role".to_owned(), "agent".to_owned()),
            ("muxa.branch".to_owned(), branch.to_owned()),
        ]
        .into(),
        ..Launch::default()
    };
    // Failed before anyone watched.
    let early = backend.run(&run("early")).await.unwrap().id;
    let exit = |code| Exit {
        reason: "exited".into(),
        code: Some(code),
        signal: None,
    };
    backend.end(&early, exit(2)).unwrap();
    let (watched, inbox2) = (backend.clone(), Arc::clone(&inbox));
    tokio::spawn(async move { watch_tasks(&watched, &inbox2).await });
    tokio::time::sleep(Duration::from_millis(50)).await;

    let crash = backend.run(&run("crash")).await.unwrap().id;
    let clean = backend.run(&run("clean")).await.unwrap().id;
    let killed = backend.run(&run("killed")).await.unwrap().id;
    backend.end(&crash, exit(1)).unwrap();
    backend.end(&clean, exit(0)).unwrap();
    backend.kill(&killed).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let view = serde_json::to_value(inbox.view()).unwrap();
    assert_eq!(
        titles(&view),
        ["The agent of crash stopped", "The agent of early stopped"]
    );
    assert_eq!(view["items"][0]["note"], "It exited with code 1.");
    assert_eq!(view["items"][0]["kind"], "failed");
}

#[tokio::test]
async fn routines_are_listed_and_run_now() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let list = request(&mut client, json!({"type": "routines", "id": 1})).await;
    assert_eq!(list["routines"][0]["name"], "digest");
    assert_eq!(list["routines"][0]["at"], "09:00");
    assert!(list["routines"][1].get("at").is_none());
    assert_eq!(
        list["off"],
        json!([{"workspace": "lab", "count": 2}]),
        "a Workspace with its Routines turned off"
    );
    let ran = request(
        &mut client,
        json!({"type": "run-routine", "id": 2, "workspace": "Personal", "name": "audit"}),
    )
    .await;
    assert_eq!(ran, json!({"type": "reply", "id": 2}));
    let busy = request(
        &mut client,
        json!({"type": "run-routine", "id": 3, "workspace": "Personal", "name": "digest"}),
    )
    .await;
    assert_eq!(busy["code"], "conflict");
}

/// Waits for the next `reader` or `reader-ended` notification; the server
/// reads open files every 2 s.
async fn reader_note(client: &mut Client) -> Value {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        assert!(Instant::now() < until, "no reader notification");
        let frame = client.read_for(Duration::from_secs(8)).await;
        let f: Value = serde_json::from_slice(&frame.payload).unwrap();
        if f["type"] == "reader" || f["type"] == "reader-ended" {
            return f;
        }
    }
}

#[tokio::test]
async fn the_reader_follows_a_file_of_the_terminals_folder() {
    let dir = std::env::temp_dir().join(format!("muxa-server-reader-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("PLAN.md"), "# one").unwrap();
    std::fs::write(dir.join("notes.txt"), "no").unwrap();
    let backend = FakeBackend::new();
    let agent = backend
        .run(&Launch {
            labels: [("muxa.dir".to_owned(), dir.display().to_string())].into(),
            ..Launch::default()
        })
        .await
        .unwrap()
        .id;
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let listed = request(
        &mut client,
        json!({"type": "reader-list", "id": 1, "terminal": agent.as_str(), "kind": "docs"}),
    )
    .await;
    assert_eq!(listed["list"], json!({"entries": [{"path": "PLAN.md"}]}));
    let opened = request(
        &mut client,
        json!({"type": "reader-open", "id": 2, "terminal": agent.as_str(), "kind": "docs", "path": "PLAN.md"}),
    )
    .await;
    let reader = opened["reader"].clone();
    assert_eq!(
        opened["content"],
        json!({"path": "PLAN.md", "text": "# one"})
    );
    std::fs::write(dir.join("PLAN.md"), "# two").unwrap();
    let changed = reader_note(&mut client).await;
    assert_eq!(
        changed,
        json!({"type": "reader", "reader": reader, "content": {"path": "PLAN.md", "text": "# two"}})
    );
    std::fs::remove_file(dir.join("PLAN.md")).unwrap();
    let ended = reader_note(&mut client).await;
    assert_eq!(
        (&ended["type"], &ended["reader"]),
        (&json!("reader-ended"), &reader)
    );
    let closed = request(
        &mut client,
        json!({"type": "reader-close", "id": 3, "reader": reader}),
    )
    .await;
    assert_eq!(closed["code"], "not-found", "an ended reader is closed");

    // A terminal muxa did not start has no folder.
    let shell = terminal(&backend).await;
    let refused = request(
        &mut client,
        json!({"type": "reader-list", "id": 4, "terminal": shell.as_str(), "kind": "docs"}),
    )
    .await;
    assert_eq!(refused["code"], "bad-request");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn a_task_starts_with_its_loop_and_watchers_see_loop_changes() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let refused = request(
        &mut client,
        json!({"type": "start-task", "id": 1, "workspace": "w", "title": "bad",
               "loop": {"check": "rm -rf /"}}),
    )
    .await;
    assert_eq!(refused["code"], "bad-request");
    assert!(
        backend.launches().is_empty(),
        "a refused loop starts no agent"
    );
    let started = request(
        &mut client,
        json!({"type": "start-task", "id": 2, "workspace": "w", "title": "fix",
               "loop": {"check": "make test"}}),
    )
    .await;
    let agent = started["terminal"]["id"].clone();
    assert_eq!(
        request(&mut client, json!({"type": "watch", "id": 3})).await["type"],
        "reply"
    );
    let again = request(
        &mut client,
        json!({"type": "loop-start", "id": 4, "terminal": agent, "loop": {"check": "make test"}, "reset": true}),
    )
    .await;
    assert_eq!(again["loop"]["state"], "armed");
    let note = loop {
        let n = client.read_control().await;
        if n["type"] == "loops" {
            break n;
        }
    };
    assert_eq!(note["loops"][0]["check"], "make test");
    let stopped = request(
        &mut client,
        json!({"type": "loop-stop", "id": 5, "terminal": agent}),
    )
    .await;
    assert_eq!(stopped["loop"]["state"], "stopped");
    let listed = request(&mut client, json!({"type": "loops", "id": 6})).await;
    assert_eq!(listed["loops"].as_array().unwrap().len(), 1);
    let shell = terminal(&backend).await;
    let not_agent = request(
        &mut client,
        json!({"type": "loop-start", "id": 7, "terminal": shell.as_str(), "loop": {"check": "make test"}}),
    )
    .await;
    assert_eq!(not_agent["code"], "bad-request");
}

#[tokio::test]
async fn a_chat_opens_with_the_conversation_and_ends_with_the_agent() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let fix = request(
        &mut client,
        json!({"type": "start-task", "id": 1, "workspace": "w", "title": "fix"}),
    )
    .await;
    let opened = request(
        &mut client,
        json!({"type": "chat-open", "id": 2, "terminal": fix["terminal"]["id"]}),
    )
    .await;
    assert_eq!(opened["chat"], 1);
    assert_eq!(opened["update"]["reset"], true);
    assert_eq!(opened["update"]["entries"][0]["text"], "Clean the build");
    assert_eq!(
        opened["update"]["prompt"]["options"][0],
        json!({"label": "Allow", "input": "1"})
    );
    let gone = request(
        &mut client,
        json!({"type": "start-task", "id": 3, "workspace": "w", "title": "gone"}),
    )
    .await;
    let second = request(
        &mut client,
        json!({"type": "chat-open", "id": 4, "terminal": gone["terminal"]["id"]}),
    )
    .await;
    assert_eq!(second["chat"], 2);
    let ended = client.read_for(Duration::from_secs(3)).await;
    let ended: Value = serde_json::from_slice(&ended.payload).unwrap();
    assert_eq!(
        ended,
        json!({"type": "chat-ended", "chat": 2, "reason": "Claude Code exited"})
    );
    let closed = request(
        &mut client,
        json!({"type": "chat-close", "id": 5, "chat": 1}),
    )
    .await;
    assert_eq!(closed["type"], "reply");
    let shell = terminal(&backend).await;
    let refused = request(
        &mut client,
        json!({"type": "chat-open", "id": 6, "terminal": shell.as_str()}),
    )
    .await;
    assert_eq!(refused["code"], "bad-request");
}

#[tokio::test]
async fn workspaces_list_what_a_task_may_use() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let reply = request(&mut client, json!({"type": "workspaces", "id": 1})).await;
    assert_eq!(
        reply["workspaces"],
        json!([{"name": "Personal", "root": "/work", "repos": ["app"], "agent": "claude",
                "loop": {"checks": "any"},
                "connections": [{"name": "github", "default": true,
                    "used_by": {"routines": ["digest"], "tasks": []}, "last_used": 1_700_000_000}],
                "mcp": [{"name": "docs", "default": true,
                    "used_by": {"routines": [], "tasks": []}, "last_used": null}],
                "branches": {"app": "main"}}])
    );
}

#[tokio::test]
async fn workspaces_show_every_live_task_agent_under_each_connection_and_server() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let start = |id: u64, title: &str, extra: Value| {
        let mut msg =
            json!({"type": "start-task", "id": id, "workspace": "Personal", "title": title});
        if let (Some(m), Some(e)) = (msg.as_object_mut(), extra.as_object()) {
            m.extend(e.clone());
        }
        msg
    };
    let both = request(
        &mut client,
        start(
            1,
            "both",
            json!({"connections": ["github"], "mcp": ["docs"]}),
        ),
    )
    .await;
    let both = both["terminal"]["id"].as_str().unwrap().to_owned();
    let plain = request(&mut client, start(2, "plain", json!({}))).await;
    assert_eq!(plain["existing"], false, "{plain}");
    let again = request(
        &mut client,
        start(
            3,
            "both",
            json!({"connections": ["github"], "mcp": ["docs"]}),
        ),
    )
    .await;
    assert_eq!(again["existing"], true, "{again}");
    // Only a new agent counts as a start.
    assert_eq!(STARTED.with(|s| s.borrow().clone()), ["both", "plain"]);

    let plain = plain["terminal"]["id"].as_str().unwrap().to_owned();
    for launch in backend.launches() {
        assert_eq!(
            launch.labels.get("muxa.mcp").map(String::as_str),
            Some(r#"["docs"]"#),
            "named or not, every agent got the Workspace's server"
        );
    }
    let reply = request(&mut client, json!({"type": "workspaces", "id": 4})).await;
    let ws = &reply["workspaces"][0];
    assert_eq!(
        ws["connections"][0]["used_by"],
        json!({"routines": ["digest"], "tasks": [both, plain]})
    );
    assert_eq!(ws["mcp"][0]["used_by"]["tasks"], json!([both, plain]));

    backend
        .end(
            &TerminalId::new(&both),
            Exit {
                reason: "exited".into(),
                code: Some(0),
                signal: None,
            },
        )
        .unwrap();
    let reply = request(&mut client, json!({"type": "workspaces", "id": 5})).await;
    assert_eq!(
        reply["workspaces"][0]["connections"][0]["used_by"]["tasks"],
        json!([plain]),
        "an ended agent uses nothing"
    );
}

/// A live Task agent working in `dir`, with a branch place there and a
/// read-only one.
async fn task_agent(backend: &FakeBackend, dir: &str) -> TerminalId {
    let places = json!([{"kind": "branch", "path": dir, "branch": "fix"},
                        {"kind": "read", "path": "/src/docs"}]);
    let launch = Launch {
        name: Some("fix".into()),
        labels: [
            ("muxa.kind".to_owned(), "task".to_owned()),
            ("muxa.role".to_owned(), "agent".to_owned()),
            ("muxa.dir".to_owned(), dir.to_owned()),
            ("muxa.branch".to_owned(), "fix".to_owned()),
            ("muxa.places".to_owned(), places.to_string()),
        ]
        .into(),
        ..Launch::default()
    };
    backend.run(&launch).await.unwrap().id
}

/// A Review item for `agent`; its id.
async fn review_item(client: &mut Client, agent: &TerminalId) -> String {
    let reply = request(
        client,
        json!({"type": "task-review", "id": 90, "terminal": agent.as_str()}),
    )
    .await;
    let item = &reply["inbox"]["items"][0];
    assert_eq!(item["terminal"], agent.as_str(), "{reply}");
    item["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn approve_closes_the_task_unless_it_would_lose_work() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let agent = task_agent(&backend, "/work/dirty-fix").await;
    let item = review_item(&mut client, &agent).await;

    let refused = request(
        &mut client,
        json!({"type": "inbox-approve", "id": 1, "item": item}),
    )
    .await;
    assert_eq!(
        (refused["code"].as_str(), refused["message"].as_str()),
        (
            Some("would-lose-work"),
            Some("dirty has uncommitted changes")
        )
    );
    assert!(backend.get(&agent).await.unwrap().live, "nothing changed");
    assert!(CLOSED.with(|c| c.borrow().is_empty()));

    let forced = request(
        &mut client,
        json!({"type": "inbox-approve", "id": 2, "item": item, "worktree": "force"}),
    )
    .await;
    assert_eq!(forced["inbox"]["done"][0]["id"], item.as_str(), "{forced}");
    assert!(forced["inbox"]["items"].as_array().unwrap().is_empty());
    assert!(
        backend.get(&agent).await.is_err(),
        "the agent's terminal is gone"
    );
    let closed = CLOSED.with(|c| c.borrow().clone());
    assert_eq!(closed.len(), 1);
    assert_eq!(closed[0].1, WorktreeMode::Force);
    let kinds: Vec<PlaceKind> = closed[0].0.iter().map(|p| p.kind).collect();
    assert_eq!(kinds, [PlaceKind::Branch, PlaceKind::Read]);

    // A clean Task closes without force; a Todo has no Task to close.
    let clean = task_agent(&backend, "/work/fix").await;
    let item = review_item(&mut client, &clean).await;
    let ok = request(
        &mut client,
        json!({"type": "inbox-approve", "id": 3, "item": item}),
    )
    .await;
    assert_eq!(ok["type"], "reply", "{ok}");
    let todo = request(
        &mut client,
        json!({"type": "inbox-add", "id": 4, "kind": "todo", "title": "milk"}),
    )
    .await["inbox"]["items"][0]["id"]
        .clone();
    let wrong = request(
        &mut client,
        json!({"type": "inbox-approve", "id": 5, "item": todo}),
    )
    .await;
    assert_eq!(wrong["code"], "bad-request");
}

#[tokio::test]
async fn request_changes_types_the_comment_into_the_agent() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let agent = task_agent(&backend, "/work/fix").await;
    let item = review_item(&mut client, &agent).await;
    let bad = request(
        &mut client,
        json!({"type": "inbox-request-changes", "id": 1, "item": item, "comment": "no\u{1b}[201~"}),
    )
    .await;
    assert_eq!(bad["code"], "bad-request");
    let sent = request(
        &mut client,
        json!({"type": "inbox-request-changes", "id": 2, "item": item,
               "comment": "Rename it.\nAnd add a test."}),
    )
    .await;
    assert_eq!(sent["inbox"]["done"][0]["id"], item.as_str(), "{sent}");
    let typed: Vec<u8> = backend.inputs(&agent).concat();
    assert_eq!(typed, b"\x1b[200~Rename it.\nAnd add a test.\x1b[201~\r");

    let item = review_item(&mut client, &agent).await;
    request(
        &mut client,
        json!({"type": "inbox-request-changes", "id": 3, "item": item, "comment": "One more"}),
    )
    .await;
    assert!(backend.inputs(&agent).concat().ends_with(b"One more\r"));
}

#[tokio::test]
async fn retry_runs_again_what_made_the_failed_item() {
    let backend = FakeBackend::new();
    let inbox = Arc::new(Inbox::memory());
    let failed = |title: &str, terminal: Option<String>, routine: Option<&str>| NewItem {
        kind: ItemKind::Failed,
        title: title.into(),
        note: None,
        workspace: None,
        due: None,
        source: "task".into(),
        key: None,
        once: false,
        terminal,
        routine: routine.map(|name| RoutineRef {
            workspace: "Personal".into(),
            name: name.into(),
        }),
        place: None,
        extra: Default::default(),
    };
    let add = |new| inbox.change(|b, now| b.add(new, now)).unwrap().unwrap();
    let busy = add(failed("digest failed", None, Some("digest")));
    let audit = add(failed("audit failed", None, Some("audit")));
    let agent = task_agent(&backend, "/work/fix").await;
    backend
        .end(
            &agent,
            Exit {
                reason: "exited".into(),
                code: Some(1),
                signal: None,
            },
        )
        .unwrap();
    let stopped = add(failed(
        "The agent of fix stopped",
        Some(agent.as_str().into()),
        None,
    ));
    let capture = add(failed("odd", None, None));
    let sessions = Arc::new(Sessions::new());
    let mut client = Client::connect_with(&backend, &sessions, &inbox, PARK, hello(None)).await;
    client.welcome().await;

    let conflict = request(
        &mut client,
        json!({"type": "inbox-retry", "id": 1, "item": busy}),
    )
    .await;
    assert_eq!(conflict["code"], "conflict");
    let ran = request(
        &mut client,
        json!({"type": "inbox-retry", "id": 2, "item": audit}),
    )
    .await;
    assert_eq!(ran["inbox"]["done"][0]["id"], audit.as_str(), "{ran}");

    let restarted = request(
        &mut client,
        json!({"type": "inbox-retry", "id": 3, "item": stopped}),
    )
    .await;
    assert_eq!(
        restarted["inbox"]["done"][0]["id"],
        stopped.as_str(),
        "{restarted}"
    );
    let launch = backend.launches().last().cloned().unwrap();
    assert_eq!(launch.argv, ["agent", "--again"]);
    assert_eq!(STARTED.with(|s| s.borrow().clone()), ["fix"]);
    assert_eq!(
        launch.labels.get("muxa.dir").map(String::as_str),
        Some("/work/fix")
    );
    assert!(
        backend.get(&agent).await.is_err(),
        "the ended terminal is removed"
    );

    let nothing = request(
        &mut client,
        json!({"type": "inbox-retry", "id": 4, "item": capture}),
    )
    .await;
    assert_eq!(nothing["code"], "bad-request");
}

#[tokio::test]
async fn retry_starts_a_live_agents_task_loop_over() {
    let backend = FakeBackend::new();
    let inbox = Arc::new(Inbox::memory());
    let sessions = Arc::new(Sessions::new());
    let mut client = Client::connect_with(&backend, &sessions, &inbox, PARK, hello(None)).await;
    client.welcome().await;
    let started = request(
        &mut client,
        json!({"type": "start-task", "id": 1, "workspace": "Personal", "title": "fix",
               "loop": {"check": "make test"}}),
    )
    .await;
    let agent = started["terminal"]["id"].as_str().unwrap().to_owned();
    assert!(RESETS.with(|r| r.borrow().is_empty()));
    let item = |terminal: &str| NewItem {
        kind: ItemKind::Failed,
        title: "The loop of fix gave up".into(),
        note: None,
        workspace: None,
        due: None,
        source: "task".into(),
        key: None,
        once: false,
        terminal: Some(terminal.to_owned()),
        routine: None,
        place: None,
        extra: Default::default(),
    };
    let failed = inbox
        .change(|b, now| b.add(item(&agent), now))
        .unwrap()
        .unwrap();

    let retried = request(
        &mut client,
        json!({"type": "inbox-retry", "id": 2, "item": failed}),
    )
    .await;
    assert_eq!(
        retried["inbox"]["done"][0]["id"],
        failed.as_str(),
        "{retried}"
    );
    assert_eq!(RESETS.with(|r| r.borrow().clone()), [agent.as_str()]);
    assert_eq!(backend.launches().len(), 1, "the live agent keeps running");
    assert!(backend.get(&TerminalId::new(&agent)).await.unwrap().live);

    // A live agent without a loop has nothing to retry.
    let bare = task_agent(&backend, "/work/bare").await;
    let failed = inbox
        .change(|b, now| b.add(item(bare.as_str()), now))
        .unwrap()
        .unwrap();
    let refused = request(
        &mut client,
        json!({"type": "inbox-retry", "id": 3, "item": failed}),
    )
    .await;
    assert_eq!(refused["code"], "bad-request", "{refused}");
}

#[tokio::test]
async fn a_tasks_shells_start_in_its_place_and_only_they_close() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let agent = task_agent(&backend, "/work/fix").await;

    let started = request(
        &mut client,
        json!({"type": "task-shell", "id": 1, "terminal": agent.as_str()}),
    )
    .await;
    let shell = started["terminal"].clone();
    assert_eq!(shell["labels"]["muxa.role"], "shell", "{started}");
    assert_eq!(shell["labels"]["muxa.kind"], "task");
    assert_eq!(shell["labels"]["muxa.dir"], "/work/fix");
    assert_eq!(shell["name"], "fix-sh");
    let launch = backend.launches().pop().unwrap();
    assert_eq!(
        launch.cwd.as_deref(),
        Some(std::path::Path::new("/work/fix"))
    );
    assert_eq!(launch.argv, ["/bin/sh", "-l"]);
    assert!(launch.env.is_empty(), "a shell gets no secrets");

    // A shell beside a shell, or beside nothing, is refused.
    let wrong = request(
        &mut client,
        json!({"type": "task-shell", "id": 2, "terminal": shell["id"]}),
    )
    .await;
    assert_eq!(wrong["code"], "bad-request");
    for id in [3, 4] {
        let more = request(
            &mut client,
            json!({"type": "task-shell", "id": id, "terminal": agent.as_str()}),
        )
        .await;
        assert_eq!(more["type"], "reply", "{more}");
    }
    let fourth = request(
        &mut client,
        json!({"type": "task-shell", "id": 5, "terminal": agent.as_str()}),
    )
    .await;
    assert_eq!(fourth["code"], "conflict", "three shells per Task");

    let agent_close = request(
        &mut client,
        json!({"type": "close-terminal", "id": 6, "terminal": agent.as_str()}),
    )
    .await;
    assert_eq!(
        agent_close["code"], "bad-request",
        "the agent never closes this way"
    );
    assert!(backend.get(&agent).await.unwrap().live);
    let closed = request(
        &mut client,
        json!({"type": "close-terminal", "id": 7, "terminal": shell["id"]}),
    )
    .await;
    assert_eq!(closed["type"], "reply", "{closed}");
    let id = TerminalId::new(shell["id"].as_str().unwrap().to_owned());
    assert!(
        backend.get(&id).await.is_err(),
        "the shell's terminal is gone"
    );
}

#[tokio::test]
async fn approving_a_task_ends_its_shells() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let agent = task_agent(&backend, "/work/fix").await;
    let started = request(
        &mut client,
        json!({"type": "task-shell", "id": 1, "terminal": agent.as_str()}),
    )
    .await;
    let shell = TerminalId::new(started["terminal"]["id"].as_str().unwrap().to_owned());
    let item = review_item(&mut client, &agent).await;
    let ok = request(
        &mut client,
        json!({"type": "inbox-approve", "id": 2, "item": item}),
    )
    .await;
    assert_eq!(ok["type"], "reply", "{ok}");
    assert!(
        backend.get(&shell).await.is_err(),
        "the shell went with the Task"
    );
}

#[tokio::test]
async fn close_task_ends_a_task_without_its_item() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let dirty = task_agent(&backend, "/work/dirty-fix").await;
    let refused = request(
        &mut client,
        json!({"type": "close-task", "id": 1, "terminal": dirty.as_str()}),
    )
    .await;
    assert_eq!(refused["code"], "would-lose-work", "{refused}");
    assert!(backend.get(&dirty).await.unwrap().live, "nothing changed");

    let agent = task_agent(&backend, "/work/fix").await;
    let item = review_item(&mut client, &agent).await;
    let closed = request(
        &mut client,
        json!({"type": "close-task", "id": 2, "terminal": agent.as_str()}),
    )
    .await;
    assert_eq!(closed["inbox"]["done"][0]["id"], item.as_str(), "{closed}");
    assert!(
        backend.get(&agent).await.is_err(),
        "the agent's terminal is gone"
    );

    let shell = terminal(&backend).await;
    let wrong = request(
        &mut client,
        json!({"type": "close-task", "id": 3, "terminal": shell.as_str()}),
    )
    .await;
    assert_eq!(wrong["code"], "bad-request");
}

#[tokio::test]
async fn an_upload_goes_to_the_tasks_folder_by_the_host() {
    let backend = FakeBackend::new();
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    let agent = task_agent(&backend, "/work/fix").await;
    // "build log" in base64.
    let ok = request(
        &mut client,
        json!({"type": "upload", "id": 1, "terminal": agent.as_str(), "name": "build.log", "data": "YnVpbGQgbG9n"}),
    )
    .await;
    assert_eq!(ok["path"], ".muxa/uploads/build.log", "{ok}");
    let written = UPLOADS.with(|u| u.borrow().clone());
    assert_eq!(
        written,
        [(
            "/work/fix".into(),
            "build.log".to_owned(),
            b"build log".to_vec()
        )]
    );

    let refusals = [
        json!({"name": "../x", "data": "eA=="}),
        json!({"name": ".env", "data": "eA=="}),
        json!({"name": "x", "data": "not base64!"}),
    ];
    for (n, fields) in refusals.into_iter().enumerate() {
        let mut msg = json!({"type": "upload", "id": 10 + n, "terminal": agent.as_str()});
        msg.as_object_mut()
            .unwrap()
            .extend(fields.as_object().unwrap().clone());
        let refused = request(&mut client, msg).await;
        assert_eq!(refused["code"], "bad-request", "{refused}");
    }
    let plain = terminal(&backend).await;
    let wrong = request(
        &mut client,
        json!({"type": "upload", "id": 20, "terminal": plain.as_str(), "name": "x", "data": "eA=="}),
    )
    .await;
    assert_eq!(
        wrong["code"], "bad-request",
        "only a Task's terminal takes files"
    );
    let typo = request(
        &mut client,
        json!({"type": "upload", "id": 21, "terminal": 7, "name": "x", "data": "c2VjcmV0"}),
    )
    .await;
    let message = typo["message"].as_str().unwrap_or_default();
    assert!(
        !message.contains("c2VjcmV0"),
        "a bad upload never echoes its bytes: {message}"
    );
    assert_eq!(UPLOADS.with(|u| u.borrow().len()), 1);
}
