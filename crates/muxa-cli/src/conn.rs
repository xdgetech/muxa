//! A connection to a bridge process: runs the bridge command, checks hello,
//! and exchanges protocol lines.

use std::process::Stdio;
use std::time::Duration;

use muxa_protocol::{ErrorBody, Message, Request, Session};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

const WAIT: Duration = Duration::from_secs(10);

pub struct Conn {
    _child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    capabilities: Vec<String>,
}

impl Conn {
    /// Runs `command` with `sh -c`, e.g. `ssh host ~/.local/bin/muxa-bridge`.
    pub async fn start(command: &str) -> Result<(Conn, String), String> {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("cannot run {command:?}: {e}"))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let lines = BufReader::new(child.stdout.take().ok_or("no stdout")?).lines();
        let mut conn = Conn {
            _child: child,
            stdin,
            lines,
            next_id: 0,
            capabilities: Vec::new(),
        };
        match conn.recv().await? {
            Message::Hello {
                protocol,
                tmux,
                capabilities,
                ..
            } if protocol == muxa_protocol::VERSION => {
                conn.capabilities = capabilities;
                Ok((conn, tmux))
            }
            Message::Hello { protocol, .. } => Err(format!(
                "bridge speaks protocol {protocol}, this client {}",
                muxa_protocol::VERSION
            )),
            Message::Bye { reason } => Err(format!("bridge exited: {reason}")),
            other => Err(format!("expected hello, got {other:?}")),
        }
    }

    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|c| c == capability)
    }

    pub fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub async fn send(&mut self, req: &Request) -> Result<(), String> {
        let mut line = serde_json::to_vec(req).map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.stdin
            .write_all(&line)
            .await
            .map_err(|e| format!("bridge closed: {e}"))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| format!("bridge closed: {e}"))
    }

    pub async fn recv(&mut self) -> Result<Message, String> {
        self.recv_within(WAIT).await
    }

    pub async fn recv_within(&mut self, wait: Duration) -> Result<Message, String> {
        let line = tokio::time::timeout(wait, self.lines.next_line())
            .await
            .map_err(|_| "the bridge did not answer".to_string())?
            .map_err(|e| e.to_string())?
            .ok_or("the bridge closed the connection")?;
        serde_json::from_str(&line).map_err(|e| format!("bad message from the bridge: {e}"))
    }

    /// Like `recv`, without a timeout: for the watch loop.
    pub async fn recv_forever(&mut self) -> Result<Message, String> {
        let line = self
            .lines
            .next_line()
            .await
            .map_err(|e| e.to_string())?
            .ok_or("the bridge closed the connection")?;
        serde_json::from_str(&line).map_err(|e| format!("bad message from the bridge: {e}"))
    }

    /// Sends the request made with a fresh id, and returns the messages sent
    /// before its `result`. `wait` bounds each message.
    pub async fn call(
        &mut self,
        make: impl FnOnce(u64) -> Request,
        wait: Duration,
    ) -> Result<Vec<Message>, String> {
        let id = self.id();
        self.send(&make(id)).await?;
        let mut before = Vec::new();
        loop {
            match self.recv_within(wait).await? {
                Message::Result {
                    id: Some(got),
                    error,
                    ..
                } if got == id => {
                    return match error {
                        Some(e) => Err(failure(&e, &before)),
                        None => Ok(before),
                    };
                }
                Message::Bye { reason } => return Err(format!("bridge exited: {reason}")),
                msg => before.push(msg),
            }
        }
    }

    /// Closes the bridge's input and waits up to `wait` for it to exit, so
    /// work it still runs for this client can finish. Kills it after that.
    pub async fn finish(self, wait: Duration) {
        let Conn {
            _child: mut child,
            stdin,
            mut lines,
            ..
        } = self;
        drop(stdin);
        // Read on, so the bridge never blocks on a full pipe meanwhile.
        let exited = tokio::time::timeout(wait, async {
            while let Ok(Some(_)) = lines.next_line().await {}
            child.wait().await
        })
        .await;
        if exited.is_err() {
            let _ = child.kill().await;
        }
    }

    pub async fn list(&mut self) -> Result<Vec<Session>, String> {
        let id = self.id();
        self.send(&Request::List { id }).await?;
        loop {
            match self.recv().await? {
                Message::Result {
                    id: Some(got),
                    error,
                    sessions,
                } if got == id => {
                    return match (error, sessions) {
                        (Some(e), _) => Err(e.message),
                        (None, s) => Ok(s.unwrap_or_default()),
                    };
                }
                Message::Bye { reason } => return Err(format!("bridge exited: {reason}")),
                _ => {}
            }
        }
    }
}

/// An error, and what the bridge kept anyway, e.g. a task record after a failed launch.
fn failure(e: &ErrorBody, before: &[Message]) -> String {
    let kept = before.iter().find_map(|m| match m {
        Message::TaskSaved { record, .. } => Some(record.id.as_str()),
        _ => None,
    });
    // The wire name, e.g. `task-failed`.
    let code = serde_json::to_value(e.code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    match kept {
        Some(id) => format!("{code}: {} (task {id} was kept)", e.message),
        None => format!("{code}: {}", e.message),
    }
}
