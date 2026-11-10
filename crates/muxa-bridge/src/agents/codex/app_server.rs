//! A client of Codex's App Server (ADR 0028): one JSON-RPC connection per call,
//! through `codex app-server proxy` to the shared daemon. Blocking. Every
//! failure is an [`Error`]; callers read any error as unknown, never as idle.
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// How long one call may take, from spawning the proxy to its answer.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(15);
/// An answer line longer than this is cut and fails to parse.
const MAX_LINE: u64 = 1 << 20;
/// The rollout format the Codex adapter reads (ADR 0028, Consequences).
const HISTORY_MODE: &str = "legacy";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot start {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("the Codex app server connection was lost")]
    Lost,
    #[error("the Codex app server did not answer within {0:?}")]
    Timeout(Duration),
    /// The server's message is left out: it may echo what was sent.
    #[error("the Codex app server refused {method} with error {code}")]
    Rpc { method: &'static str, code: i64 },
    #[error("the Codex app server answered {method} with no valid {what}")]
    Shape {
        method: &'static str,
        what: &'static str,
    },
    #[error("not a Codex thread id")]
    BadId,
    #[error("codex app-server daemon start failed: {0}")]
    Daemon(String),
}

/// A thread id as Codex prints it: a UUID's characters only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadId(String);

impl ThreadId {
    pub fn parse(s: &str) -> Result<Self, Error> {
        if super::valid_id(s) {
            Ok(Self(s.to_owned()))
        } else {
            Err(Error::BadId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// `thread/read`'s `status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Idle,
    /// `waiting`: the turn waits on an approval or the user's input.
    Active {
        waiting: bool,
    },
    SystemError,
    NotLoaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: ThreadId,
    pub status: Status,
    /// The rollout. It does not exist before the first turn.
    pub path: Option<PathBuf>,
}

/// Why `turn/start` has no result. `uncertain`: the request was written and
/// no answer came, so the turn may have started (ADR 0024: never retried).
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct TurnError {
    pub error: Error,
    pub uncertain: bool,
}

/// Where the App Server is: the `codex` program, its `CODEX_HOME`, and the
/// environment a new thread starts with.
#[derive(Clone)]
pub struct Endpoint {
    program: String,
    home: Option<PathBuf>,
    env: Vec<(String, String)>,
    timeout: Duration,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Keys only: values may be tokens.
        let keys: Vec<&str> = self.env.iter().map(|(k, _)| k.as_str()).collect();
        f.debug_struct("Endpoint")
            .field("program", &self.program)
            .field("home", &self.home)
            .field("env", &keys)
            .finish()
    }
}

impl Endpoint {
    pub fn new(program: impl Into<String>, home: Option<PathBuf>) -> Self {
        Self {
            program: program.into(),
            home,
            env: Vec::new(),
            timeout: CALL_TIMEOUT,
        }
    }

    /// The workspace's agent environment, for the daemon and a new thread.
    #[must_use]
    pub fn with_env(mut self, env: Vec<(String, String)>) -> Self {
        self.env = env;
        self
    }

    #[cfg(test)]
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(test)]
    pub fn program(&self) -> &str {
        &self.program
    }

    #[cfg(test)]
    pub fn home(&self) -> Option<&Path> {
        self.home.as_deref()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(args).envs(self.env.iter().map(|(k, v)| (k, v)));
        if let Some(home) = &self.home {
            cmd.env(super::DIR_ENV, home);
        }
        cmd
    }

    /// Starts the shared daemon unless it runs: the proxy needs it, and the
    /// TUI that would start it runs only after the thread exists.
    pub fn start_daemon(&self) -> Result<(), Error> {
        let mut child = self
            .command(&["app-server", "daemon", "start"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| Error::Spawn {
                program: self.program.clone(),
                source,
            })?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(Error::Daemon(status.to_string())),
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Timeout(self.timeout));
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => return Err(Error::Daemon(e.to_string())),
            }
        }
    }

    /// `thread/start` in `cwd`: a thread with no turn (ADR 0028, fact 4).
    pub fn start_thread(&self, cwd: &Path) -> Result<Thread, Error> {
        const METHOD: &str = "thread/start";
        let mut conn = Connection::open(self)?;
        let params = json!({"cwd": cwd, "historyMode": HISTORY_MODE});
        let result = conn.call(METHOD, params)?;
        thread(METHOD, &result, None)
    }

    pub fn read_thread(&self, id: &ThreadId) -> Result<Thread, Error> {
        const METHOD: &str = "thread/read";
        let mut conn = Connection::open(self)?;
        let result = conn.call(METHOD, json!({"threadId": id.as_str()}))?;
        thread(METHOD, &result, Some(id))
    }

    /// `turn/start` with `text` on thread `id`. A result is a definite send.
    pub fn start_turn(&self, id: &ThreadId, text: &str) -> Result<(), TurnError> {
        const METHOD: &str = "turn/start";
        let not_sent = |error| TurnError {
            error,
            uncertain: false,
        };
        let mut conn = Connection::open(self).map_err(not_sent)?;
        let params = json!({
            "threadId": id.as_str(),
            "input": [{"type": "text", "text": text}],
        });
        conn.write(METHOD, params).map_err(|error| TurnError {
            uncertain: matches!(error, Error::Lost),
            error,
        })?;
        match conn.answer(METHOD) {
            Ok(_) => Ok(()),
            Err(error @ Error::Rpc { .. }) => Err(not_sent(error)),
            Err(error) => Err(TurnError {
                error,
                uncertain: true,
            }),
        }
    }
}

/// The `thread` of a result, checked. `want`: the id that was read.
fn thread(method: &'static str, result: &Value, want: Option<&ThreadId>) -> Result<Thread, Error> {
    let shape = |what| Error::Shape { method, what };
    let t = result.get("thread").ok_or(shape("thread"))?;
    let id = t
        .get("id")
        .and_then(Value::as_str)
        .ok_or(shape("thread id"))
        .and_then(|s| ThreadId::parse(s).map_err(|_| shape("thread id")))?;
    if want.is_some_and(|w| *w != id) {
        return Err(shape("thread id"));
    }
    let status = t
        .get("status")
        .and_then(status)
        .ok_or(shape("thread status"))?;
    let path = t
        .get("path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute());
    Ok(Thread { id, status, path })
}

fn status(v: &Value) -> Option<Status> {
    Some(match v.get("type")?.as_str()? {
        "idle" => Status::Idle,
        "systemError" => Status::SystemError,
        "notLoaded" => Status::NotLoaded,
        "active" => {
            let flags = v.get("activeFlags")?.as_array()?;
            let waiting = flags
                .iter()
                .any(|f| matches!(f.as_str(), Some("waitingOnApproval" | "waitingOnUserInput")));
            Status::Active { waiting }
        }
        _ => return None,
    })
}

/// One proxy process, initialized. Dropping it ends the process.
struct Connection {
    child: Child,
    stdin: ChildStdin,
    lines: mpsc::Receiver<Vec<u8>>,
    next: u64,
    deadline: Instant,
    timeout: Duration,
}

impl Connection {
    fn open(endpoint: &Endpoint) -> Result<Self, Error> {
        let mut child = endpoint
            .command(&["app-server", "proxy"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| Error::Spawn {
                program: endpoint.program.clone(),
                source,
            })?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Lost);
        };
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match (&mut reader).take(MAX_LINE).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => return,
                    Ok(_) => {
                        if tx.send(line).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        let mut conn = Self {
            child,
            stdin,
            lines,
            next: 0,
            deadline: Instant::now() + endpoint.timeout,
            timeout: endpoint.timeout,
        };
        let info = json!({
            "clientInfo": {"name": "muxa", "version": env!("CARGO_PKG_VERSION")},
            // `historyMode` is experimental (ADR 0028, fact 5).
            "capabilities": {"experimentalApi": true},
        });
        conn.call("initialize", info)?;
        conn.send(&json!({"method": "initialized"}))?;
        Ok(conn)
    }

    fn call(&mut self, method: &'static str, params: Value) -> Result<Value, Error> {
        self.write(method, params)?;
        self.answer(method)
    }

    fn write(&mut self, method: &'static str, params: Value) -> Result<(), Error> {
        self.next += 1;
        self.send(&json!({"id": self.next, "method": method, "params": params}))
    }

    fn send(&mut self, msg: &Value) -> Result<(), Error> {
        let mut line = msg.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|_| Error::Lost)
    }

    /// The answer to the last request; notifications and server requests are skipped.
    fn answer(&mut self, method: &'static str) -> Result<Value, Error> {
        loop {
            let left = self.deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(line) => line,
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(Error::Timeout(self.timeout)),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(Error::Lost),
            };
            let Ok(msg) = serde_json::from_slice::<Value>(&line) else {
                return Err(Error::Shape {
                    method,
                    what: "JSON line",
                });
            };
            if msg.get("method").is_some()
                || msg.get("id").and_then(Value::as_u64) != Some(self.next)
            {
                continue;
            }
            if let Some(error) = msg.get("error") {
                let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
                return Err(Error::Rpc { method, code });
            }
            return msg.get("result").cloned().ok_or(Error::Shape {
                method,
                what: "result",
            });
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
#[path = "app_server_tests.rs"]
mod tests;
