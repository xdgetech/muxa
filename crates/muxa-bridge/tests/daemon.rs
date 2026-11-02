//! The host daemon (ADR 0019) as a real process on a throwaway tmux server:
//! its socket and lock, `shutdown`, and a bridge that sends record requests
//! to it while it runs and serves them itself when it does not.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::daemon::{Conn, Daemon, short_dir, spawn};
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message, TaskRecord};
use serde_json::json;

/// A state directory for the daemon and another for the bridge, and the
/// `workspaces.toml` both read. A record shows which one served a request.
struct Host {
    dir: PathBuf,
}

impl Host {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-daemon-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let dir = dir.canonicalize().unwrap();
        std::fs::write(
            dir.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"work\"\nroot = \"{}\"\n",
                dir.join("work").display()
            ),
        )
        .unwrap();
        Self { dir }
    }

    fn flags(&self, state: &str) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.dir.join(state).display().to_string(),
            "--workspaces".into(),
            self.dir.join("workspaces.toml").display().to_string(),
            "--task-launcher".into(),
            "builtin".into(),
            "--task-command".into(),
            "cat".into(),
        ]
    }

    async fn daemon(&self, server: &Server) -> Daemon {
        let flags = self.flags("daemon-state");
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        Daemon::start(server, &flags).await
    }

    async fn bridge(&self, server: &Server, daemon_dir: &std::path::Path) -> Bridge {
        let mut flags = self.flags("bridge-state");
        flags.extend(["--daemon-dir".into(), daemon_dir.display().to_string()]);
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        Bridge::start(server, &flags).await.0
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

async fn list(bridge: &mut Bridge) -> Vec<TaskRecord> {
    let result = bridge.request(json!({"type": "list-tasks"})).await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    while let Some(msg) = bridge.event(Duration::from_millis(200)).await {
        if let Message::TaskList { entries, .. } = msg {
            return entries;
        }
    }
    panic!("no task-list")
}

const GOAL: &str = "A goal the log must not show: daemon-secret-7f3a.";

/// A record the daemon wrote, through its own socket.
async fn create_through(conn: &mut Conn) -> TaskRecord {
    let answer = conn
        .request(json!({"type": "create-task", "workspace": "work", "title": "Daemon task", "goal": GOAL}))
        .await;
    match answer.as_slice() {
        [
            Message::TaskSaved { record, .. },
            Message::Result { error: None, .. },
        ] => record.clone(),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_socket_is_private_and_one_daemon_runs() {
    let server = Server::start();
    let host = Host::new("socket");
    // A socket file left by a daemon that died.
    let dir = short_dir();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("daemon.sock"), "").unwrap();
    let flags = host.flags("daemon-state");
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let mut daemon = Daemon::start_in(&server, dir, &flags).await;
    assert_eq!(Daemon::mode(&daemon.dir), 0o700);
    assert_eq!(Daemon::mode(&daemon.socket()), 0o600);

    let mut conn = daemon.connect().await;
    let Message::Hello { bridge, .. } = &conn.hello else {
        panic!("{:?}", conn.hello)
    };
    assert_eq!(bridge, env!("CARGO_PKG_VERSION"));
    let answer = conn.request(json!({"type": "ping"})).await;
    let Some(Message::Result { error: Some(e), .. }) = answer.last() else {
        panic!("{answer:?}")
    };
    assert_eq!(e.code, ErrorCode::Unsupported);

    // A second daemon on the same dir exits 0, and the first keeps serving.
    let mut second = spawn(&server, &daemon.dir, &flags);
    let status = tokio::time::timeout(Duration::from_secs(5), second.wait())
        .await
        .expect("the second daemon did not exit")
        .unwrap();
    assert!(status.success(), "{status:?}");
    let answer = conn.request(json!({"type": "list-tasks"})).await;
    assert!(matches!(
        answer.last(),
        Some(Message::Result { error: None, .. })
    ));

    let status = daemon.shutdown().await;
    assert!(status.success(), "{status:?}");
    assert!(!daemon.socket().exists(), "the socket was not removed");
}

#[tokio::test]
async fn a_bridge_uses_the_daemon_while_it_runs_and_falls_back_after() {
    let server = Server::start();
    let host = Host::new("forward");
    let before = server.layout();
    let mut daemon = host.daemon(&server).await;
    let mut conn = daemon.connect().await;
    let record = create_through(&mut conn).await;

    // The bridge keeps its own state dir; only the daemon has the record.
    let mut bridge = host.bridge(&server, &daemon.dir).await;
    let listed = list(&mut bridge).await;
    assert_eq!(
        listed.iter().map(|r| &r.id).collect::<Vec<_>>(),
        [&record.id]
    );

    let status = daemon.shutdown().await;
    assert!(status.success());
    assert!(
        list(&mut bridge).await.is_empty(),
        "the fallback reads the bridge's own dir"
    );

    let log = daemon.log();
    assert!(log.contains("daemon started"), "{log}");
    for secret in ["daemon-secret-7f3a", "Daemon task"] {
        assert!(!log.contains(secret), "the log shows task text: {log}");
    }
    // Only the new task's session was added; nothing else moved.
    let after = server.layout();
    let kept: Vec<&str> = after
        .lines()
        .filter(|l| {
            before
                .lines()
                .any(|b| b.split(' ').nth(3) == l.split(' ').nth(3))
        })
        .collect();
    assert_eq!(kept.join("\n"), before);
}

#[tokio::test]
async fn after_a_crash_the_bridge_falls_back_and_a_new_daemon_starts() {
    let server = Server::start();
    let host = Host::new("crash");
    let mut daemon = host.daemon(&server).await;
    let mut conn = daemon.connect().await;
    let record = create_through(&mut conn).await;
    daemon.crash().await;
    assert!(daemon.socket().exists(), "a crash leaves the socket");

    let mut bridge = host.bridge(&server, &daemon.dir).await;
    assert!(list(&mut bridge).await.is_empty());

    // The kernel dropped the lock with the process.
    let flags = host.flags("daemon-state");
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    daemon.start_again(&server, &flags).await;
    let listed = list(&mut bridge).await;
    assert_eq!(
        listed.iter().map(|r| &r.id).collect::<Vec<_>>(),
        [&record.id]
    );
    assert!(daemon.shutdown().await.success());
}
