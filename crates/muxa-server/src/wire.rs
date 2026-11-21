//! Frame i/o: a reader task that decodes frames, and a writer task that
//! encodes them, so the session can select on plain channels.

use muxa_protocol::v2::{Frame, FrameError};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;

const READ_CHUNK: usize = 64 * 1024;

#[derive(Debug)]
pub(crate) enum Incoming {
    Frame(Frame),
    /// A bad frame or a read error; the connection is over.
    Broken(String),
}

pub(crate) fn spawn_reader<R>(mut reader: R) -> mpsc::Receiver<Incoming>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    // A small queue: a client that floods requests waits on the socket.
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(async move {
        let mut buf = Vec::new();
        let mut chunk = vec![0u8; READ_CHUNK];
        loop {
            loop {
                match Frame::decode(&mut buf) {
                    Ok(Some(frame)) => {
                        if tx.send(Incoming::Frame(frame)).await.is_err() {
                            return;
                        }
                    }
                    Ok(None) => break,
                    Err(e @ (FrameError::TooLarge(_) | FrameError::UnknownKind(_))) => {
                        let _ = tx.send(Incoming::Broken(e.to_string())).await;
                        return;
                    }
                }
            }
            match reader.read(&mut chunk).await {
                Ok(0) => return,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(e) => {
                    let _ = tx.send(Incoming::Broken(e.to_string())).await;
                    return;
                }
            }
        }
    });
    rx
}

/// Frames queued for the client. Unbounded: output is bounded by each
/// channel's credit, and control replies by the client's own requests.
pub(crate) fn spawn_writer<W>(mut writer: W) -> mpsc::UnboundedSender<Frame>
where
    W: AsyncWrite + Unpin + Send + 'static,
{
    let (tx, mut rx) = mpsc::unbounded_channel::<Frame>();
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            let bytes = match frame.encode() {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(channel = frame.channel, error = %e, "frame not sent");
                    continue;
                }
            };
            if writer.write_all(&bytes).await.is_err() {
                return;
            }
            // Flush when nothing else is queued, so frames go out in batches.
            if rx.is_empty() && writer.flush().await.is_err() {
                return;
            }
        }
        let _ = writer.shutdown().await;
    });
    tx
}
