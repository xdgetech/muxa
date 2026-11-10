//! A workspace `lisk` with one repo, for the workspace memory tests (ADR
//! 0053): its `workspaces.toml`, a bridge with or without a host daemon,
//! and the live `.muxa/WORKSPACE.md`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use muxa_protocol::{ErrorBody, Message, TaskRecord};
use serde_json::{Value, json};

use super::daemon::Daemon;
use super::repo::git;
use super::{Bridge, Server};

/// Echoes what is typed; enough of an agent here.
pub const AGENT: &str = "cat";

pub struct Host {
    pub dir: PathBuf,
    /// The workspace root, with the repo `app`.
    pub root: PathBuf,
    pub daemon: Option<Daemon>,
}

impl Host {
    /// `extra` goes into the `[[workspace]]` table, e.g. `memory = "auto"`.
    pub fn new(name: &str, extra: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-memory-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lisk")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let root = dir.join("lisk");
        init(&root.join("app"));
        std::fs::write(
            dir.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"lisk\"\nroot = \"{}\"\n{extra}\n",
                root.display()
            ),
        )
        .unwrap();
        Self {
            dir,
            root,
            daemon: None,
        }
    }

    /// A bridge; with `daemon`, the record requests go to a host daemon.
    pub async fn bridge(&mut self, server: &Server, daemon: bool) -> Bridge {
        let mut args: Vec<String> = [
            "--state-dir",
            &self.dir.join("state").display().to_string(),
            "--workspaces",
            &self.dir.join("workspaces.toml").display().to_string(),
            "--task-launcher",
            "builtin",
            "--task-command",
            AGENT,
        ]
        .map(String::from)
        .into();
        let daemon_dir = if daemon {
            let flags: Vec<&str> = args.iter().map(String::as_str).collect();
            let started = Daemon::start(server, &flags).await;
            let dir = started.dir.clone();
            self.daemon = Some(started);
            dir
        } else {
            self.dir.join("no-daemon")
        };
        args.extend(["--daemon-dir".to_owned(), daemon_dir.display().to_string()]);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        Bridge::start(server, &args).await.0
    }

    pub fn memory(&self) -> PathBuf {
        self.root.join(".muxa/WORKSPACE.md")
    }

    pub fn write_memory(&self, text: &str) {
        std::fs::create_dir_all(self.root.join(".muxa")).unwrap();
        std::fs::write(self.memory(), text).unwrap();
    }

    pub fn read_memory(&self) -> Option<String> {
        std::fs::read_to_string(self.memory()).ok()
    }

    /// The daemon's log; empty without one.
    pub fn log(&self) -> String {
        self.daemon.as_ref().map(Daemon::log).unwrap_or_default()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn init(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    git(repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "# repo\n").unwrap();
    git(repo, &["add", "."]);
    git(
        repo,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
}

/// The messages before the result, and the result.
pub async fn send(bridge: &mut Bridge, req: Value) -> (Vec<Message>, Message) {
    let result = bridge.request_within(req, Duration::from_secs(60)).await;
    let mut before = Vec::new();
    while let Some(msg) = bridge.event(Duration::from_millis(200)).await {
        before.push(msg);
    }
    (before, result)
}

/// The `task-saved` of a request, and its result.
pub async fn saved(bridge: &mut Bridge, req: Value) -> (Option<TaskRecord>, Message) {
    let (before, result) = send(bridge, req).await;
    let record = before.into_iter().find_map(|m| match m {
        Message::TaskSaved { record, .. } => Some(record),
        _ => None,
    });
    (record, result)
}

/// `create-task` in `lisk` with `fields` over the defaults.
pub async fn create(bridge: &mut Bridge, fields: Value) -> (Option<TaskRecord>, Message) {
    let mut req = json!({"type": "create-task", "workspace": "lisk", "title": "Fix the login", "goal": "Make login work."});
    for (k, v) in fields.as_object().unwrap() {
        req[k] = v.clone();
    }
    saved(bridge, req).await
}

pub fn ok(result: &Message) {
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
}

pub fn error_of(result: &Message) -> ErrorBody {
    match result {
        Message::Result { error: Some(e), .. } => e.clone(),
        _ => panic!("{result:?}"),
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The ids of the server's sessions.
pub fn sessions(server: &Server) -> Vec<String> {
    server
        .tmux(&["list-sessions", "-F", "#{session_name}"])
        .lines()
        .map(str::to_owned)
        .collect()
}
