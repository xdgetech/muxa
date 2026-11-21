//! A protocol v2 client against the server on the fake backend.

use std::time::Duration;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Launch, SessionBackend, TerminalId};
use muxa_protocol::v2::{Frame, INITIAL_CREDIT, Kind};
use muxa_server::{Config, ServeError, serve};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::task::JoinHandle;

struct Client {
    io: DuplexStream,
    buf: Vec<u8>,
    server: JoinHandle<Result<(), ServeError>>,
}

impl Client {
    async fn start(backend: &FakeBackend, version: u32) -> Client {
        let (client_end, server_end) = tokio::io::duplex(8 << 20);
        let backend = backend.clone();
        let server = tokio::spawn(async move {
            let (r, w) = tokio::io::split(server_end);
            let config = Config {
                server: "muxa test".into(),
                backend: "fake".into(),
            };
            serve(&backend, r, w, &config).await
        });
        let mut client = Client {
            io: client_end,
            buf: Vec::new(),
            server,
        };
        client
            .control(json!({"type": "hello", "version": version, "client": "test", "resume": null}))
            .await;
        client
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
async fn leaving_detaches_every_terminal_and_keeps_programs_running() {
    let backend = FakeBackend::new();
    let id = terminal(&backend).await;
    let mut client = Client::start(&backend, 2).await;
    client.welcome().await;
    client.attach(1, &id).await;
    assert_eq!(backend.viewers(&id), 1);
    let Client { io, server, .. } = client;
    drop(io);
    server.await.unwrap().unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(backend.viewers(&id), 0);
    assert!(backend.get(&id).await.unwrap().live);
}
