//! One attached terminal: forwards output within the client's credit, applies
//! input once per sequence number, and keeps running while its client is away
//! so that a resume sends only what the client missed.

use std::time::Duration;

use muxa_backend::{Exit, Size, StreamEvent, TerminalStream};
use muxa_protocol::v2::{self, Frame, INITIAL_CREDIT, Kind};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use crate::ring::Ring;

/// The client this attachment sends to.
pub(crate) struct Link {
    pub out: mpsc::UnboundedSender<Frame>,
    /// Told the channel when the attachment ends.
    pub ended: mpsc::UnboundedSender<u32>,
}

/// The answer to [`Cmd::Check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Checked {
    /// The buffer covers the client's offset; the last input applied.
    Resume(u64),
    /// The client needs a fresh screen; the last input applied.
    Repaint(u64),
}

pub(crate) enum Cmd {
    Input {
        seq: u64,
        bytes: Vec<u8>,
    },
    Resize(Size),
    Credit(u32),
    Detach,
    /// The client went away: keep reading into the buffer for a while.
    Park,
    /// A returning client expects output from `offset`. A parked attachment
    /// keeps that output until `Resume`; one that cannot ends after replying.
    Check {
        offset: u64,
        reply: oneshot::Sender<Checked>,
    },
    Resume(Link),
}

enum Flow {
    Go,
    Stop,
}

pub(crate) struct Attachment<S> {
    channel: u32,
    /// `None` once the buffer lost output a parked client never got.
    stream: Option<S>,
    ring: Ring,
    credit: u64,
    last_seq: u64,
    link: Option<Link>,
    /// How the program ended, kept until the client has the output before it.
    exit: Option<Exit>,
    park_for: Duration,
    parked_until: Option<Instant>,
}

impl<S: TerminalStream> Attachment<S> {
    pub(crate) fn new(
        channel: u32,
        stream: S,
        last_seq: u64,
        link: Link,
        park_for: Duration,
    ) -> Self {
        Attachment {
            channel,
            stream: Some(stream),
            ring: Ring::new(),
            credit: u64::from(INITIAL_CREDIT),
            last_seq,
            link: Some(link),
            exit: None,
            park_for,
            parked_until: None,
        }
    }

    /// Runs until the client detaches, the program ends and the client has
    /// seen it, or the parked time runs out.
    pub(crate) async fn run(mut self, mut cmds: mpsc::UnboundedReceiver<Cmd>) {
        loop {
            if self.link.is_some() && self.ring.unsent() > 0 {
                if self.credit > 0 {
                    self.send_output();
                    continue;
                }
            } else if self.link.is_some() && self.exit.is_some() {
                self.close();
                break;
            }
            let reading = self.stream.is_some()
                && self.exit.is_none()
                && (self.link.is_none() || (self.credit > 0 && self.ring.unsent() == 0));
            let flow = tokio::select! {
                cmd = cmds.recv() => self.command(cmd),
                event = next(&mut self.stream), if reading => self.event(event),
                () = until(self.parked_until), if self.parked_until.is_some() => {
                    tracing::info!(channel = self.channel, "parked attachment expired");
                    Flow::Stop
                }
            };
            if matches!(flow, Flow::Stop) {
                break;
            }
        }
        if let Some(s) = self.stream.take() {
            let _ = s.detach();
        }
        if let Some(link) = self.link {
            let _ = link.ended.send(self.channel);
        }
    }

    fn command(&mut self, cmd: Option<Cmd>) -> Flow {
        match cmd {
            Some(Cmd::Input { seq, bytes }) => self.input(seq, &bytes),
            Some(Cmd::Resize(size)) => {
                if let Some(Err(e)) = self.stream.as_ref().map(|s| s.resize(size)) {
                    tracing::info!(channel = self.channel, error = %e, "resize not applied");
                }
                Flow::Go
            }
            Some(Cmd::Credit(n)) => {
                self.credit = self.credit.saturating_add(u64::from(n));
                Flow::Go
            }
            Some(Cmd::Detach) | None => {
                self.send(Frame::new(self.channel, Kind::Close, Vec::new()));
                Flow::Stop
            }
            Some(Cmd::Park) => {
                self.link = None;
                self.parked_until = Some(Instant::now() + self.park_for);
                self.ring.unpin();
                Flow::Go
            }
            Some(Cmd::Check { offset, reply }) => self.check(offset, reply),
            Some(Cmd::Resume(link)) => {
                self.link = Some(link);
                self.parked_until = None;
                self.credit = u64::from(INITIAL_CREDIT);
                self.ring.unpin();
                Flow::Go
            }
        }
    }

    fn check(&mut self, offset: u64, reply: oneshot::Sender<Checked>) -> Flow {
        if self.link.is_none() && self.stream.is_some() && self.ring.rewind(offset) {
            let _ = reply.send(Checked::Resume(self.last_seq));
            return Flow::Go;
        }
        let _ = reply.send(Checked::Repaint(self.last_seq));
        Flow::Stop
    }

    fn input(&mut self, seq: u64, bytes: &[u8]) -> Flow {
        if seq > self.last_seq {
            let Some(stream) = self.stream.as_ref() else {
                return Flow::Go;
            };
            if let Err(e) = stream.input(bytes) {
                tracing::info!(channel = self.channel, error = %e, "input not applied");
                self.exit = Some(gone("closed"));
                return Flow::Go;
            }
            self.last_seq = seq;
        }
        self.send(Frame::ack(self.channel, seq));
        Flow::Go
    }

    fn event(&mut self, event: Option<StreamEvent>) -> Flow {
        match event {
            Some(StreamEvent::Output(bytes)) => {
                self.ring.push(&bytes);
                if !self.ring.trim(self.link.is_some()) {
                    // The client can only get a fresh screen now; free the
                    // viewer and keep the input count for the resume.
                    tracing::info!(channel = self.channel, "parked output over the buffer");
                    if let Some(s) = self.stream.take() {
                        let _ = s.detach();
                    }
                }
            }
            Some(StreamEvent::Resized(_)) => {}
            Some(StreamEvent::Closed(exit)) => self.exit = Some(exit),
            None => self.exit = Some(gone("connection_lost")),
        }
        Flow::Go
    }

    fn send_output(&mut self) {
        let max = usize::try_from(self.credit)
            .unwrap_or(usize::MAX)
            .min(v2::MAX_PAYLOAD - 8);
        let (offset, bytes) = self.ring.take(max);
        self.credit -= bytes.len() as u64;
        self.send(Frame::output(self.channel, offset, &bytes));
    }

    fn close(&mut self) {
        let Some(exit) = self.exit.as_ref() else {
            return;
        };
        let body = v2::Exit {
            reason: exit.reason.clone(),
            exit_code: exit.code,
            signal: exit.signal,
        };
        let payload = serde_json::to_vec(&body).unwrap_or_default();
        self.send(Frame::new(self.channel, Kind::Close, payload));
    }

    fn send(&self, frame: Frame) {
        if let Some(link) = &self.link {
            let _ = link.out.send(frame);
        }
    }
}

async fn next<S: TerminalStream>(stream: &mut Option<S>) -> Option<StreamEvent> {
    match stream {
        Some(s) => s.next().await,
        None => std::future::pending().await,
    }
}

async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(d) => tokio::time::sleep_until(d).await,
        None => std::future::pending().await,
    }
}

fn gone(reason: &str) -> Exit {
    Exit {
        reason: reason.into(),
        code: None,
        signal: None,
    }
}
