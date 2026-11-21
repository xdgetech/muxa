//! One framed connection to herdr-core. A reader task routes JSON-RPC replies
//! on channel 0 to their callers and stream frames to their channels.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};

use crate::frame::{Frame, FrameKind, resize_size};
use crate::{Error, Terminal};

/// The herdr-core protocol generation this client speaks.
pub const PROTOCOL_GENERATION: u64 = 1;

const REQUIRED_FEATURES: [&str; 3] = ["framed", "attach", "events"];
const MAX_HELLO_LINE: u64 = 1024 * 1024;

/// What arrives on a stream channel.
#[derive(Debug)]
pub(crate) enum ChannelMsg {
    Data(Vec<u8>),
    Resize(u16, u16),
    Close(Vec<u8>),
}

type Reply = Result<Value, Error>;

#[derive(Default)]
struct State {
    pending: HashMap<u64, oneshot::Sender<Reply>>,
    channels: HashMap<u32, mpsc::UnboundedSender<ChannelMsg>>,
    /// Frames for a channel whose reply the caller has not handled yet. The
    /// core sends a reply before its stream, so these are only a few frames,
    /// bounded by the stream's initial credit.
    orphans: HashMap<u32, Vec<ChannelMsg>>,
    closed: bool,
}

struct Inner {
    tx: mpsc::UnboundedSender<Frame>,
    state: Mutex<State>,
    next_id: AtomicU64,
    version: String,
    features: Vec<String>,
    _child: Mutex<Option<Child>>,
}

/// A connection to herdr-core. Cloning shares the connection; the `hc`
/// process ends when the last clone is dropped.
#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

impl Client {
    /// Runs `program args...` (for example `hc connect`, or
    /// `ssh host hc connect`) and speaks the protocol on its stdin and stdout.
    pub async fn spawn(program: &str, args: &[&str]) -> Result<Client, Error> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|source| Error::Spawn {
                program: program.to_owned(),
                source,
            })?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(Error::unexpected("spawn", "no stdin or stdout"));
        };
        Client::handshake(stdout, stdin, Some(child)).await
    }

    /// Speaks the protocol over any byte stream. Used by tests and relays.
    pub async fn from_io<R, W>(reader: R, writer: W) -> Result<Client, Error>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        Client::handshake(reader, writer, None).await
    }

    async fn handshake<R, W>(
        reader: R,
        mut writer: W,
        child: Option<Child>,
    ) -> Result<Client, Error>
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let hello = json!({"jsonrpc": "2.0", "id": 0, "method": "hello",
            "params": {"generation": PROTOCOL_GENERATION, "framed": true}});
        writer.write_all(format!("{hello}\n").as_bytes()).await?;
        writer.flush().await?;

        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        (&mut reader)
            .take(MAX_HELLO_LINE)
            .read_line(&mut line)
            .await?;
        if line.is_empty() {
            return Err(Error::Closed);
        }
        let result = rpc_result("hello", serde_json::from_str(&line)?)?;
        let generation = result["generation"].as_u64().unwrap_or(0);
        if generation != PROTOCOL_GENERATION {
            return Err(Error::Generation {
                found: generation,
                wanted: PROTOCOL_GENERATION,
            });
        }
        let features: Vec<String> = result["features"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(missing) = REQUIRED_FEATURES
            .iter()
            .find(|f| !features.iter().any(|have| have == *f))
        {
            return Err(Error::MissingFeature((*missing).to_owned()));
        }

        let (tx, rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            tx,
            state: Mutex::new(State::default()),
            next_id: AtomicU64::new(1),
            version: result["version"].as_str().unwrap_or_default().to_owned(),
            features,
            _child: Mutex::new(child),
        });
        tokio::spawn(write_frames(writer, rx, Arc::downgrade(&inner)));
        tokio::spawn(read_frames(reader, Arc::downgrade(&inner)));
        Ok(Client { inner })
    }

    pub fn version(&self) -> &str {
        &self.inner.version
    }

    pub fn has_feature(&self, feature: &str) -> bool {
        self.inner.features.iter().any(|f| f == feature)
    }

    /// One JSON-RPC call on channel 0.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, Error> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        {
            let mut state = self.state();
            if state.closed {
                return Err(Error::Closed);
            }
            state.pending.insert(id, reply_tx);
        }
        let body = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if let Err(e) = self.send(Frame::new(
            0,
            FrameKind::Data,
            body.to_string().into_bytes(),
        )) {
            self.state().pending.remove(&id);
            return Err(e);
        }
        let reply = reply_rx.await.map_err(|_| Error::Closed)?;
        reply.map_err(|e| match e {
            Error::Rpc { code, message, .. } => Error::Rpc {
                method: method.to_owned(),
                code,
                message,
            },
            other => other,
        })
    }

    pub async fn run(&self, spec: &crate::RunSpec) -> Result<Terminal, Error> {
        let result = self.call("terminal.run", spec.params()).await?;
        Ok(serde_json::from_value(result)?)
    }

    pub async fn get(&self, terminal: &str) -> Result<Terminal, Error> {
        let result = self
            .call("terminal.get", json!({"terminal": terminal}))
            .await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Live terminals, or every terminal with `all`.
    pub async fn list(&self, all: bool) -> Result<Vec<Terminal>, Error> {
        let result = self.call("terminal.list", json!({"all": all})).await?;
        Ok(serde_json::from_value(result["terminals"].clone())?)
    }

    /// The screen plus `lines` of history, as text.
    pub async fn read(&self, terminal: &str, lines: u32) -> Result<String, Error> {
        let result = self
            .call(
                "terminal.read",
                json!({"terminal": terminal, "lines": lines}),
            )
            .await?;
        result["text"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| Error::unexpected("terminal.read", "no text"))
    }

    /// Ends the program: hangup, then terminate, then kill.
    pub async fn kill(&self, terminal: &str) -> Result<Terminal, Error> {
        let result = self
            .call("terminal.kill", json!({"terminal": terminal}))
            .await?;
        Ok(serde_json::from_value(result)?)
    }

    /// Removes a finished terminal.
    pub async fn remove(&self, terminal: &str) -> Result<(), Error> {
        self.call("terminal.rm", json!({"terminal": terminal}))
            .await?;
        Ok(())
    }

    pub(crate) fn send(&self, frame: Frame) -> Result<(), Error> {
        if self.state().closed {
            return Err(Error::Closed);
        }
        self.inner.tx.send(frame).map_err(|_| Error::Closed)
    }

    /// Starts routing a channel's frames to the returned receiver, including
    /// frames that arrived before this call.
    pub(crate) fn open_channel(&self, channel: u32) -> mpsc::UnboundedReceiver<ChannelMsg> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut state = self.state();
        for msg in state.orphans.remove(&channel).unwrap_or_default() {
            let _ = tx.send(msg);
        }
        if !state.closed {
            state.channels.insert(channel, tx);
        }
        rx
    }

    pub(crate) fn close_channel(&self, channel: u32) {
        let mut state = self.state();
        state.channels.remove(&channel);
        state.orphans.remove(&channel);
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.inner.state)
    }
}

/// A poisoned lock only means another task panicked mid-update of plain maps;
/// the maps stay usable.
fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn rpc_result(method: &str, msg: Value) -> Result<Value, Error> {
    if let Some(err) = msg.get("error") {
        return Err(Error::Rpc {
            method: method.to_owned(),
            code: err["code"].as_i64().unwrap_or(0),
            message: err["message"].as_str().unwrap_or_default().to_owned(),
        });
    }
    msg.get("result")
        .cloned()
        .ok_or_else(|| Error::unexpected(method, "no result"))
}

async fn write_frames<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut rx: mpsc::UnboundedReceiver<Frame>,
    inner: std::sync::Weak<Inner>,
) {
    while let Some(frame) = rx.recv().await {
        if let Err(e) = frame.write(&mut writer).await {
            tracing::warn!(error = %e, "herdr-core write failed");
            break;
        }
    }
    if let Some(inner) = inner.upgrade() {
        shut(&inner);
    }
}

async fn read_frames<R: AsyncRead + Unpin>(mut reader: R, inner: std::sync::Weak<Inner>) {
    loop {
        let frame = match Frame::read(&mut reader).await {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!(error = %e, "herdr-core read failed");
                break;
            }
        };
        let Some(inner) = inner.upgrade() else { return };
        route(&inner, frame);
    }
    if let Some(inner) = inner.upgrade() {
        shut(&inner);
    }
}

fn route(inner: &Inner, frame: Frame) {
    if frame.channel == 0 {
        if frame.kind == FrameKind::Data {
            route_rpc(inner, &frame.payload);
        }
        return;
    }
    let msg = match frame.kind {
        FrameKind::Data => ChannelMsg::Data(frame.payload),
        FrameKind::Close => ChannelMsg::Close(frame.payload),
        FrameKind::Resize => match resize_size(&frame.payload) {
            Some((cols, rows)) => ChannelMsg::Resize(cols, rows),
            None => return,
        },
        _ => return,
    };
    let mut state = lock(&inner.state);
    match state.channels.get(&frame.channel) {
        Some(tx) => {
            if tx.send(msg).is_err() {
                state.channels.remove(&frame.channel);
            }
        }
        None => state.orphans.entry(frame.channel).or_default().push(msg),
    }
}

fn route_rpc(inner: &Inner, payload: &[u8]) {
    let msg: Value = match serde_json::from_slice(payload) {
        Ok(msg) => msg,
        Err(e) => {
            tracing::warn!(error = %e, len = payload.len(), "herdr-core sent bad json");
            return;
        }
    };
    let Some(id) = msg.get("id").and_then(Value::as_u64) else {
        // A notification on channel 0. Events arrive on their own channel in
        // framed mode, so nothing here needs routing.
        return;
    };
    let reply = rpc_result("", msg);
    if let Some(tx) = lock(&inner.state).pending.remove(&id) {
        let _ = tx.send(reply);
    }
}

/// Marks the connection closed. Dropping the senders wakes every waiting
/// caller with `Closed` and ends every stream.
fn shut(inner: &Inner) {
    let mut state = lock(&inner.state);
    state.closed = true;
    state.pending.clear();
    state.channels.clear();
    state.orphans.clear();
}
