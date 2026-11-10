//! A client of Codex's App Server (ADR 0028): one JSON-RPC connection per call,
//! through `codex app-server proxy` to the shared daemon, one message per
//! WebSocket text message. Blocking. Every failure is an [`Error`]; callers
//! read any error as unknown, never as idle.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::proxy::Proxy;

/// How long one call may take, from spawning the proxy to its answer.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(15);
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
    /// No WebSocket: for example the proxy found no daemon to relay to.
    #[error("the Codex app server WebSocket handshake failed: {0}")]
    Handshake(String),
    #[error("{method} is {bytes} bytes; the Codex app server takes at most {limit}")]
    TooLarge {
        method: &'static str,
        bytes: usize,
        limit: usize,
    },
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
    /// After `thread/name/set` the rollout must exist (ADR 0028, fact 10).
    #[error("the Codex thread has no rollout on disk")]
    NotSaved,
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

    /// One open connection. A thread with no turn stays loaded only while
    /// a connection holds it (ADR 0028, fact 9).
    pub fn session(&self) -> Result<Session, Error> {
        Connection::open(self).map(|conn| Session { conn })
    }

    /// `thread/start` in `cwd` on its own connection. With no turn and no
    /// rollout the daemon drops it soon after the connection closes (fact 9),
    /// so runs use [`Self::start_saved_thread`].
    #[cfg(test)]
    pub fn start_thread(&self, cwd: &Path) -> Result<Thread, Error> {
        self.session()?.start_thread(cwd)
    }

    /// A thread in `cwd` that `codex resume` can open: `thread/start`, then
    /// `thread/name/set` on the same connection, which writes the rollout
    /// (fact 10). Fails unless the rollout exists afterwards.
    pub fn start_saved_thread(&self, cwd: &Path, name: &str) -> Result<Thread, Error> {
        let mut session = self.session()?;
        let thread = session.start_thread(cwd)?;
        session.name_thread(&thread.id, name)?;
        if !thread.path.as_deref().is_some_and(Path::is_file) {
            return Err(Error::NotSaved);
        }
        Ok(thread)
    }

    pub fn read_thread(&self, id: &ThreadId) -> Result<Thread, Error> {
        self.session()?.read_thread(id)
    }

    /// `turn/start` with `text` on thread `id`. A result is a definite send.
    pub fn start_turn(&self, id: &ThreadId, text: &str) -> Result<(), TurnError> {
        let mut session = self.session().map_err(|error| TurnError {
            error,
            uncertain: false,
        })?;
        session.start_turn(id, text)
    }
}

/// An initialized connection that stays open until it is dropped. Each
/// call has the endpoint's timeout.
pub struct Session {
    conn: Connection,
}

impl Session {
    /// `thread/start` in `cwd`: a thread with no turn (ADR 0028, fact 4).
    pub fn start_thread(&mut self, cwd: &Path) -> Result<Thread, Error> {
        const METHOD: &str = "thread/start";
        let params = json!({"cwd": cwd, "historyMode": HISTORY_MODE});
        let result = self.conn.call(METHOD, params)?;
        thread(METHOD, &result, None)
    }

    pub fn read_thread(&mut self, id: &ThreadId) -> Result<Thread, Error> {
        const METHOD: &str = "thread/read";
        let result = self.conn.call(METHOD, json!({"threadId": id.as_str()}))?;
        thread(METHOD, &result, Some(id))
    }

    /// `thread/name/set`. The TUI shows the name.
    pub fn name_thread(&mut self, id: &ThreadId, name: &str) -> Result<(), Error> {
        const METHOD: &str = "thread/name/set";
        self.conn
            .call(METHOD, json!({"threadId": id.as_str(), "name": name}))
            .map(drop)
    }

    /// `turn/start` with `text` on thread `id`. A result is a definite send.
    pub fn start_turn(&mut self, id: &ThreadId, text: &str) -> Result<(), TurnError> {
        const METHOD: &str = "turn/start";
        let params = json!({
            "threadId": id.as_str(),
            "input": [{"type": "text", "text": text}],
        });
        self.conn
            .write(METHOD, METHOD, params)
            .map_err(|error| TurnError {
                uncertain: matches!(error, Error::Lost),
                error,
            })?;
        match self.conn.answer(METHOD) {
            Ok(_) => Ok(()),
            Err(error @ Error::Rpc { .. }) => Err(TurnError {
                error,
                uncertain: false,
            }),
            Err(error) => Err(TurnError {
                error,
                uncertain: true,
            }),
        }
    }

    /// One request; the whole response, result or error. For the
    /// developer's checks (`muxa-bridge codex-rpc`), not for runs.
    pub fn request(&mut self, method: &str, params: Value) -> Result<Value, Error> {
        const LABEL: &str = "the request";
        self.conn.write(LABEL, method, params)?;
        self.conn.response(LABEL)
    }

    /// The next notification or server request, or None when `until` comes
    /// first. Server requests are never answered.
    pub fn next_event(&mut self, until: Instant) -> Result<Option<Value>, Error> {
        const LABEL: &str = "an event";
        self.conn.proxy.set_deadline(until);
        loop {
            let text = match self.conn.proxy.recv(LABEL) {
                Ok(text) => text,
                Err(Error::Timeout(_)) => return Ok(None),
                Err(e) => return Err(e),
            };
            let msg = serde_json::from_slice::<Value>(&text).map_err(|_| Error::Shape {
                method: LABEL,
                what: "JSON message",
            })?;
            if msg.get("method").is_some() {
                return Ok(Some(msg));
            }
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

/// One proxy connection, initialized. Dropping it ends the proxy.
struct Connection {
    proxy: Proxy,
    next: u64,
    timeout: Duration,
}

impl Connection {
    fn open(endpoint: &Endpoint) -> Result<Self, Error> {
        let proxy = Proxy::open(
            endpoint.command(&["app-server", "proxy"]),
            &endpoint.program,
            Instant::now() + endpoint.timeout,
            endpoint.timeout,
        )?;
        let mut conn = Self {
            proxy,
            next: 0,
            timeout: endpoint.timeout,
        };
        let info = json!({
            "clientInfo": {"name": "muxa", "version": env!("CARGO_PKG_VERSION")},
            // `historyMode` is experimental (ADR 0028, fact 5).
            "capabilities": {"experimentalApi": true},
        });
        conn.call("initialize", info)?;
        conn.send("initialized", &json!({"method": "initialized"}))?;
        Ok(conn)
    }

    fn call(&mut self, method: &'static str, params: Value) -> Result<Value, Error> {
        self.write(method, method, params)?;
        self.answer(method)
    }

    /// Writes request `name`; `method` labels its errors. Its answer is
    /// due within the timeout.
    fn write(&mut self, method: &'static str, name: &str, params: Value) -> Result<(), Error> {
        self.proxy.set_deadline(Instant::now() + self.timeout);
        self.next += 1;
        self.send(
            method,
            &json!({"id": self.next, "method": name, "params": params}),
        )
    }

    fn send(&mut self, method: &'static str, msg: &Value) -> Result<(), Error> {
        self.proxy.send(method, msg.to_string())
    }

    /// The result of the last request.
    fn answer(&mut self, method: &'static str) -> Result<Value, Error> {
        let msg = self.response(method)?;
        if let Some(error) = msg.get("error") {
            let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
            return Err(Error::Rpc { method, code });
        }
        msg.get("result").cloned().ok_or(Error::Shape {
            method,
            what: "result",
        })
    }

    /// The whole response to the last request; notifications and server
    /// requests are skipped.
    fn response(&mut self, method: &'static str) -> Result<Value, Error> {
        loop {
            let text = self.proxy.recv(method)?;
            let Ok(msg) = serde_json::from_slice::<Value>(&text) else {
                return Err(Error::Shape {
                    method,
                    what: "JSON message",
                });
            };
            if msg.get("method").is_none()
                && msg.get("id").and_then(Value::as_u64) == Some(self.next)
            {
                return Ok(msg);
            }
        }
    }
}

#[cfg(test)]
#[path = "app_server_tests.rs"]
mod tests;
