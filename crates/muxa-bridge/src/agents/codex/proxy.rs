//! `codex app-server proxy` relays raw bytes to the daemon's control socket,
//! and that socket speaks WebSocket (`docs/findings/codex-app-server.md`).
//! This runs the WebSocket client over the proxy's stdin and stdout.
use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::protocol::WebSocketConfig;
use tungstenite::{HandshakeError, Message, WebSocket};

use super::app_server::Error;

/// The daemon's limit when its 101 answer does not name one (Codex 0.167.0).
const DEFAULT_MAX_MESSAGE: usize = 16 << 20;
/// The 101 answer's header with the largest message the daemon takes.
const MAX_MESSAGE_HEADER: &str = "x-codex-websocket-max-unfragmented-message-bytes";
/// The proxy forwards bytes to a Unix socket, so the host is a placeholder.
const URL: &str = "ws://localhost/";
const READ_CHUNK: usize = 64 << 10;

/// One proxy process with an open WebSocket. Dropping it ends the process.
pub struct Proxy {
    child: Child,
    socket: WebSocket<Pipes>,
    /// The largest text message the daemon takes, in bytes.
    max_message: usize,
    timeout: Duration,
}

impl Proxy {
    /// Spawns `cmd` (the proxy) and opens the WebSocket. Every read of this
    /// connection ends at `deadline`.
    pub fn open(
        mut cmd: Command,
        program: &str,
        deadline: Instant,
        timeout: Duration,
    ) -> Result<Self, Error> {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| Error::Spawn {
                program: program.to_owned(),
                source,
            })?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            end(&mut child);
            return Err(Error::Lost);
        };
        match handshake(Pipes::new(stdin, stdout, deadline), timeout) {
            Ok((socket, max_message)) => Ok(Self {
                child,
                socket,
                max_message,
                timeout,
            }),
            Err(e) => {
                end(&mut child);
                Err(e)
            }
        }
    }

    /// Sends one JSON-RPC message as one text message. A message over the
    /// daemon's limit is refused before anything is written.
    pub fn send(&mut self, method: &'static str, text: String) -> Result<(), Error> {
        if text.len() > self.max_message {
            return Err(Error::TooLarge {
                method,
                bytes: text.len(),
                limit: self.max_message,
            });
        }
        self.socket
            .send(Message::text(text))
            .map_err(|e| self.error(e))
    }

    /// The next text message. Pings are answered by the socket itself.
    pub fn recv(&mut self, method: &'static str) -> Result<Vec<u8>, Error> {
        loop {
            match self.socket.read() {
                Ok(Message::Text(text)) => return Ok(text.as_bytes().to_vec()),
                Ok(Message::Binary(_)) => {
                    return Err(Error::Shape {
                        method,
                        what: "text message",
                    });
                }
                Ok(Message::Close(_)) => return Err(Error::Lost),
                Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_)) => {}
                Err(tungstenite::Error::Capacity(_)) => {
                    return Err(Error::Shape {
                        method,
                        what: "message within the size limit",
                    });
                }
                Err(e) => return Err(self.error(e)),
            }
        }
    }

    /// Reads from now on end at `deadline`.
    pub fn set_deadline(&mut self, deadline: Instant) {
        self.socket.get_mut().deadline = deadline;
    }

    fn error(&self, e: tungstenite::Error) -> Error {
        match io_kind(&e) {
            Some(io::ErrorKind::TimedOut) => Error::Timeout(self.timeout),
            _ => Error::Lost,
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        end(&mut self.child);
    }
}

/// The open socket and the daemon's message limit from its 101 answer.
fn handshake(pipes: Pipes, timeout: Duration) -> Result<(WebSocket<Pipes>, usize), Error> {
    let config = WebSocketConfig::default()
        .max_message_size(Some(DEFAULT_MAX_MESSAGE))
        .max_frame_size(Some(DEFAULT_MAX_MESSAGE));
    let request = URL
        .into_client_request()
        .map_err(|e| Error::Handshake(e.to_string()))?;
    let (socket, response) =
        match tungstenite::client::client_with_config(request, pipes, Some(config)) {
            Ok(done) => done,
            Err(HandshakeError::Failure(e)) => {
                return Err(match io_kind(&e) {
                    Some(io::ErrorKind::TimedOut) => Error::Timeout(timeout),
                    Some(_) => Error::Lost,
                    None => Error::Handshake(e.to_string()),
                });
            }
            // Only a non-blocking stream interrupts; these pipes block.
            Err(HandshakeError::Interrupted(_)) => return Err(Error::Lost),
        };
    let max_message = response
        .headers()
        .get(MAX_MESSAGE_HEADER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_MAX_MESSAGE);
    Ok((socket, max_message))
}

fn end(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn io_kind(e: &tungstenite::Error) -> Option<io::ErrorKind> {
    match e {
        tungstenite::Error::Io(e) => Some(e.kind()),
        tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed => {
            Some(io::ErrorKind::UnexpectedEof)
        }
        _ => None,
    }
}

/// The proxy's stdin and stdout as one stream. A thread reads stdout, so a
/// read can stop at the deadline: it then fails with `TimedOut`.
struct Pipes {
    stdin: ChildStdin,
    chunks: mpsc::Receiver<Vec<u8>>,
    pending: Vec<u8>,
    at: usize,
    deadline: Instant,
}

impl Pipes {
    fn new(stdin: ChildStdin, mut stdout: ChildStdout, deadline: Instant) -> Self {
        let (tx, chunks) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = vec![0; READ_CHUNK];
            loop {
                match stdout.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        if tx.send(buf[..n].to_vec()).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Self {
            stdin,
            chunks,
            pending: Vec::new(),
            at: 0,
            deadline,
        }
    }
}

impl Read for Pipes {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.at == self.pending.len() {
            let left = self.deadline.saturating_duration_since(Instant::now());
            match self.chunks.recv_timeout(left) {
                Ok(chunk) => {
                    self.pending = chunk;
                    self.at = 0;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                // The proxy closed its stdout: end of stream.
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(0),
            }
        }
        let n = buf.len().min(self.pending.len() - self.at);
        buf[..n].copy_from_slice(&self.pending[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}

impl Write for Pipes {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdin.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdin.flush()
    }
}
