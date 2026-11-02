//! The host daemon as a child process on a test tmux server, and a client
//! for its socket. Its dir is short and under `/tmp`: socket paths are
//! limited to 103 bytes.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use muxa_protocol::Message;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::process::Child;

use super::{Server, WAIT};

static NEXT: AtomicU32 = AtomicU32::new(0);

pub struct Daemon {
    pub child: Child,
    pub dir: PathBuf,
}

/// A fresh short daemon dir, removed by the `Daemon` that uses it.
pub fn short_dir() -> PathBuf {
    let dir = PathBuf::from(format!(
        "/tmp/mxd-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Runs `muxa-bridge daemon run` on `server`, with `extra` flags.
pub fn spawn(server: &Server, dir: &Path, extra: &[&str]) -> Child {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_muxa-bridge"))
        .args(["daemon", "run", "--socket", &server.socket, "--daemon-dir"])
        .arg(dir)
        .args(extra)
        .env_remove("TMUX")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("start the daemon")
}

impl Daemon {
    /// Starts a daemon in `dir` and waits until it answers on its socket.
    pub async fn start_in(server: &Server, dir: PathBuf, extra: &[&str]) -> Daemon {
        let child = spawn(server, &dir, extra);
        let daemon = Daemon { child, dir };
        for _ in 0..100 {
            if UnixStream::connect(daemon.socket()).await.is_ok() {
                return daemon;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the daemon did not listen: {}", daemon.log());
    }

    pub async fn start(server: &Server, extra: &[&str]) -> Daemon {
        Self::start_in(server, short_dir(), extra).await
    }

    pub fn socket(&self) -> PathBuf {
        self.dir.join("daemon.sock")
    }

    pub fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("daemon.log")).unwrap_or_default()
    }

    pub fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).expect("stat").permissions().mode() & 0o777
    }

    pub async fn connect(&self) -> Conn {
        Conn::open(&self.socket()).await
    }

    /// Sends `shutdown` and waits for the daemon to exit.
    pub async fn shutdown(&mut self) -> ExitStatus {
        let mut conn = self.connect().await;
        let answer = conn.request(serde_json::json!({"type": "shutdown"})).await;
        assert!(
            matches!(answer.last(), Some(Message::Result { error: None, .. })),
            "{answer:?}"
        );
        self.wait().await
    }

    pub async fn wait(&mut self) -> ExitStatus {
        tokio::time::timeout(WAIT, self.child.wait())
            .await
            .expect("the daemon did not exit")
            .expect("wait")
    }

    /// Kills the daemon as a crash would: no shutdown, the socket stays.
    pub async fn crash(&mut self) {
        self.child.kill().await.expect("kill");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// One connection to the daemon's socket, after its `hello`.
pub struct Conn {
    pub hello: Message,
    lines: Lines<BufReader<OwnedReadHalf>>,
    write: OwnedWriteHalf,
    next_id: u64,
}

impl Conn {
    pub async fn open(socket: &Path) -> Conn {
        let stream = UnixStream::connect(socket).await.expect("connect");
        let (read, write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let hello = read_message(&mut lines).await.expect("hello");
        Conn {
            hello,
            lines,
            write,
            next_id: 0,
        }
    }

    /// Sends `req` with a fresh id; returns every message up to its `result`.
    pub async fn request(&mut self, mut req: Value) -> Vec<Message> {
        self.next_id += 1;
        let id = self.next_id;
        req["id"] = serde_json::json!(id);
        let mut line = req.to_string();
        line.push('\n');
        self.write.write_all(line.as_bytes()).await.expect("write");
        let mut out = Vec::new();
        loop {
            let msg = read_message(&mut self.lines)
                .await
                .expect("the daemon closed before the result");
            let last = matches!(&msg, Message::Result { id: Some(got), .. } if *got == id);
            out.push(msg);
            if last {
                return out;
            }
        }
    }
}

async fn read_message(lines: &mut Lines<BufReader<OwnedReadHalf>>) -> Option<Message> {
    let line = tokio::time::timeout(Duration::from_secs(30), lines.next_line())
        .await
        .ok()?
        .ok()??;
    Some(serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad message {line}: {e}")))
}
