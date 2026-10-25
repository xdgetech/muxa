//! `list` for a session without tmux-agents tags: the repo comes from git, and
//! the agent pane from the Claude Code adapter.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::PathBuf;
use std::process::Command;

use common::{Bridge, Server};
use muxa_protocol::{Message, Role};
use serde_json::{Value, json};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/claude-code");

struct Dirs {
    root: PathBuf,
}

impl Drop for Dirs {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test]
async fn an_untagged_session_looks_like_a_tagged_one() {
    let server = Server::start();
    let root = std::env::temp_dir().join(format!("muxa-sessions-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let dirs = Dirs { root: root.clone() };
    let app = root.join("app");
    std::fs::create_dir_all(&app).unwrap();
    let app = app.canonicalize().unwrap();
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&app)
            .args(["init", "-q"])
            .status()
            .unwrap()
            .success()
    );

    // An agent pane and a shell beside it, as a user would split them by hand.
    let dir = app.to_str().unwrap();
    let out = server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "plain",
        "-c",
        dir,
        "-P",
        "-F",
        "#{pane_id} #{pane_pid}",
        "sleep 600",
    ]);
    let (agent, pid) = out.split_once(' ').unwrap();
    let shell = server.tmux(&[
        "split-window",
        "-d",
        "-t",
        agent,
        "-c",
        dir,
        "-P",
        "-F",
        "#{pane_id}",
        "sh",
    ]);

    let claude = root.join("claude");
    std::fs::create_dir_all(claude.join("sessions")).unwrap();
    let mut v: Value =
        serde_json::from_str(&std::fs::read_to_string(format!("{SAMPLE}/session.json")).unwrap())
            .unwrap();
    v["pid"] = pid.parse::<u64>().unwrap().into();
    v["sessionId"] = "0b8c3c0e-1111-4a2b-9c3d-000000000003".into();
    v["tmux"] = format!("plain:@1.{agent}").into();
    v["cwd"] = dir.into();
    std::fs::write(claude.join(format!("sessions/{pid}.json")), v.to_string()).unwrap();

    let flag = format!("claude-code={}", claude.display());
    let (mut bridge, _) = Bridge::start(&server, &["--agent-dir", &flag]).await;
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    let plain = sessions.iter().find(|s| s.name == "plain").unwrap();
    assert_eq!(plain.repo.as_deref(), Some("app"));
    assert_eq!(plain.repo_dir.as_deref(), Some(dir));
    let role = |id: &str| plain.panes.iter().find(|p| p.id == id).unwrap().role;
    assert_eq!(role(agent), Role::Agent);
    assert_eq!(role(&shell), Role::Shell);
    drop(dirs);
}
