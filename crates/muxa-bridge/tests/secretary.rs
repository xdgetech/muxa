//! The secretary stays open (ADR 0056), on a throwaway tmux server: the
//! daemon opens the overall workspace's root Ask session, opens it again
//! after it is closed, respawns its agent in place, stops a crash loop with
//! one failure item, and resumes a known Claude Code conversation. The
//! desktop's session never changes.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::Server;
use common::daemon::{Daemon, short_dir};

const FAST: [(&str, &str); 1] = [("MUXA_TEST_SECRETARY_FAST", "1")];
const ID: &str = "11111111-2222-4333-8444-555555555555";

/// The overall workspace `assistant` with `command` as its agent.
struct Host {
    dir: PathBuf,
}

impl Host {
    fn new(name: &str, command: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-secretary-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assistant-root")).unwrap();
        std::fs::create_dir_all(dir.join("state")).unwrap();
        let dir = dir.canonicalize().unwrap();
        std::fs::write(
            dir.join("workspaces.toml"),
            format!(
                "[assistant]\nenabled = true\n[[workspace]]\nname = \"assistant\"\nroot = \"{}\"\n\
                 [workspace.agent]\ncommand = \"{command}\"\n[workspace.assistant]\noverall = true\n\
                 plans = []\n",
                dir.join("assistant-root").display()
            ),
        )
        .unwrap();
        Self { dir }
    }

    fn state(&self) -> PathBuf {
        self.dir.join("state")
    }

    async fn daemon(&self, server: &Server) -> Daemon {
        let state = self.state().display().to_string();
        let workspaces = self.dir.join("workspaces.toml").display().to_string();
        let flags = [
            "--state-dir",
            state.as_str(),
            "--workspaces",
            workspaces.as_str(),
            "--task-launcher",
            "builtin",
            "--task-command",
            "cat",
        ];
        Daemon::start_with(server, short_dir(), &flags, &FAST).await
    }

    /// An executable `name` in the host dir running `body` under sh.
    fn script(&self, name: &str, body: &str) -> PathBuf {
        let path = self.dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn lines(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.dir.join(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// `<session id> <pane id> <pane pid> <pane dead>` of the secretary's session, if open.
fn secretary(server: &Server) -> Option<String> {
    let all = server.tmux(&[
        "list-panes",
        "-a",
        "-F",
        "#{@muxa_secretary}|#{@muxa_ask_root}|#{session_name}|#{session_id} #{pane_id} #{pane_pid} #{pane_dead}",
    ]);
    all.lines()
        .find(|l| l.starts_with("1|1|ask-assistant|"))
        .map(|l| l.rsplit('|').next().unwrap_or_default().to_owned())
}

/// The desktop's session as the user sees it: windows, layout, sizes, processes.
fn desk(server: &Server) -> String {
    server.tmux(&[
        "list-panes",
        "-s",
        "-t",
        "=work",
        "-F",
        "#{window_id} #{window_layout} #{window_active} #{pane_id} #{pane_active} #{pane_width}x#{pane_height} #{pane_pid}",
    ])
}

async fn wait_for<T>(what: &str, wait: Duration, mut found: impl FnMut() -> Option<T>) -> T {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        if let Some(v) = found() {
            return v;
        }
        assert!(tokio::time::Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn field(line: &str, n: usize) -> String {
    line.split(' ').nth(n).unwrap_or_default().to_owned()
}

#[tokio::test]
async fn the_daemon_opens_the_chat_and_opens_it_again_when_it_ends() {
    let server = Server::start();
    server.tmux(&["split-window", "-d", "-t", "=work:", "sh"]);
    let before = desk(&server);
    let host = Host::new("open", "cat");
    let daemon = host.daemon(&server).await;
    let first = wait_for(
        "the daemon did not open the chat",
        Duration::from_secs(10),
        || secretary(&server),
    )
    .await;
    assert_eq!(field(&first, 3), "0");

    server.tmux(&["kill-session", "-t", &field(&first, 0)]);
    let second = wait_for(
        "the closed chat did not open again",
        Duration::from_secs(10),
        || secretary(&server).filter(|s| field(s, 0) != field(&first, 0)),
    )
    .await;

    // The agent exits: its pane stays dead, and the daemon respawns it in place.
    let pane = field(&second, 1);
    server.tmux(&["respawn-pane", "-k", "-t", &pane, "true"]);
    let third = wait_for(
        "the exited agent did not start again",
        Duration::from_secs(10),
        || {
            secretary(&server).filter(|s| {
                field(s, 3) == "0"
                    && field(s, 2) != field(&second, 2)
                    && server.tmux(&[
                        "display-message",
                        "-p",
                        "-t",
                        &pane,
                        "#{pane_current_command}",
                    ]) == "cat"
            })
        },
    )
    .await;
    assert_eq!(
        (field(&third, 0), field(&third, 1)),
        (field(&second, 0), pane)
    );

    assert_eq!(desk(&server), before, "the desktop's session changed");
    let log = daemon.log();
    assert!(
        !log.lines()
            .any(|l| l.contains("WARN") && l.contains("secretary")),
        "{log}"
    );
}

#[tokio::test]
async fn a_crash_loop_stops_with_one_failure_item() {
    let server = Server::start();
    let before = desk(&server);
    let host = Host::new("crash", "");
    let agent = host.script(
        "agent",
        &format!(
            "echo start >> '{}'\nexit 1",
            host.dir.join("starts").display()
        ),
    );
    let toml = std::fs::read_to_string(host.dir.join("workspaces.toml")).unwrap();
    std::fs::write(
        host.dir.join("workspaces.toml"),
        toml.replace(
            "command = \"\"",
            &format!("command = \"{}\"", agent.display()),
        ),
    )
    .unwrap();
    let daemon = host.daemon(&server).await;
    let items = host.state().join("items/t-0.json");
    wait_for(
        "no failure item after the crash loop",
        Duration::from_secs(20),
        || items.is_file().then_some(()),
    )
    .await;
    let starts = host.lines("starts").len();
    assert_eq!(starts, 4, "one open and three restarts");
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(host.lines("starts").len(), starts, "it kept restarting");

    let text = std::fs::read_to_string(&items).unwrap();
    assert_eq!(text.matches("\"kind\": \"failure\"").count(), 1, "{text}");
    assert!(text.contains("\"workspace\": \"assistant\""), "{text}");

    // Retry resets the cap: the daemon starts it again (ADR 0061).
    let file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let item = file["items"][0]["id"].as_str().unwrap().to_owned();
    let mut conn = daemon.connect().await;
    let retry = serde_json::json!({"type": "update-inbox", "entry": item, "action": "retry"});
    for _ in 0..2 {
        let answer = conn.request(retry.clone()).await;
        match answer.as_slice() {
            [
                muxa_protocol::Message::InboxSaved { item: Some(i), .. },
                muxa_protocol::Message::Result { error: None, .. },
            ] => assert_eq!(i.answer, Some(muxa_protocol::InboxAction::Retry)),
            other => panic!("retry: {other:?}"),
        }
    }
    wait_for("no start after Retry", Duration::from_secs(20), || {
        (host.lines("starts").len() > starts).then_some(())
    })
    .await;
    assert_eq!(desk(&server), before, "the desktop's session changed");
    drop(daemon);
}

#[tokio::test]
async fn a_known_conversation_is_resumed_and_a_failed_resume_starts_fresh() {
    let server = Server::start();
    let host = Host::new("resume", "");
    // A stand-in for Claude Code: it records its arguments, and exits at
    // once on --resume, as Claude Code does with an unknown id.
    let args = host.dir.join("args");
    let bin = host.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let claude = bin.join("claude");
    std::fs::write(
        &claude,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\ncase \"$*\" in *--resume*) exit 1 ;; esac\nexec cat\n",
            args.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let toml = std::fs::read_to_string(host.dir.join("workspaces.toml")).unwrap();
    std::fs::write(
        host.dir.join("workspaces.toml"),
        toml.replace(
            "command = \"\"",
            &format!("command = \"{}\"", claude.display()),
        ),
    )
    .unwrap();
    let file = host.state().join("assistant/secretary.json");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, format!("{{\"agentSession\":\"{ID}\"}}\n")).unwrap();

    let daemon = host.daemon(&server).await;
    let lines = wait_for(
        "the agent did not start twice",
        Duration::from_secs(15),
        || {
            let lines = host.lines("args");
            (lines.len() >= 2).then_some(lines)
        },
    )
    .await;
    assert_eq!(lines[0], format!("--resume {ID}"));
    assert_eq!(lines[1], "", "the start after a failed resume is fresh");
    wait_for(
        "the fresh agent does not run",
        Duration::from_secs(5),
        || secretary(&server).filter(|s| field(s, 3) == "0"),
    )
    .await;
    let state = std::fs::read_to_string(&file).unwrap();
    assert!(!state.contains(ID), "the id was kept: {state}");
    assert!(state.contains("resumeFailedAt"), "{state}");
    assert!(!Path::new(&host.state().join("items/t-0.json")).exists());
    drop(daemon);
}
