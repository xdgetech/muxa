//! A protocol v2 client against the server on the fake backend.

use std::sync::Arc;
use std::time::Duration;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Exit, Launch, SessionBackend, TerminalId};
use muxa_protocol::v2::{Frame, INITIAL_CREDIT, Kind};
use muxa_server::{
    Config, Host, Inbox, Prepared, RoutineList, ServeError, Services, Sessions, TaskError,
    TaskSpec, serve, watch_tasks,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::task::JoinHandle;

/// Worktrees under `/work`; a branch named `bad` is refused.
struct FakeTasks;

impl Host for FakeTasks {
    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        if spec.branch().is_some_and(|(_, b)| b == "bad") {
            return Err(TaskError::Invalid("bad is not a branch".into()));
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
            argv,
            env: [("K".to_owned(), "v".to_owned())].into(),
            places: Vec::new(),
        })
    }

    async fn workspaces(&self) -> Vec<String> {
        vec!["Personal".into()]
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
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(f) = Frame::decode(&mut self.buf).unwrap() {
                    return f;
                }
                let mut chunk = [0u8; 65536];
                let n = self.io.read(&mut chunk).await.unwrap();
                assert!(n > 0, "server closed the connection");
                self.buf.extend_from_slice(&chunk[..n]);
            }
        })
        .await
        .expect("a frame within 2 s")
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
    let start = json!({"type": "start-task", "id": 1, "workspace": "w", "title": "fix", "places": [{"kind": "branch", "repo": "/src/app", "branch": "fix"}], "plan": true});
    client.control(start.clone()).await;
    let reply = client.read_control().await;
    assert_eq!(reply["existing"], false, "{reply}");
    let labels = &reply["terminal"]["labels"];
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
        reply["terminal"]["name"], "fix",
        "the title names the terminal"
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
