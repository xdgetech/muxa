//! The client against the user's running herdr-core (`hc` on PATH).
//! Needs `--features herdr-integration` and MUXA_TEST_HERDR=1: herdr-core has
//! no isolated server mode, so each test adds one labelled terminal and
//! removes it again.
#![cfg(feature = "herdr-integration")]

use std::time::Duration;

use muxa_herdr::{AttachSpec, Client, Event, EventFilter, Name, RunSpec, StreamEvent};
use serde_json::json;

const SCRIPT: &str = r#"printf '\033]7501;state=working:app=muxa-test\033\\'
echo ready
while read l; do
  echo "got:$l size:$(stty size)"
  [ "$l" = done ] && printf '\033]7501;state=done:app=muxa-test\033\\'
done"#;

fn enabled() -> bool {
    if std::env::var("MUXA_TEST_HERDR").as_deref() == Ok("1") {
        return true;
    }
    eprintln!("SKIP  set MUXA_TEST_HERDR=1 to use the running herdr-core");
    false
}

/// Reads output until `needle` appears, or fails after 5 s.
async fn wait_for(stream: &mut muxa_herdr::Attach, seen: &mut Vec<u8>, needle: &str) {
    let found = tokio::time::timeout(Duration::from_secs(5), async {
        while !String::from_utf8_lossy(seen).contains(needle) {
            match stream.next().await {
                Some(StreamEvent::Output(bytes)) => seen.extend_from_slice(&bytes),
                Some(StreamEvent::Resize { .. }) => {}
                other => panic!("stream ended before {needle:?}: {other:?}"),
            }
        }
    })
    .await;
    assert!(found.is_ok(), "no {needle:?} within 5 s");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_terminal_runs_streams_resizes_reports_and_ends() {
    if !enabled() {
        return;
    }
    let client = Client::spawn("hc", &["connect"]).await.unwrap();
    let tag = format!("muxa-test-{}", std::process::id());
    let filter = EventFilter {
        labels: [("muxa".to_owned(), tag.clone())].into(),
        terminal: None,
    };
    let (_, before, mut events) = client.subscribe(&filter).await.unwrap();
    assert!(before.is_empty());

    let dir = std::env::temp_dir();
    let mut spec = RunSpec::new(vec!["sh".into(), "-c".into(), SCRIPT.into()]);
    spec.cwd = Some(dir.to_string_lossy().into_owned());
    spec.name = Some(Name::new(&tag).unwrap());
    spec.labels.insert("muxa".into(), tag.clone());
    let started = client.run(&spec).await.unwrap();
    assert!(started.is_live());
    let address = started.address.clone();

    let result = async {
        let mut stream = client
            .attach(&AttachSpec {
                terminal: address.clone(),
                cols: 100,
                rows: 30,
                history: 50,
                observe: false,
            })
            .await
            .unwrap();
        let mut seen = Vec::new();
        wait_for(&mut stream, &mut seen, "ready").await;

        stream.input(b"one\r").unwrap();
        wait_for(&mut stream, &mut seen, "got:one size:30 100").await;

        stream.resize(120, 40).unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        stream.input(b"two\r").unwrap();
        wait_for(&mut stream, &mut seen, "got:two size:40 120").await;

        stream.input(b"done\r").unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let status = client.get(&address).await.unwrap().status.unwrap();
        assert_eq!(
            (status.state.as_str(), status.app.as_deref()),
            ("done", Some("muxa-test"))
        );

        client.kill(&address).await.unwrap();
        let end = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match stream.next().await {
                    Some(StreamEvent::Closed { reason }) => return reason,
                    Some(_) => {}
                    None => panic!("stream ended without a reason"),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(end["reason"], json!("killed"));

        match events.next().await.unwrap().unwrap() {
            Event::Change { kind, terminal, .. } => {
                assert_eq!(
                    (kind.as_str(), terminal.as_deref()),
                    ("created", Some(address.as_str()))
                );
            }
            other => panic!("first event {other:?}"),
        }
    }
    .await;

    let _ = client.kill(&address).await;
    client.remove(&address).await.unwrap();
    result
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_terminal_is_an_rpc_error() {
    if !enabled() {
        return;
    }
    let client = Client::spawn("hc", &["connect"]).await.unwrap();
    let err = client.get("muxa-test-no-such-terminal").await.unwrap_err();
    assert!(
        matches!(&err, muxa_herdr::Error::Rpc { kind: Some(k), .. } if k == "not_found"),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn herdr_core_passes_the_backend_contract() {
    if !enabled() {
        return;
    }
    let client = Client::spawn("hc", &["connect"]).await.unwrap();
    let backend = muxa_herdr::HerdrBackend::new(client);
    let tag = format!("muxa-contract-{}", std::process::id());
    muxa_backend::contract::check(&backend, &std::env::temp_dir(), &tag)
        .await
        .unwrap();
}
