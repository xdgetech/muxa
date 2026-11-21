//! `events.subscribe`: a snapshot of matching terminals, then every change.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

use crate::client::ChannelMsg;
use crate::frame::Frame;
use crate::{Client, Error, Terminal};

#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    /// Only terminals carrying all of these labels.
    pub labels: BTreeMap<String, String>,
    pub terminal: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A fact changed: `kind` is `created`, `status`, `size`, `finished`, ...
    /// `data` is herdr-core's data for that kind.
    Change {
        seq: u64,
        kind: String,
        terminal: Option<String>,
        data: Value,
    },
    Heartbeat {
        seq: u64,
    },
    /// The subscriber fell behind; `terminals` replaces the snapshot.
    Missed {
        seq: u64,
        terminals: Vec<Terminal>,
    },
}

/// A subscription. Messages are JSON-RPC notifications on its own channel.
pub struct Events {
    client: Client,
    channel: u32,
    rx: mpsc::UnboundedReceiver<ChannelMsg>,
    buf: Vec<u8>,
    done: bool,
}

impl Client {
    /// Subscribes, and returns the snapshot's `seq` and terminals with the stream.
    pub async fn subscribe(
        &self,
        filter: &EventFilter,
    ) -> Result<(u64, Vec<Terminal>, Events), Error> {
        let mut params = Map::new();
        if !filter.labels.is_empty() {
            params.insert("labels".into(), json!(filter.labels));
        }
        if let Some(t) = &filter.terminal {
            params.insert("terminal".into(), json!(t));
        }
        let result = self.call("events.subscribe", Value::Object(params)).await?;
        let channel = result["channel"]
            .as_u64()
            .and_then(|c| u32::try_from(c).ok())
            .filter(|c| *c != 0)
            .ok_or_else(|| Error::unexpected("events.subscribe", "no event channel"))?;
        let seq = result["seq"].as_u64().unwrap_or(0);
        let terminals = serde_json::from_value(result["terminals"].clone())?;
        let rx = self.open_channel(channel);
        let events = Events {
            client: self.clone(),
            channel,
            rx,
            buf: Vec::new(),
            done: false,
        };
        Ok((seq, terminals, events))
    }
}

impl Events {
    /// The next event, or `None` when the subscription or connection ends.
    pub async fn next(&mut self) -> Option<Result<Event, Error>> {
        loop {
            if let Some(parsed) = self.take_message() {
                return Some(parsed.and_then(to_event));
            }
            if self.done {
                return None;
            }
            match self.rx.recv().await {
                Some(ChannelMsg::Data(bytes)) => {
                    let consumed = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
                    let _ = self.client.send(Frame::credit(self.channel, consumed));
                    self.buf.extend_from_slice(&bytes);
                }
                Some(ChannelMsg::Resize(..)) => {}
                Some(ChannelMsg::Close(_)) | None => self.done = true,
            }
        }
    }

    /// One complete JSON message from the front of the buffer, if there is one.
    fn take_message(&mut self) -> Option<Result<Value, Error>> {
        let mut stream = serde_json::Deserializer::from_slice(&self.buf).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) => {
                let used = stream.byte_offset();
                self.buf.drain(..used);
                Some(Ok(value))
            }
            Some(Err(e)) if e.is_eof() => None,
            Some(Err(e)) => {
                self.buf.clear();
                Some(Err(e.into()))
            }
            None => {
                self.buf.clear();
                None
            }
        }
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        self.client.close_channel(self.channel);
    }
}

fn to_event(msg: Value) -> Result<Event, Error> {
    let params = &msg["params"];
    let seq = params["seq"].as_u64().unwrap_or(0);
    match msg["method"].as_str() {
        Some("event") => Ok(Event::Change {
            seq,
            kind: params["type"].as_str().unwrap_or_default().to_owned(),
            terminal: params["terminal"].as_str().map(str::to_owned),
            data: params["data"].clone(),
        }),
        Some("heartbeat") => Ok(Event::Heartbeat { seq }),
        Some("missed") => Ok(Event::Missed {
            seq,
            terminals: serde_json::from_value(params["terminals"].clone())?,
        }),
        other => Err(Error::unexpected(
            "events.subscribe",
            format!("unknown notification {other:?}"),
        )),
    }
}
