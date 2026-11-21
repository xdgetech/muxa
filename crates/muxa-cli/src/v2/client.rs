//! A protocol v2 client over any byte stream (`protocol/PROTOCOL-v2.md`).
//! It keeps what a resume needs: the token, and per channel the next output
//! offset and the input not yet acknowledged.

use std::collections::{BTreeMap, VecDeque};

use muxa_protocol::v2::{self, Frame, Kind};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// What survives a lost connection.
#[derive(Debug, Default, Clone)]
pub struct State {
    pub token: Option<String>,
    channels: BTreeMap<u32, Channel>,
}

#[derive(Debug, Clone)]
struct Channel {
    terminal: String,
    cols: u16,
    rows: u16,
    /// The next output offset expected.
    offset: u64,
    last_seq: u64,
    acked: u64,
    pending: VecDeque<(u64, Vec<u8>)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Output to show. Bytes the client already had are cut off.
    Output { channel: u32, bytes: Vec<u8> },
    /// The next output starts a fresh screen.
    Paint { channel: u32 },
    /// The channel is over: the exit, or `None` when it was detached or not
    /// resumed.
    Closed { channel: u32, exit: Option<Value> },
    /// `changed`, `removed` or `resync` after `watch`.
    Notice(Value),
}

pub struct Client<R, W> {
    reader: R,
    writer: W,
    buf: Vec<u8>,
    next_id: u64,
    state: State,
    /// Read while waiting for a reply.
    queued: VecDeque<Event>,
    pub welcome: Value,
}

impl<R, W> Client<R, W>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    /// Says hello, with a resume when `state` has a token. Channels the
    /// server did not resume come out of [`Client::next`] as `Closed`.
    pub async fn open(reader: R, writer: W, name: &str, state: State) -> Result<Self, String> {
        let mut c = Client {
            reader,
            writer,
            buf: Vec::new(),
            next_id: 0,
            state,
            queued: VecDeque::new(),
            welcome: Value::Null,
        };
        let resume = c.state.token.clone().map(|token| {
            let channels: Vec<Value> = c
                .state
                .channels
                .iter()
                .map(|(n, ch)| {
                    json!({"channel": n, "terminal": ch.terminal, "offset": ch.offset,
                           "input_seq": ch.acked, "cols": ch.cols, "rows": ch.rows})
                })
                .collect();
            json!({"token": token, "channels": channels})
        });
        c.control(
            &json!({"type": "hello", "version": v2::VERSION, "client": name, "resume": resume}),
        )
        .await?;
        let welcome = loop {
            let frame = c.read().await?;
            if frame.channel == 0 && frame.kind == Kind::Data {
                break parse(&frame.payload)?;
            }
        };
        if welcome["type"] != "welcome" {
            return Err(format!("the server refused: {}", message(&welcome)));
        }
        c.state.token = welcome["token"].as_str().map(str::to_owned);
        c.resumed(&welcome["resumed"]).await?;
        c.welcome = welcome;
        Ok(c)
    }

    /// Resends input the server did not apply; drops channels it ended.
    async fn resumed(&mut self, resumed: &Value) -> Result<(), String> {
        let listed: BTreeMap<u32, Option<u64>> = resumed
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| {
                Some((
                    u32::try_from(r["channel"].as_u64()?).ok()?,
                    r["input_seq"].as_u64(),
                ))
            })
            .collect();
        let gone: Vec<u32> = self
            .state
            .channels
            .keys()
            .filter(|n| !listed.contains_key(n))
            .copied()
            .collect();
        for channel in gone {
            self.state.channels.remove(&channel);
            self.queued.push_back(Event::Closed {
                channel,
                exit: None,
            });
        }
        let mut resend = Vec::new();
        for (n, applied) in listed {
            let Some(ch) = self.state.channels.get_mut(&n) else {
                continue;
            };
            match applied {
                Some(seq) => {
                    ch.pending.retain(|(s, _)| *s > seq);
                    ch.acked = ch.acked.max(seq);
                    resend.extend(ch.pending.iter().map(|(s, b)| Frame::input(n, *s, b)));
                }
                // The server cannot tell what it applied: never type twice.
                None => ch.pending.clear(),
            }
        }
        for frame in resend {
            self.write(&frame).await?;
        }
        Ok(())
    }

    pub fn into_state(self) -> State {
        self.state
    }

    /// Sends a request and waits for its reply. Other messages are queued.
    pub async fn request(&mut self, mut msg: Value) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        msg["id"] = json!(id);
        self.control(&msg).await?;
        loop {
            let frame = self.read().await?;
            if frame.channel == 0 && frame.kind == Kind::Data {
                let reply = parse(&frame.payload)?;
                if reply["id"] == id {
                    return match reply["type"].as_str() {
                        Some("reply") => Ok(reply),
                        _ => Err(message(&reply)),
                    };
                }
                if let Some(e) = self.event_of(reply) {
                    self.queued.push_back(e);
                }
                continue;
            }
            if let Some(e) = self.frame(frame).await? {
                self.queued.push_back(e);
            }
        }
    }

    /// Attaches `terminal` and returns its channel.
    pub async fn attach(&mut self, terminal: &str, cols: u16, rows: u16) -> Result<u32, String> {
        let reply = self
            .request(json!({"type": "attach", "terminal": terminal, "cols": cols, "rows": rows}))
            .await?;
        let channel = reply["channel"]
            .as_u64()
            .and_then(|c| u32::try_from(c).ok())
            .ok_or("the attach reply has no channel")?;
        self.state.channels.insert(
            channel,
            Channel {
                terminal: terminal.to_owned(),
                cols,
                rows,
                offset: 0,
                last_seq: 0,
                acked: 0,
                pending: VecDeque::new(),
            },
        );
        Ok(channel)
    }

    /// Sends input; it is kept until the server acknowledges it.
    pub async fn input(&mut self, channel: u32, bytes: &[u8]) -> Result<(), String> {
        let ch = self
            .state
            .channels
            .get_mut(&channel)
            .ok_or_else(|| format!("no channel {channel}"))?;
        ch.last_seq += 1;
        let seq = ch.last_seq;
        ch.pending.push_back((seq, bytes.to_vec()));
        self.write(&Frame::input(channel, seq, bytes)).await
    }

    pub async fn resize(&mut self, channel: u32, cols: u16, rows: u16) -> Result<(), String> {
        if let Some(ch) = self.state.channels.get_mut(&channel) {
            (ch.cols, ch.rows) = (cols, rows);
        }
        self.write(&Frame::resize(channel, cols, rows)).await
    }

    /// The next event. An error means the connection is lost.
    pub async fn next(&mut self) -> Result<Event, String> {
        loop {
            if let Some(e) = self.queued.pop_front() {
                return Ok(e);
            }
            let frame = self.read().await?;
            if frame.channel == 0 && frame.kind == Kind::Data {
                if let Some(e) = self.event_of(parse(&frame.payload)?) {
                    return Ok(e);
                }
                continue;
            }
            if let Some(e) = self.frame(frame).await? {
                return Ok(e);
            }
        }
    }

    fn event_of(&mut self, msg: Value) -> Option<Event> {
        match msg["type"].as_str()? {
            "paint" => {
                let channel = u32::try_from(msg["channel"].as_u64()?).ok()?;
                if let Some(ch) = self.state.channels.get_mut(&channel) {
                    ch.offset = 0;
                }
                Some(Event::Paint { channel })
            }
            "changed" | "removed" | "resync" => Some(Event::Notice(msg)),
            _ => None,
        }
    }

    async fn frame(&mut self, frame: Frame) -> Result<Option<Event>, String> {
        let channel = frame.channel;
        match frame.kind {
            Kind::Data => {
                let (offset, bytes) = frame.split_u64().ok_or("short output frame")?;
                let Some(ch) = self.state.channels.get_mut(&channel) else {
                    return Ok(None);
                };
                let end = offset + bytes.len() as u64;
                // A resume may send again bytes this client already has.
                let skip = usize::try_from(ch.offset.saturating_sub(offset))
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                ch.offset = ch.offset.max(end);
                let fresh = bytes[skip..].to_vec();
                let n = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                self.write(&Frame::credit(channel, n)).await?;
                Ok((!fresh.is_empty()).then_some(Event::Output {
                    channel,
                    bytes: fresh,
                }))
            }
            Kind::Ack => {
                let seq = frame.split_u64().ok_or("short ack frame")?.0;
                if let Some(ch) = self.state.channels.get_mut(&channel) {
                    ch.pending.retain(|(s, _)| *s > seq);
                    ch.acked = ch.acked.max(seq);
                }
                Ok(None)
            }
            Kind::Close => {
                self.state.channels.remove(&channel);
                let exit = serde_json::from_slice(&frame.payload).ok();
                Ok(Some(Event::Closed { channel, exit }))
            }
            Kind::Credit | Kind::Resize => Ok(None),
        }
    }

    async fn control(&mut self, msg: &Value) -> Result<(), String> {
        self.write(&Frame::new(0, Kind::Data, msg.to_string().into_bytes()))
            .await
    }

    async fn write(&mut self, frame: &Frame) -> Result<(), String> {
        let bytes = frame.encode().map_err(|e| e.to_string())?;
        self.writer
            .write_all(&bytes)
            .await
            .map_err(|e| format!("connection lost: {e}"))?;
        self.writer
            .flush()
            .await
            .map_err(|e| format!("connection lost: {e}"))
    }

    async fn read(&mut self) -> Result<Frame, String> {
        loop {
            if let Some(f) = Frame::decode(&mut self.buf).map_err(|e| e.to_string())? {
                return Ok(f);
            }
            let mut chunk = [0u8; 65536];
            let n = self
                .reader
                .read(&mut chunk)
                .await
                .map_err(|e| format!("connection lost: {e}"))?;
            if n == 0 {
                return Err("connection lost".into());
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

fn parse(payload: &[u8]) -> Result<Value, String> {
    serde_json::from_slice(payload).map_err(|e| format!("bad control message: {e}"))
}

fn message(msg: &Value) -> String {
    match (msg["code"].as_str(), msg["message"].as_str()) {
        (Some(code), Some(m)) => format!("{m} ({code})"),
        _ => msg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use muxa_backend::fake::FakeBackend;
    use muxa_backend::{Launch, SessionBackend, TerminalId};
    use muxa_server::{
        Config, Host, Inbox, PARK, Prepared, Services, Sessions, TaskError, TaskSpec, serve,
    };
    use tokio::io::{DuplexStream, ReadHalf, WriteHalf};

    use super::*;

    struct NoTasks;

    impl Host for NoTasks {
        async fn prepare(&self, _: &TaskSpec) -> Result<Prepared, TaskError> {
            Err(TaskError::Invalid("no tasks here".into()))
        }
    }

    type Test = Client<ReadHalf<DuplexStream>, WriteHalf<DuplexStream>>;

    /// A client on a new connection to the server, which keeps `sessions`.
    async fn connect(backend: &FakeBackend, sessions: &Arc<Sessions>, state: State) -> Test {
        let (ours, theirs) = tokio::io::duplex(1 << 20);
        let (backend, sessions) = (backend.clone(), Arc::clone(sessions));
        tokio::spawn(async move {
            let (r, w) = tokio::io::split(theirs);
            let config = Config {
                server: "test".into(),
                backend: "fake".into(),
                park: PARK,
            };
            let inbox = Inbox::memory();
            let services = Services {
                backend: &backend,
                host: &NoTasks,
                sessions: &sessions,
                inbox: &inbox,
            };
            let _ = serve(&services, r, w, &config).await;
        });
        let (r, w) = tokio::io::split(ours);
        Client::open(r, w, "test", state).await.unwrap()
    }

    async fn terminal(backend: &FakeBackend) -> TerminalId {
        backend.run(&Launch::default()).await.unwrap().id
    }

    async fn output(c: &mut Test) -> Vec<u8> {
        match tokio::time::timeout(Duration::from_secs(2), c.next()).await {
            Ok(Ok(Event::Output { bytes, .. })) => bytes,
            other => panic!("expected output, got {other:?}"),
        }
    }

    async fn applied(backend: &FakeBackend, id: &TerminalId, n: usize) {
        while backend.inputs(id).len() < n {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    /// Lets the server see the lost connection and park the attachment.
    async fn lose(c: Test) -> State {
        let state = c.into_state();
        tokio::time::sleep(Duration::from_millis(50)).await;
        state
    }

    #[tokio::test]
    async fn a_resumed_client_gets_only_new_output_and_types_once() {
        let backend = FakeBackend::new();
        let id = terminal(&backend).await;
        backend.emit(&id, b"hello ");
        let sessions = Arc::new(Sessions::new());
        let mut c = connect(&backend, &sessions, State::default()).await;
        let ch = c.attach(id.as_str(), 80, 24).await.unwrap();
        assert_eq!(c.next().await.unwrap(), Event::Paint { channel: ch });
        assert_eq!(output(&mut c).await, b"hello ");
        // Applied, but the ACK is never read: the connection is lost first.
        c.input(ch, b"y").await.unwrap();
        applied(&backend, &id, 1).await;
        let state = lose(c).await;
        backend.emit(&id, b"world");

        let mut c = connect(&backend, &sessions, state).await;
        assert_eq!(c.welcome["resumed"][0]["input_seq"], 1);
        // The echo of `y` and `world`, both missed; not `hello ` again.
        assert_eq!(output(&mut c).await, b"echo:yworld");
        c.input(ch, b"n").await.unwrap();
        applied(&backend, &id, 2).await;
        assert_eq!(backend.inputs(&id), vec![b"y".to_vec(), b"n".to_vec()]);
    }

    #[tokio::test]
    async fn a_server_that_lost_the_session_repaints_and_drops_unsure_input() {
        let backend = FakeBackend::new();
        let id = terminal(&backend).await;
        backend.emit(&id, b"screen");
        let mut c = connect(&backend, &Arc::new(Sessions::new()), State::default()).await;
        let ch = c.attach(id.as_str(), 80, 24).await.unwrap();
        c.next().await.unwrap();
        assert_eq!(output(&mut c).await, b"screen");
        let state = lose(c).await;
        let mut state = state;
        // Input sent but never acknowledged, to a server that then restarted.
        state
            .channels
            .get_mut(&ch)
            .unwrap()
            .pending
            .push_back((1, b"rm".to_vec()));

        let mut c = connect(&backend, &Arc::new(Sessions::new()), state).await;
        assert_eq!(c.next().await.unwrap(), Event::Paint { channel: ch });
        assert_eq!(output(&mut c).await, b"screen");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(backend.inputs(&id).is_empty());
    }

    #[tokio::test]
    async fn a_terminal_gone_while_away_closes_its_channel() {
        let backend = FakeBackend::new();
        let id = terminal(&backend).await;
        let sessions = Arc::new(Sessions::new());
        let mut c = connect(&backend, &sessions, State::default()).await;
        let ch = c.attach(id.as_str(), 80, 24).await.unwrap();
        let state = lose(c).await;
        backend.kill(&id).await.unwrap();
        backend.remove(&id).await.unwrap();
        // A server without the session cannot attach a removed terminal.
        let mut c = connect(&backend, &Arc::new(Sessions::new()), state).await;
        assert_eq!(
            c.next().await.unwrap(),
            Event::Closed {
                channel: ch,
                exit: None
            }
        );
    }

    #[tokio::test]
    async fn a_refused_request_is_an_error_with_its_code() {
        let backend = FakeBackend::new();
        let mut c = connect(&backend, &Arc::new(Sessions::new()), State::default()).await;
        let err = c
            .request(json!({"type": "start-task", "workspace": "w", "title": "b", "places": [{"kind": "branch", "repo": "/r", "branch": "b"}]}))
            .await
            .unwrap_err();
        assert_eq!(err, "no tasks here (bad-request)");
    }
}
