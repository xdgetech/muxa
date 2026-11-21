//! Protocol v2 through the server to the user's running herdr-core.
//! Needs `--features herdr-integration` and MUXA_TEST_HERDR=1.
#![cfg(feature = "herdr-integration")]

use std::time::Duration;

use muxa_backend::{Launch, SessionBackend, TerminalId};
use muxa_herdr::{Client, HerdrBackend};
use muxa_protocol::v2::{Frame, Kind};
use muxa_server::{
    Config, Host, Inbox, PARK, Prepared, Services, Sessions, TaskError, TaskSpec, serve,
};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// A "Task" in the temp dir whose agent reports `blocked` with OSC 7501.
struct Script;

impl Host for Script {
    async fn prepare(&self, spec: &TaskSpec) -> Result<Prepared, TaskError> {
        let line = r"printf '\033]7501;state=blocked:app=muxa-test\033\\'; sleep 30";
        Ok(Prepared {
            dir: std::env::temp_dir(),
            repo: spec.repo.clone().into(),
            branch: spec.branch.clone(),
            argv: vec!["sh".into(), "-c".into(), line.into()],
            env: Default::default(),
        })
    }
}

fn config() -> Config {
    Config {
        server: "muxa test".into(),
        backend: "herdr-core".into(),
        park: PARK,
    }
}

async fn send(io: &mut tokio::io::DuplexStream, msg: Value) {
    let frame = Frame::new(0, Kind::Data, msg.to_string().into_bytes());
    io.write_all(&frame.encode().unwrap()).await.unwrap();
}

async fn control(io: &mut tokio::io::DuplexStream, buf: &mut Vec<u8>) -> Value {
    loop {
        if let Some(f) = Frame::decode(buf).unwrap() {
            if f.channel == 0 {
                return serde_json::from_slice(&f.payload).unwrap();
            }
            continue;
        }
        let mut chunk = [0u8; 65536];
        let n = io.read(&mut chunk).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&chunk[..n]);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_started_task_reports_its_agent_state() {
    if std::env::var("MUXA_TEST_HERDR").as_deref() != Ok("1") {
        eprintln!("SKIP  set MUXA_TEST_HERDR=1 to use the running herdr-core");
        return;
    }
    let backend = HerdrBackend::new(Client::spawn("hc", &["connect"]).await.unwrap());
    let (mut io, server_end) = tokio::io::duplex(8 << 20);
    let server_backend = backend.clone();
    let server = tokio::spawn(async move {
        let (r, w) = tokio::io::split(server_end);
        let (sessions, inbox) = (Sessions::new(), Inbox::memory());
        let services = Services {
            backend: &server_backend,
            host: &Script,
            sessions: &sessions,
            inbox: &inbox,
        };
        serve(&services, r, w, &config()).await
    });
    let branch = format!("muxa-task-{}", std::process::id());
    let mut started: Option<String> = None;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut buf = Vec::new();
        send(
            &mut io,
            json!({"type": "hello", "version": 2, "client": "test", "resume": null}),
        )
        .await;
        assert_eq!(control(&mut io, &mut buf).await["type"], "welcome");
        send(&mut io, json!({"type": "watch", "id": 1})).await;
        assert_eq!(control(&mut io, &mut buf).await["id"], 1);
        send(
            &mut io,
            json!({"type": "start-task", "id": 2, "repo": "/r", "branch": branch}),
        )
        .await;
        loop {
            let msg = control(&mut io, &mut buf).await;
            if msg["id"] == 2 {
                assert_eq!(msg["terminal"]["labels"]["muxa.role"], "agent", "{msg}");
                started = msg["terminal"]["id"].as_str().map(str::to_owned);
            }
            let mine = started.is_some() && msg["terminal"]["id"].as_str() == started.as_deref();
            if mine && msg["type"] == "changed" && msg["terminal"]["status"]["state"] == "blocked" {
                assert_eq!(msg["terminal"]["status"]["app"], "muxa-test");
                return;
            }
        }
    })
    .await;
    drop(io);
    let _ = server.await;
    if let Some(id) = started {
        let id = TerminalId::new(id);
        let _ = backend.kill(&id).await;
        backend.remove(&id).await.unwrap();
    }
    result.expect("the blocked state within 10 s");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_v2_client_types_into_a_herdr_terminal() {
    if std::env::var("MUXA_TEST_HERDR").as_deref() != Ok("1") {
        eprintln!("SKIP  set MUXA_TEST_HERDR=1 to use the running herdr-core");
        return;
    }
    let backend = HerdrBackend::new(Client::spawn("hc", &["connect"]).await.unwrap());
    let tag = format!("muxa-server-{}", std::process::id());
    let launch = Launch {
        argv: vec![
            "sh".into(),
            "-c".into(),
            "echo ready; while read l; do echo \"got:$l\"; done".into(),
        ],
        cwd: Some(std::env::temp_dir()),
        name: Some(tag.clone()),
        labels: [("muxa.test".to_owned(), tag.clone())].into(),
        env: Default::default(),
    };
    let id = backend.run(&launch).await.unwrap().id;

    let (mut io, server_end) = tokio::io::duplex(8 << 20);
    let server_backend = backend.clone();
    let server = tokio::spawn(async move {
        let (r, w) = tokio::io::split(server_end);
        let (sessions, inbox) = (Sessions::new(), Inbox::memory());
        let services = Services {
            backend: &server_backend,
            host: &Script,
            sessions: &sessions,
            inbox: &inbox,
        };
        serve(&services, r, w, &config()).await
    });

    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let mut buf = Vec::new();
        let next = async |io: &mut tokio::io::DuplexStream, buf: &mut Vec<u8>| loop {
            if let Some(f) = Frame::decode(buf).unwrap() {
                return f;
            }
            let mut chunk = [0u8; 65536];
            let n = io.read(&mut chunk).await.unwrap();
            assert!(n > 0);
            buf.extend_from_slice(&chunk[..n]);
        };
        let hello = json!({"type": "hello", "version": 2, "client": "test", "resume": null});
        io.write_all(
            &Frame::new(0, Kind::Data, hello.to_string().into_bytes())
                .encode()
                .unwrap(),
        )
        .await
        .unwrap();
        let attach =
            json!({"type": "attach", "id": 1, "terminal": id.as_str(), "cols": 90, "rows": 25});
        io.write_all(
            &Frame::new(0, Kind::Data, attach.to_string().into_bytes())
                .encode()
                .unwrap(),
        )
        .await
        .unwrap();
        let mut channel = 0;
        let mut seen = Vec::new();
        let mut typed = false;
        loop {
            let f = next(&mut io, &mut buf).await;
            if f.channel == 0 {
                let msg: Value = serde_json::from_slice(&f.payload).unwrap();
                if msg["type"] == "reply" {
                    channel = msg["channel"].as_u64().unwrap() as u32;
                }
                continue;
            }
            assert_eq!(f.channel, channel);
            if f.kind == Kind::Data {
                seen.extend_from_slice(f.split_u64().unwrap().1);
            }
            let text = String::from_utf8_lossy(&seen).into_owned();
            if text.contains("ready") && !typed {
                io.write_all(&Frame::input(channel, 1, b"one\r").encode().unwrap())
                    .await
                    .unwrap();
                typed = true;
            }
            if text.contains("got:one") {
                return;
            }
        }
    })
    .await;

    drop(io);
    let _ = server.await;
    let _ = backend.kill(&id).await;
    backend.remove(&id).await.unwrap();
    result.expect("the echo within 10 s");
}
