//! Sends a record request to the host daemon, one connection per request,
//! and falls back to [`handle`] in this process when no daemon of this
//! version answers (ADR 0019). The only branch between the two paths.

use std::path::Path;
use std::time::Duration;

use muxa_protocol::{ErrorCode, Message, Request};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use super::{Context, handle};
use crate::convert::error_body;
use crate::daemon::socket;

const CONNECT_WAIT: Duration = Duration::from_millis(200);
const HELLO_WAIT: Duration = Duration::from_secs(2);
/// Between two messages of one answer. A `create-task` launches the agent
/// (up to 60 s) before it answers.
const ANSWER_WAIT: Duration = Duration::from_secs(120);

/// What the daemon in a daemon dir can do for this bridge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    /// No socket, or nothing answers on it.
    Absent,
    /// A daemon of another version answers. The bridge does not use it.
    OtherVersion(String),
    Ready,
}

/// Answers a record request through the daemon, or here when none is usable.
pub async fn serve(cx: &Context, daemon_dir: Option<&Path>, req: Request) -> Vec<Message> {
    if let Some(dir) = daemon_dir.filter(|_| super::is_record_request(&req)) {
        if let Some(answer) = forward(dir, &req).await {
            return answer;
        }
    }
    handle(cx, req).await
}

/// A loop request (ADR 0019): only a daemon of this bridge's version runs
/// loops, so there is no fallback. The message says what the host needs.
pub async fn serve_loop(daemon_dir: Option<&Path>, req: Request) -> Vec<Message> {
    let id = req.id();
    let refused = |message: String| {
        vec![Message::Result {
            id: Some(id),
            error: Some(error_body(ErrorCode::Unsupported, message)),
            sessions: None,
        }]
    };
    let absent = match req {
        Request::RunRoutine { .. } => NO_DAEMON_ROUTINES,
        Request::RequestAssistant { .. } | Request::CancelRequest { .. } => {
            crate::assistant::NO_DAEMON
        }
        _ => NO_DAEMON,
    };
    let Some(dir) = daemon_dir else {
        return refused(absent.into());
    };
    match connect(dir).await {
        Some((reader, writer, Probe::Ready)) => send(reader, writer, &req).await,
        Some((_, _, Probe::OtherVersion(v))) => refused(format!(
            "The muxa daemon on the host runs version {v}. Run: muxa-bridge daemon restart"
        )),
        _ => refused(absent.into()),
    }
}

/// Without a usable daemon, the Task Loop is off.
pub const NO_DAEMON: &str =
    "The Task Loop needs the muxa daemon. On the host, run: muxa-bridge daemon install";

/// Without a usable daemon, Routines do not run (ADR 0026 section 2).
pub const NO_DAEMON_ROUTINES: &str =
    "Routines run in the muxa daemon. On the host, run: muxa-bridge daemon install";

/// Whether a daemon of this bridge's version answers in `dir`.
pub async fn probe(dir: &Path) -> Probe {
    match connect(dir).await {
        Some((_, _, probe)) => probe,
        None => Probe::Absent,
    }
}

/// The daemon's answer, or `None` to fall back. Once the request is sent,
/// the daemon may have written, so a broken answer is an error, not a fallback.
async fn forward(dir: &Path, req: &Request) -> Option<Vec<Message>> {
    let (reader, writer, probe) = connect(dir).await?;
    if probe != Probe::Ready {
        return None;
    }
    Some(send(reader, writer, req).await)
}

/// Sends `req` on a connection that said hello, and reads up to its `result`.
async fn send(
    mut reader: BufReader<OwnedReadHalf>,
    mut writer: OwnedWriteHalf,
    req: &Request,
) -> Vec<Message> {
    let id = req.id();
    if let Err(e) = socket::write(&mut writer, req).await {
        tracing::warn!(error = %e, "the muxa daemon closed before the request");
        return vec![stopped(id)];
    }
    let mut out = Vec::new();
    loop {
        let next = tokio::time::timeout(
            ANSWER_WAIT,
            socket::read::<Message, _>(&mut reader, socket::MAX_LINE),
        )
        .await;
        match next {
            Ok(Ok(Some(msg))) => {
                let last = matches!(&msg, Message::Result { id: Some(got), .. } if *got == id);
                out.push(msg);
                if last {
                    return out;
                }
            }
            Ok(Ok(None)) => break,
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "unreadable answer from the muxa daemon");
                break;
            }
            Err(_) => {
                tracing::warn!("the muxa daemon did not answer in time");
                break;
            }
        }
    }
    out.push(stopped(id));
    out
}

fn stopped(id: u64) -> Message {
    Message::Result {
        id: Some(id),
        error: Some(error_body(
            ErrorCode::Internal,
            "the muxa daemon stopped; try again",
        )),
        sessions: None,
    }
}

type Connection = (BufReader<OwnedReadHalf>, OwnedWriteHalf, Probe);

async fn connect(dir: &Path) -> Option<Connection> {
    let path = dir.join(socket::FILE);
    if !path.exists() {
        return None;
    }
    let stream = match tokio::time::timeout(CONNECT_WAIT, UnixStream::connect(&path)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "no muxa daemon; serving task records here");
            return None;
        }
        Err(_) => {
            tracing::warn!("the muxa daemon did not accept in time; serving task records here");
            return None;
        }
    };
    let (read, writer) = stream.into_split();
    let mut reader = BufReader::new(read);
    let hello = tokio::time::timeout(
        HELLO_WAIT,
        socket::read::<Message, _>(&mut reader, socket::MAX_LINE),
    )
    .await;
    let probe = match hello {
        Ok(Ok(Some(hello))) => judge(&hello),
        _ => {
            tracing::warn!("the muxa daemon sent no hello; serving task records here");
            return None;
        }
    };
    if let Probe::OtherVersion(v) = &probe {
        tracing::warn!(daemon = %v, bridge = env!("CARGO_PKG_VERSION"), "the muxa daemon runs another version; serving task records here");
    }
    Some((reader, writer, probe))
}

/// Only a daemon of this bridge's version serves its client, so one client
/// never meets two versions of the handlers.
fn judge(hello: &Message) -> Probe {
    match hello {
        Message::Hello { bridge, .. } if bridge == env!("CARGO_PKG_VERSION") => Probe::Ready,
        Message::Hello { bridge, .. } => Probe::OtherVersion(bridge.clone()),
        _ => Probe::Absent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(version: &str) -> Message {
        Message::Hello {
            protocol: muxa_protocol::VERSION,
            bridge: version.into(),
            tmux: String::new(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn only_a_daemon_of_the_same_version_is_used() {
        assert_eq!(judge(&hello(env!("CARGO_PKG_VERSION"))), Probe::Ready);
        assert_eq!(
            judge(&hello("0.0.0-old")),
            Probe::OtherVersion("0.0.0-old".into())
        );
        assert_eq!(judge(&Message::SessionsChanged), Probe::Absent);
    }

    #[tokio::test]
    async fn no_socket_is_absent() {
        let dir = std::env::temp_dir().join(format!("muxa-remote-{}", std::process::id()));
        assert_eq!(probe(&dir).await, Probe::Absent);
    }
}
