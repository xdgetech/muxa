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

/// The control clients on the server, as `#{client_flags}`.
fn control_clients(server: &Server) -> Vec<String> {
    server
        .tmux(&["list-clients", "-F", "#{client_flags}"])
        .lines()
        .filter(|f| f.contains("control-mode"))
        .map(str::to_owned)
        .collect()
}

/// The daemon attaches one quiet client while it has work, and attaches
/// again after tmux detached it.
#[tokio::test]
async fn the_daemon_attaches_one_no_output_client_and_reattaches() {
    let server = Server::start();
    let host = Host::new("client");
    assert!(control_clients(&server).is_empty());
    let mut daemon = host.daemon(&server).await;
    assert!(control_clients(&server).is_empty(), "attached with no work");

    let mut conn = daemon.connect().await;
    let first = create_through(&mut conn).await;
    // The start prompt's wait holds the client.
    let clients = control_clients(&server);
    assert_eq!(clients.len(), 1, "{clients:?}");
    assert!(clients[0].contains("no-output"), "{clients:?}");
    assert!(clients[0].contains("ignore-size"), "{clients:?}");

    // Destroying its session detaches it; the next request attaches again.
    server.tmux(&["kill-session", "-t", "=work"]);
    common::wait_until(|| control_clients(&server).is_empty()).await;
    let second = create_through(&mut conn).await;
    assert_ne!(first.id, second.id);
    assert_eq!(control_clients(&server).len(), 1);

    let first_session = first.session.unwrap();
    let still = server.tmux(&[
        "display-message",
        "-p",
        "-t",
        &format!("={first_session}:"),
        "#{window_width}x#{window_height}",
    ]);
    assert_eq!(still, "80x24", "the daemon's client sized a window");
    assert!(daemon.shutdown().await.success());
}

/// ADR 0043 §6: at start the daemon removes a listed worktree that nothing
/// holds when nothing can be lost. One with work, or one a pane works in, stays.
#[tokio::test]
async fn the_daemon_sweeps_listed_worktrees_nothing_holds() {
    let server = Server::start();
    let host = Host::new("sweep");
    let repo = common::repo::Repo::new("daemon-sweep");
    let root = repo.root.join("repos/.worktrees");
    std::fs::create_dir_all(&root).unwrap();
    let list = server.xdg_state().join("muxa/worktrees");
    std::fs::create_dir_all(&list).unwrap();
    let add = |branch: &str| {
        let path = root.join(format!("app-{branch}"));
        common::repo::git(
            &repo.app,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                branch,
                path.to_str().unwrap(),
            ],
        );
        let real = path.canonicalize().unwrap();
        let entry = list.join(real.display().to_string().replace('/', "%"));
        std::fs::write(entry, format!("{}\n", real.display())).unwrap();
        real
    };
    let clean = add("clean");
    let work = add("work");
    common::repo::git(
        &work,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "work",
        ],
    );
    let held = add("held");
    server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "held",
        "-c",
        held.to_str().unwrap(),
        "sh",
    ]);

    let _daemon = host.daemon(&server).await;
    common::wait_until(|| !clean.exists()).await;
    let branch = std::process::Command::new("git")
        .arg("-C")
        .arg(&repo.app)
        .args(["rev-parse", "--verify", "-q", "refs/heads/clean"])
        .output()
        .unwrap();
    assert!(!branch.status.success(), "the branch went with it");
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(work.is_dir(), "unmerged work waits for the user");
    assert!(held.is_dir(), "a pane works in it");
}

/// ADR 0047: at start the daemon closes a session tagged with a done or
/// dropped Task, as tmux-resurrect restores one, and keeps its folder. A
/// session of an open Task and an untagged one stay.
#[tokio::test]
async fn the_daemon_closes_sessions_of_ended_tasks() {
    let server = Server::start();
    let host = Host::new("ended");
    let state = host.dir.join("daemon-state");
    std::fs::create_dir_all(&state).unwrap();
    let task = |id: &str, status: &str| {
        let dir = host.dir.join("work").join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let record = json!({
            "version": 1, "id": id, "rev": 1, "workspace": "work", "title": id, "goal": "",
            "status": status, "createdAt": 1, "updatedAt": 1,
            "dir": dir.display().to_string(),
        });
        std::fs::write(state.join(format!("{id}.json")), record.to_string()).unwrap();
        let session = server.tmux(&[
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{session_id}",
            "-s",
            id,
            "-c",
            dir.to_str().unwrap(),
            "sh",
        ]);
        server.tmux(&["set-option", "-t", session.trim(), "@muxa_task", id]);
        dir
    };
    let done = task("t-d0ae01", "done");
    task("t-d40901", "dropped");
    task("t-09e401", "open");
    server.tmux(&["new-session", "-d", "-s", "plain", "sh"]);
    let names = || {
        let mut names: Vec<String> = server
            .tmux(&["list-sessions", "-F", "#{session_name}"])
            .lines()
            .map(str::to_owned)
            .collect();
        names.sort();
        names
    };

    let _daemon = host.daemon(&server).await;
    common::wait_until(|| names() == ["plain", "t-09e401", "work"]).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(names(), ["plain", "t-09e401", "work"]);
    assert!(done.is_dir(), "the Task's folder stays");
}
