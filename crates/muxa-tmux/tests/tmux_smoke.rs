//! End-to-end checks against a throwaway tmux server (tmux -L muxa-test-*).
//! Run with `make test-tmux`.
#![cfg(feature = "tmux-integration")]

use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use muxa_tmux::{Client, Error, Options, PaneId, Role, list_sessions, locate, send_text, snapshot};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// An isolated tmux server, killed on drop.
struct Server {
    socket: String,
}

impl Server {
    fn start() -> Self {
        let socket = format!(
            "muxa-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let server = Server { socket };
        server.tmux(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "work",
            "-x",
            "80",
            "-y",
            "24",
            "sh",
        ]);
        server.tmux(&["set-option", "-g", "default-shell", "/bin/sh"]);
        server
    }

    fn tmux(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .env_remove("TMUX")
            .output()
            .expect("run tmux");
        assert!(
            out.status.success(),
            "tmux {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
    }

    fn options(&self) -> Options {
        Options {
            socket: Some(self.socket.clone()),
            ..Options::default()
        }
    }

    fn first_pane(&self) -> PaneId {
        self.tmux(&["display-message", "-p", "-t", "work", "#{pane_id}"])
            .parse()
            .expect("pane id")
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

async fn wait_for(mut cond: impl FnMut() -> bool) {
    for _ in 0..50 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("condition not met in 5 s");
}

#[tokio::test]
async fn lists_sessions_with_roles() {
    let server = Server::start();
    let pane = server.first_pane();
    server.tmux(&[
        "set-option",
        "-p",
        "-t",
        &pane.to_string(),
        "@muxa_role",
        "agent",
    ]);
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();

    let sessions = list_sessions(&client).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "work");
    assert_eq!(sessions[0].panes[0].id, pane);
    assert_eq!(sessions[0].panes[0].role, Role::Agent);
    assert_eq!(
        (sessions[0].panes[0].cols, sessions[0].panes[0].rows),
        (80, 24)
    );
    assert_eq!(locate(&client, pane).await.unwrap().0, sessions[0].id);
}

#[tokio::test]
async fn replies_stay_aligned_after_a_failed_command() {
    let server = Server::start();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();

    let results = client
        .run(
            "display-message -p one ; capture-pane -p -t %999 ; display-message -p three",
            3,
        )
        .await
        .unwrap();
    assert_eq!(results[0].as_ref().unwrap().lines, vec![b"one".to_vec()]);
    assert!(matches!(results[1], Err(Error::Tmux { .. })));
    assert!(matches!(results[2], Err(Error::Skipped { .. })));

    let after = client.command("display-message -p after").await.unwrap();
    assert_eq!(after.lines, vec![b"after".to_vec()]);
}

#[tokio::test]
async fn snapshot_and_input_round_trip() {
    let server = Server::start();
    let pane = server.first_pane();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();

    send_text(&client, pane, "echo 'a\"b$c é'").await.unwrap();
    muxa_tmux::send_keys(&client, pane, &["Enter".into()])
        .await
        .unwrap();
    wait_for(|| {
        server
            .tmux(&["capture-pane", "-p", "-t", &pane.to_string()])
            .contains("a\"b$c é\n")
    })
    .await;

    let screen = snapshot(&client, pane).await.unwrap();
    assert_eq!((screen.cols, screen.rows), (80, 24));
    assert!(String::from_utf8_lossy(&screen.data).contains("a\"b$c é"));
    assert!(matches!(
        snapshot(&client, PaneId::new(9999)).await,
        Err(Error::NoPane(_))
    ));
}

#[tokio::test]
async fn never_changes_the_window_size() {
    let server = Server::start();
    let size = || {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            "work",
            "#{window_width}x#{window_height}",
        ])
    };
    let before = size();
    let (client, _notes) = Client::attach(&server.options(), None).unwrap();
    list_sessions(&client).await.unwrap();
    snapshot(&client, server.first_pane()).await.unwrap();
    assert_eq!(size(), before);
}
