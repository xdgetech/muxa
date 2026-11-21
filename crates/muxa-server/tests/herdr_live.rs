//! Protocol v2 through the server to the user's running herdr-core.
//! Needs `--features herdr-integration` and MUXA_TEST_HERDR=1.
#![cfg(feature = "herdr-integration")]

use std::time::Duration;

use muxa_backend::{Launch, SessionBackend};
use muxa_herdr::{Client, HerdrBackend};
use muxa_protocol::v2::{Frame, Kind};
use muxa_server::{Config, serve};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
        let config = Config {
            server: "muxa test".into(),
            backend: "herdr-core".into(),
        };
        serve(&server_backend, r, w, &config).await
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
