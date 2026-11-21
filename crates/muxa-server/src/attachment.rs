//! One attached terminal: forwards output within the client's credit, and
//! applies input once per sequence number.

use muxa_backend::{Exit, Size, StreamEvent, TerminalStream};
use muxa_protocol::v2::{self, Frame, INITIAL_CREDIT, Kind};
use tokio::sync::mpsc;

#[derive(Debug)]
pub(crate) enum Cmd {
    Input { seq: u64, bytes: Vec<u8> },
    Resize(Size),
    Credit(u32),
    Detach,
}

/// Runs until the stream ends or the client detaches, then reports `channel`
/// on `ended`.
pub(crate) async fn run<S: TerminalStream>(
    channel: u32,
    mut stream: S,
    mut cmds: mpsc::UnboundedReceiver<Cmd>,
    out: mpsc::UnboundedSender<Frame>,
    ended: mpsc::UnboundedSender<u32>,
) {
    let mut credit = u64::from(INITIAL_CREDIT);
    let mut offset: u64 = 0;
    let mut last_seq: u64 = 0;
    // Output read from the backend that did not fit the credit.
    let mut held: Vec<u8> = Vec::new();

    loop {
        if credit > 0 && !held.is_empty() {
            send_output(channel, &out, &mut held, &mut credit, &mut offset);
            continue;
        }
        tokio::select! {
            cmd = cmds.recv() => match cmd {
                Some(Cmd::Input { seq, bytes }) => {
                    if seq > last_seq {
                        if let Err(e) = stream.input(&bytes) {
                            tracing::info!(channel, error = %e, "input not applied");
                            close(channel, &out, &Exit { reason: "closed".into(), code: None, signal: None });
                            break;
                        }
                        last_seq = seq;
                    }
                    let _ = out.send(Frame::ack(channel, seq));
                }
                Some(Cmd::Resize(size)) => {
                    if let Err(e) = stream.resize(size) {
                        tracing::info!(channel, error = %e, "resize not applied");
                    }
                }
                Some(Cmd::Credit(n)) => credit = credit.saturating_add(u64::from(n)),
                Some(Cmd::Detach) | None => {
                    let _ = stream.detach();
                    let _ = out.send(Frame::new(channel, Kind::Close, Vec::new()));
                    break;
                }
            },
            event = stream.next(), if credit > 0 && held.is_empty() => match event {
                Some(StreamEvent::Output(bytes)) if bytes.is_empty() => {}
                Some(StreamEvent::Output(bytes)) => {
                    held = bytes;
                    send_output(channel, &out, &mut held, &mut credit, &mut offset);
                }
                Some(StreamEvent::Resized(_)) => {}
                Some(StreamEvent::Closed(exit)) => {
                    close(channel, &out, &exit);
                    break;
                }
                None => {
                    close(channel, &out, &Exit { reason: "connection_lost".into(), code: None, signal: None });
                    break;
                }
            },
        }
    }
    let _ = ended.send(channel);
}

/// Sends as much of `held` as the credit allows.
fn send_output(
    channel: u32,
    out: &mpsc::UnboundedSender<Frame>,
    held: &mut Vec<u8>,
    credit: &mut u64,
    offset: &mut u64,
) {
    let max = usize::try_from(*credit)
        .unwrap_or(usize::MAX)
        .min(v2::MAX_PAYLOAD - 8);
    let n = held.len().min(max);
    let _ = out.send(Frame::output(channel, *offset, &held[..n]));
    held.drain(..n);
    *offset += n as u64;
    *credit -= n as u64;
}

fn close(channel: u32, out: &mpsc::UnboundedSender<Frame>, exit: &Exit) {
    let body = v2::Exit {
        reason: exit.reason.clone(),
        exit_code: exit.code,
        signal: exit.signal,
    };
    let payload = serde_json::to_vec(&body).unwrap_or_default();
    let _ = out.send(Frame::new(channel, Kind::Close, payload));
}
