//! An attached terminal: its first paint and raw output, with input and resize.

use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::client::ChannelMsg;
use crate::frame::{Frame, FrameKind, MAX_PAYLOAD};
use crate::{Client, Error, Terminal};

#[derive(Debug, Clone)]
pub struct AttachSpec {
    pub terminal: String,
    pub cols: u16,
    pub rows: u16,
    /// Scrollback lines to send before the screen (main screen only).
    pub history: u32,
    /// Watch only: never type or resize.
    pub observe: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StreamEvent {
    /// Raw terminal bytes. The first ones are the first paint.
    Output(Vec<u8>),
    /// The terminal's size changed (observers that asked for it).
    Resize { cols: u16, rows: u16 },
    /// The stream ended. `reason` is herdr-core's JSON reason, for example
    /// `{"reason":"exited","exit_code":0}`, or null after a detach.
    Closed { reason: Value },
}

/// A live stream. Dropping it stops routing; call [`Attach::detach`] first to
/// tell the core.
pub struct Attach {
    client: Client,
    channel: u32,
    rx: mpsc::UnboundedReceiver<ChannelMsg>,
    terminal: Terminal,
    done: bool,
}

impl Client {
    pub async fn attach(&self, spec: &AttachSpec) -> Result<Attach, Error> {
        let result = self
            .call(
                "terminal.attach",
                json!({"terminal": spec.terminal, "cols": spec.cols, "rows": spec.rows,
                       "history": spec.history, "observe": spec.observe}),
            )
            .await?;
        let channel = result["channel"]
            .as_u64()
            .and_then(|c| u32::try_from(c).ok())
            .filter(|c| *c != 0)
            .ok_or_else(|| Error::unexpected("terminal.attach", "no stream channel"))?;
        let terminal = serde_json::from_value(result["terminal"].clone())?;
        let rx = self.open_channel(channel);
        Ok(Attach {
            client: self.clone(),
            channel,
            rx,
            terminal,
            done: false,
        })
    }
}

impl Attach {
    pub fn terminal(&self) -> &Terminal {
        &self.terminal
    }

    /// The next event, or `None` once the stream is over. Output is credited
    /// back to the core as it is handed out, so a slow reader slows the core.
    pub async fn next(&mut self) -> Option<StreamEvent> {
        if self.done {
            return None;
        }
        match self.rx.recv().await {
            Some(ChannelMsg::Data(bytes)) => {
                let consumed = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                // A closed connection also ends the stream on the next call.
                let _ = self.client.send(Frame::credit(self.channel, consumed));
                Some(StreamEvent::Output(bytes))
            }
            Some(ChannelMsg::Resize(cols, rows)) => Some(StreamEvent::Resize { cols, rows }),
            Some(ChannelMsg::Close(payload)) => {
                self.done = true;
                let reason = serde_json::from_slice(&payload).unwrap_or(Value::Null);
                Some(StreamEvent::Closed { reason })
            }
            None => {
                self.done = true;
                Some(StreamEvent::Closed {
                    reason: json!({"reason": "connection_lost"}),
                })
            }
        }
    }

    /// Sends bytes as if typed.
    pub fn input(&self, bytes: &[u8]) -> Result<(), Error> {
        for chunk in bytes.chunks(MAX_PAYLOAD) {
            self.client
                .send(Frame::new(self.channel, FrameKind::Data, chunk.to_vec()))?;
        }
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), Error> {
        self.client.send(Frame::resize(self.channel, cols, rows))
    }

    /// Asks the core to end the stream. The program keeps running.
    pub fn detach(&self) -> Result<(), Error> {
        self.client
            .send(Frame::new(self.channel, FrameKind::Close, Vec::new()))
    }
}

impl Drop for Attach {
    fn drop(&mut self) {
        self.client.close_channel(self.channel);
    }
}
