//! The client against a scripted herdr-core over an in-memory pipe.

use muxa_herdr::{AttachSpec, Client, Error, Event, EventFilter, Frame, FrameKind, StreamEvent};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

struct Core {
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
}

impl Core {
    async fn read(&mut self) -> Frame {
        Frame::read(&mut self.reader)
            .await
            .unwrap()
            .expect("a frame")
    }

    async fn request(&mut self) -> (u64, String, Value) {
        let frame = self.read().await;
        assert_eq!((frame.channel, frame.kind), (0, FrameKind::Data));
        let msg: Value = serde_json::from_slice(&frame.payload).unwrap();
        (
            msg["id"].as_u64().unwrap(),
            msg["method"].as_str().unwrap().to_owned(),
            msg["params"].clone(),
        )
    }

    async fn reply(&mut self, id: u64, result: Value) {
        let body = json!({"jsonrpc": "2.0", "id": id, "result": result});
        self.send(Frame::new(
            0,
            FrameKind::Data,
            body.to_string().into_bytes(),
        ))
        .await;
    }

    async fn send(&mut self, frame: Frame) {
        frame.write(&mut self.writer).await.unwrap();
    }
}

fn terminal(address: &str) -> Value {
    json!({"address": address, "state": "live", "labels": {}, "argv": ["sh"], "cwd": "/",
           "created_ms": 1, "screen": "", "cols": 80, "rows": 24})
}

async fn connect(features: Value) -> Result<(Client, Core), Error> {
    let (client_end, core_end) = tokio::io::duplex(1 << 20);
    let (client_r, client_w) = tokio::io::split(client_end);
    let (core_r, core_w) = tokio::io::split(core_end);
    let mut core = Core {
        reader: BufReader::new(core_r),
        writer: core_w,
    };
    let core_task = tokio::spawn(async move {
        let mut line = String::new();
        core.reader.read_line(&mut line).await.unwrap();
        let hello: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(hello["params"]["framed"], json!(true));
        let reply = json!({"jsonrpc": "2.0", "id": 0, "result": {
            "generation": 1, "version": "0.1.21", "framed": true,
            "core": {}, "methods": [], "features": features}});
        core.writer
            .write_all(format!("{reply}\n").as_bytes())
            .await
            .unwrap();
        core
    });
    let client = Client::from_io(client_r, client_w).await?;
    Ok((client, core_task.await.unwrap()))
}

fn all_features() -> Value {
    json!(["framed", "attach", "events", "status"])
}

#[tokio::test]
async fn a_core_without_attach_is_refused() {
    let err = connect(json!(["framed", "events"])).await.err().unwrap();
    assert!(matches!(err, Error::MissingFeature(f) if f == "attach"));
}

#[tokio::test]
async fn calls_get_their_own_replies_in_any_order() {
    let (client, mut core) = connect(all_features()).await.unwrap();
    let c1 = client.clone();
    let c2 = client.clone();
    let first = tokio::spawn(async move { c1.get("t1").await });
    let second = tokio::spawn(async move { c2.get("t2").await });
    let (id_a, _, params_a) = core.request().await;
    let (id_b, _, params_b) = core.request().await;
    // Answer in reverse order.
    core.reply(id_b, terminal(params_b["terminal"].as_str().unwrap()))
        .await;
    core.reply(id_a, terminal(params_a["terminal"].as_str().unwrap()))
        .await;
    assert_eq!(first.await.unwrap().unwrap().address, "t1");
    assert_eq!(second.await.unwrap().unwrap().address, "t2");
}

#[tokio::test]
async fn an_rpc_error_names_the_method() {
    let (client, mut core) = connect(all_features()).await.unwrap();
    let call = tokio::spawn(async move { client.get("nope").await });
    let (id, _, _) = core.request().await;
    let body = json!({"jsonrpc": "2.0", "id": id,
                      "error": {"code": -32001, "message": "no terminal nope"}});
    core.send(Frame::new(
        0,
        FrameKind::Data,
        body.to_string().into_bytes(),
    ))
    .await;
    let err = call.await.unwrap().unwrap_err();
    assert!(
        matches!(&err, Error::Rpc { method, code: -32001, .. } if method == "terminal.get"),
        "{err}"
    );
}

#[tokio::test]
async fn stream_frames_sent_right_after_the_reply_are_kept() {
    let (client, mut core) = connect(all_features()).await.unwrap();
    let spec = AttachSpec {
        terminal: "t7".into(),
        cols: 100,
        rows: 30,
        history: 0,
        observe: false,
    };
    let attach = tokio::spawn(async move { client.attach(&spec).await });
    let (id, method, params) = core.request().await;
    assert_eq!(method, "terminal.attach");
    assert_eq!(
        (params["cols"].clone(), params["rows"].clone()),
        (json!(100), json!(30))
    );
    core.reply(
        id,
        json!({"channel": 5, "terminal": terminal("t7"),
                          "resize_notifications": false, "automatic_input": false}),
    )
    .await;
    core.send(Frame::new(5, FrameKind::Data, b"first paint".to_vec()))
        .await;
    let mut stream = attach.await.unwrap().unwrap();
    assert_eq!(stream.terminal().address, "t7");
    assert_eq!(
        stream.next().await,
        Some(StreamEvent::Output(b"first paint".to_vec()))
    );

    // The client returns credit for what it handed out.
    let credit = core.read().await;
    assert_eq!((credit.channel, credit.kind), (5, FrameKind::Credit));
    assert_eq!(credit.payload, 11u32.to_le_bytes().to_vec());

    stream.input(b"ls\r").unwrap();
    stream.resize(120, 40).unwrap();
    assert_eq!(
        core.read().await,
        Frame::new(5, FrameKind::Data, b"ls\r".to_vec())
    );
    assert_eq!(core.read().await, Frame::resize(5, 120, 40));

    core.send(Frame::new(
        5,
        FrameKind::Close,
        br#"{"reason":"exited","exit_code":0}"#.to_vec(),
    ))
    .await;
    assert_eq!(
        stream.next().await,
        Some(StreamEvent::Closed {
            reason: json!({"reason": "exited", "exit_code": 0})
        })
    );
    assert_eq!(stream.next().await, None);
}

#[tokio::test]
async fn events_split_across_frames_are_joined() {
    let (client, mut core) = connect(all_features()).await.unwrap();
    let sub = tokio::spawn(async move {
        client
            .subscribe(&EventFilter::default())
            .await
            .map(|(seq, terminals, events)| (seq, terminals.len(), events))
    });
    let (id, method, _) = core.request().await;
    assert_eq!(method, "events.subscribe");
    core.reply(
        id,
        json!({"seq": 41, "channel": 3, "terminals": [terminal("t1")]}),
    )
    .await;
    let (seq, count, mut events) = sub.await.unwrap().unwrap();
    assert_eq!((seq, count), (41, 1));

    let created = json!({"jsonrpc": "2.0", "method": "event",
        "params": {"seq": 42, "type": "created", "terminal": "t9", "data": {}}})
    .to_string();
    let (head, tail) = created.as_bytes().split_at(20);
    core.send(Frame::new(3, FrameKind::Data, head.to_vec()))
        .await;
    core.send(Frame::new(3, FrameKind::Data, tail.to_vec()))
        .await;
    let heartbeat = json!({"jsonrpc": "2.0", "method": "heartbeat", "params": {"seq": 42}});
    core.send(Frame::new(
        3,
        FrameKind::Data,
        heartbeat.to_string().into_bytes(),
    ))
    .await;

    match events.next().await.unwrap().unwrap() {
        Event::Change {
            seq,
            kind,
            terminal,
            ..
        } => {
            assert_eq!(
                (seq, kind.as_str(), terminal.as_deref()),
                (42, "created", Some("t9"))
            );
        }
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(
        events.next().await.unwrap().unwrap(),
        Event::Heartbeat { seq: 42 }
    );
}

#[tokio::test]
async fn a_lost_connection_fails_waiting_calls_and_ends_streams() {
    let (client, core) = connect(all_features()).await.unwrap();
    let waiting = {
        let client = client.clone();
        tokio::spawn(async move { client.get("t1").await })
    };
    tokio::task::yield_now().await;
    drop(core);
    assert!(matches!(waiting.await.unwrap(), Err(Error::Closed)));
    assert!(matches!(client.get("t2").await, Err(Error::Closed)));
}
