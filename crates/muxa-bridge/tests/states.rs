//! Agent state changes reach the client as `sessions-changed`: from a tag, and
//! from Claude Code's own session file when there is no tag.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{Bridge, Server};
use muxa_protocol::{AgentState, Message};
use serde_json::{Value, json};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/claude-code");
/// A few polls.
const WAIT: Duration = Duration::from_secs(5);

/// The next `sessions-changed`, skipping anything else; None after `WAIT`.
async fn changed(bridge: &mut Bridge) -> bool {
    let deadline = tokio::time::Instant::now() + WAIT;
    while let Some(left) = deadline.checked_duration_since(tokio::time::Instant::now()) {
        match bridge.event(left).await {
            Some(Message::SessionsChanged) => return true,
            Some(_) => continue,
            None => return false,
        }
    }
    false
}

/// Waits out the structural notifications that follow an attach (a window's
/// automatic rename, for example), until a whole `WAIT` passes quietly.
async fn settle(bridge: &mut Bridge) {
    for _ in 0..3 {
        if !changed(bridge).await {
            return;
        }
    }
    panic!("sessions-changed keeps coming with nothing changing");
}

async fn state_of(bridge: &mut Bridge, pane: &str) -> Option<AgentState> {
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    let pane = sessions
        .iter()
        .flat_map(|s| &s.panes)
        .find(|p| p.id == pane)?;
    pane.agent.as_ref().map(|a| a.state)
}

#[tokio::test]
async fn a_tagged_state_change_is_announced_once() {
    let server = Server::start();
    let pane = server.pane("=work:");
    server.tmux(&["set-option", "-p", "-t", &pane, "@agents_role", "ai"]);
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "running"]);
    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    settle(&mut bridge).await;

    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "waiting"]);
    assert!(changed(&mut bridge).await, "the change was not announced");
    assert_eq!(
        state_of(&mut bridge, &pane).await,
        Some(AgentState::Waiting)
    );
    // The time alone moving is not a change.
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state_at", "123"]);
    assert!(!changed(&mut bridge).await);
}

struct Dir(PathBuf);

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn an_untagged_agents_own_state_is_followed() {
    let server = Server::start();
    let out = server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "plain",
        "-P",
        "-F",
        "#{pane_id} #{pane_pid}",
        "sleep 600",
    ]);
    let (pane, pid) = out.split_once(' ').unwrap();
    let dir = Dir(std::env::temp_dir().join(format!("muxa-states-{}", std::process::id())));
    std::fs::create_dir_all(dir.0.join("sessions")).unwrap();
    let file = dir.0.join(format!("sessions/{pid}.json"));
    let write = |status: &str| {
        let mut v: Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{SAMPLE}/session.json")).unwrap(),
        )
        .unwrap();
        v["pid"] = pid.parse::<u64>().unwrap().into();
        v["sessionId"] = "0b8c3c0e-1111-4a2b-9c3d-000000000004".into();
        v["tmux"] = format!("plain:@1.{pane}").into();
        v["status"] = status.into();
        std::fs::write(&file, v.to_string()).unwrap();
    };
    write("busy");
    let flag = format!("claude-code={}", dir.0.display());
    let (mut bridge, _) = Bridge::start(&server, &["--agent-dir", &flag]).await;
    assert_eq!(state_of(&mut bridge, pane).await, Some(AgentState::Running));
    settle(&mut bridge).await;

    write("waiting");
    assert!(changed(&mut bridge).await, "the change was not announced");
    assert_eq!(state_of(&mut bridge, pane).await, Some(AgentState::Waiting));
}
