//! Protocol v2 (`protocol/PROTOCOL-v2.md`, ADR 0067): binary frames with JSON
//! control messages on channel 0. Pure encoding and decoding, no i/o.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 2;
/// The largest payload of one frame.
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
/// Output credit each terminal channel starts with.
pub const INITIAL_CREDIT: u32 = 256 * 1024;
pub const HEADER: usize = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Data,
    Credit,
    Close,
    Resize,
    Ack,
}

impl Kind {
    pub fn code(self) -> u8 {
        match self {
            Kind::Data => 0,
            Kind::Credit => 1,
            Kind::Close => 2,
            Kind::Resize => 3,
            Kind::Ack => 4,
        }
    }

    pub fn from_code(code: u8) -> Option<Kind> {
        Some(match code {
            0 => Kind::Data,
            1 => Kind::Credit,
            2 => Kind::Close,
            3 => Kind::Resize,
            4 => Kind::Ack,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub channel: u32,
    pub kind: Kind,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    TooLarge(usize),
    UnknownKind(u8),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::TooLarge(n) => write!(f, "frame payload of {n} bytes is over 16 MiB"),
            FrameError::UnknownKind(k) => write!(f, "unknown frame kind {k}"),
        }
    }
}

impl std::error::Error for FrameError {}

impl Frame {
    pub fn new(channel: u32, kind: Kind, payload: Vec<u8>) -> Frame {
        Frame {
            channel,
            kind,
            payload,
        }
    }

    /// A control message on channel 0.
    pub fn control<T: Serialize>(msg: &T) -> Result<Frame, serde_json::Error> {
        Ok(Frame::new(0, Kind::Data, serde_json::to_vec(msg)?))
    }

    /// Terminal output: `[offset u64 LE][bytes]`.
    pub fn output(channel: u32, offset: u64, bytes: &[u8]) -> Frame {
        Frame::new(channel, Kind::Data, with_u64(offset, bytes))
    }

    /// Terminal input: `[seq u64 LE][bytes]`.
    pub fn input(channel: u32, seq: u64, bytes: &[u8]) -> Frame {
        Frame::new(channel, Kind::Data, with_u64(seq, bytes))
    }

    pub fn ack(channel: u32, seq: u64) -> Frame {
        Frame::new(channel, Kind::Ack, seq.to_le_bytes().to_vec())
    }

    pub fn credit(channel: u32, bytes: u32) -> Frame {
        Frame::new(channel, Kind::Credit, bytes.to_le_bytes().to_vec())
    }

    pub fn resize(channel: u32, cols: u16, rows: u16) -> Frame {
        let mut p = cols.to_le_bytes().to_vec();
        p.extend_from_slice(&rows.to_le_bytes());
        Frame::new(channel, Kind::Resize, p)
    }

    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        let len = self.payload.len();
        let len32 = u32::try_from(len)
            .ok()
            .filter(|_| len <= MAX_PAYLOAD)
            .ok_or(FrameError::TooLarge(len))?;
        let mut out = Vec::with_capacity(HEADER + len);
        out.extend_from_slice(&self.channel.to_le_bytes());
        out.push(self.kind.code());
        out.extend_from_slice(&len32.to_le_bytes());
        out.extend_from_slice(&self.payload);
        Ok(out)
    }

    /// Takes one whole frame from the front of `buf`, if it holds one.
    pub fn decode(buf: &mut Vec<u8>) -> Result<Option<Frame>, FrameError> {
        if buf.len() < HEADER {
            return Ok(None);
        }
        let len = u32::from_le_bytes([buf[5], buf[6], buf[7], buf[8]]) as usize;
        if len > MAX_PAYLOAD {
            return Err(FrameError::TooLarge(len));
        }
        let kind = Kind::from_code(buf[4]).ok_or(FrameError::UnknownKind(buf[4]))?;
        if buf.len() < HEADER + len {
            return Ok(None);
        }
        let channel = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let payload = buf[HEADER..HEADER + len].to_vec();
        buf.drain(..HEADER + len);
        Ok(Some(Frame {
            channel,
            kind,
            payload,
        }))
    }

    /// The u64 prefix of an output, input or ack payload, and the rest.
    pub fn split_u64(&self) -> Option<(u64, &[u8])> {
        let (head, rest) = self.payload.split_first_chunk::<8>()?;
        Some((u64::from_le_bytes(*head), rest))
    }

    pub fn as_u32(&self) -> Option<u32> {
        self.payload
            .as_slice()
            .try_into()
            .ok()
            .map(u32::from_le_bytes)
    }

    pub fn as_size(&self) -> Option<(u16, u16)> {
        match self.payload.as_slice() {
            [c0, c1, r0, r1] => Some((
                u16::from_le_bytes([*c0, *c1]),
                u16::from_le_bytes([*r0, *r1]),
            )),
            _ => None,
        }
    }
}

fn with_u64(n: u64, bytes: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(8 + bytes.len());
    p.extend_from_slice(&n.to_le_bytes());
    p.extend_from_slice(bytes);
    p
}

/// What a program reported with OSC 7501.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app: Option<String>,
}

/// A terminal's facts. Never its screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terminal {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub live: bool,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

/// How a program or stream ended; the CLOSE payload from the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub reason: String,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub signal: Option<i64>,
}

/// Client → server on channel 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ClientMsg {
    Hello {
        version: u32,
        client: String,
        #[serde(default)]
        resume: Option<Resume>,
    },
    Terminals {
        id: u64,
    },
    Attach {
        id: u64,
        terminal: String,
        cols: u16,
        rows: u16,
        #[serde(default)]
        history: u32,
    },
    Detach {
        id: u64,
        channel: u32,
    },
    /// Starts a Task's agent on a branch worktree, or finds the one running.
    StartTask {
        id: u64,
        repo: String,
        branch: String,
        #[serde(default)]
        base: Option<String>,
        /// Claude Code starts in plan mode (ADR 0060).
        #[serde(default)]
        plan: bool,
    },
    /// Asks for `changed`, `removed`, `resync` and `inbox` notifications.
    Watch {
        id: u64,
    },
    Inbox {
        id: u64,
    },
    /// Reads a Capture line into fields, for the client to show as chips.
    /// `today` is the device's local date, `YYYY-MM-DD`.
    ParseCapture {
        id: u64,
        text: String,
        today: String,
    },
    InboxAdd {
        id: u64,
        kind: ItemKind,
        title: String,
        #[serde(default)]
        note: Option<String>,
        #[serde(default)]
        workspace: Option<String>,
        #[serde(default)]
        due: Option<String>,
    },
    /// Puts an open item at `index` in the manual order.
    InboxMove {
        id: u64,
        item: String,
        index: usize,
    },
    /// Hides an item until `until` (Unix seconds); it then returns on top.
    InboxLater {
        id: u64,
        item: String,
        until: i64,
    },
    InboxDone {
        id: u64,
        item: String,
    },
    InboxReopen {
        id: u64,
        item: String,
    },
    InboxDelete {
        id: u64,
        item: String,
    },
    /// The Task of this agent terminal is ready for review: one Review item
    /// while it is open.
    TaskReview {
        id: u64,
        terminal: String,
        #[serde(default)]
        note: Option<String>,
    },
}

/// What an Inbox item asks of the user (ADR 0066).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemKind {
    Review,
    Reply,
    Decide,
    Read,
    Todo,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    /// Hidden until then (Unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub later_until: Option<i64>,
    /// Unix seconds.
    pub created: i64,
    /// `capture`, `task`, or `routine`.
    pub source: String,
}

/// The whole Inbox: open items in the user's order, then recent done ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inbox {
    pub revision: u64,
    pub items: Vec<InboxItem>,
    pub done: Vec<InboxItem>,
}

/// A Capture line read into fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    pub kind: ItemKind,
    pub title: String,
    pub workspace: Option<String>,
    pub due: Option<String>,
}

/// `hello.resume`: the session to continue and where each channel stopped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resume {
    pub token: String,
    #[serde(default)]
    pub channels: Vec<ResumeChannel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeChannel {
    pub channel: u32,
    pub terminal: String,
    /// The next output offset the client expects.
    pub offset: u64,
    /// The last input sequence number the client got an ACK for.
    #[serde(default)]
    pub input_seq: u64,
    pub cols: u16,
    pub rows: u16,
    #[serde(default)]
    pub history: u32,
}

/// A channel that continues after a resume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resumed {
    pub channel: u32,
    /// The last input the server applied; `None` when it cannot tell.
    pub input_seq: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    BadRequest,
    Version,
    NotFound,
    Backend,
    Closed,
}

/// Server → client on channel 0.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ServerMsg {
    Welcome {
        version: u32,
        server: String,
        backend: String,
        token: String,
        resumed: Vec<Resumed>,
    },
    Reply {
        id: u64,
        #[serde(flatten)]
        body: Reply,
    },
    Error {
        #[serde(default)]
        id: Option<u64>,
        code: ErrorCode,
        message: String,
    },
    Paint {
        channel: u32,
    },
    /// A terminal started, or its state, size or liveness changed.
    Changed {
        terminal: Terminal,
    },
    Removed {
        terminal: String,
    },
    /// Changes were lost: the client lists the terminals again.
    Resync {},
    /// After `watch`: the Inbox changed.
    Inbox {
        inbox: Inbox,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Reply {
    Terminals { terminals: Vec<Terminal> },
    Attached { channel: u32 },
    Started { terminal: Terminal, existing: bool },
    Inbox { inbox: Inbox },
    Capture { capture: Capture },
    Empty {},
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frames_decode_only_when_whole() {
        let a = Frame::output(3, 42, b"hi").encode().unwrap();
        let b = Frame::ack(3, 7).encode().unwrap();
        let mut buf = a[..5].to_vec();
        assert_eq!(Frame::decode(&mut buf), Ok(None));
        buf.extend_from_slice(&a[5..]);
        buf.extend_from_slice(&b);
        let out = Frame::decode(&mut buf).unwrap().unwrap();
        assert_eq!(out.split_u64(), Some((42, &b"hi"[..])));
        let ack = Frame::decode(&mut buf).unwrap().unwrap();
        assert_eq!((ack.kind, ack.split_u64()), (Kind::Ack, Some((7, &[][..]))));
        assert!(buf.is_empty());
    }

    #[test]
    fn bad_frames_are_errors() {
        let mut big = vec![0, 0, 0, 0, 0];
        big.extend_from_slice(&((MAX_PAYLOAD as u32) + 1).to_le_bytes());
        assert!(matches!(
            Frame::decode(&mut big),
            Err(FrameError::TooLarge(_))
        ));
        let mut unknown = vec![0, 0, 0, 0, 9, 0, 0, 0, 0];
        assert_eq!(Frame::decode(&mut unknown), Err(FrameError::UnknownKind(9)));
    }

    #[test]
    fn control_messages_match_the_spec() {
        let hello: ClientMsg = serde_json::from_value(
            json!({"type": "hello", "version": 2, "client": "muxa-cli", "resume": null}),
        )
        .unwrap();
        assert!(matches!(hello, ClientMsg::Hello { version: 2, .. }));
        let resume: ClientMsg = serde_json::from_value(json!({"type": "hello", "version": 2,
            "client": "c", "resume": {"token": "ab", "channels": [{"channel": 3,
            "terminal": "t7", "offset": 10, "input_seq": 2, "cols": 80, "rows": 24}]}}))
        .unwrap();
        let ClientMsg::Hello {
            resume: Some(resume),
            ..
        } = resume
        else {
            panic!("no resume");
        };
        assert_eq!(
            (resume.channels[0].offset, resume.channels[0].history),
            (10, 0)
        );
        assert_eq!(
            serde_json::to_value(Resumed {
                channel: 3,
                input_seq: None
            })
            .unwrap(),
            json!({"channel": 3, "input_seq": null})
        );
        let attach: ClientMsg = serde_json::from_value(
            json!({"type": "attach", "id": 4, "terminal": "t7", "cols": 80, "rows": 24}),
        )
        .unwrap();
        assert_eq!(
            attach,
            ClientMsg::Attach {
                id: 4,
                terminal: "t7".into(),
                cols: 80,
                rows: 24,
                history: 0
            }
        );
        let reply = ServerMsg::Reply {
            id: 4,
            body: Reply::Attached { channel: 1 },
        };
        assert_eq!(
            serde_json::to_value(&reply).unwrap(),
            json!({"type": "reply", "id": 4, "channel": 1})
        );
        let err = ServerMsg::Error {
            id: Some(4),
            code: ErrorCode::NotFound,
            message: "t7".into(),
        };
        assert_eq!(
            serde_json::to_value(&err).unwrap()["code"],
            json!("not-found")
        );
    }
}
