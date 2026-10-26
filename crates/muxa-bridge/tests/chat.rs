//! `chat` end to end: the bridge follows a fake Claude Code instance whose
//! session file points at a pane on a throwaway tmux server.
#![cfg(feature = "tmux-integration")]

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::codex::CodexDir;
use common::{Bridge, Server};
use muxa_protocol::{AgentState, ErrorCode, Message};
use serde_json::{Value, json};

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/claude-code");
const SESSION_ID: &str = "0b8c3c0e-1111-4a2b-9c3d-000000000002";
/// Longer than a few chat ticks.
const WAIT: Duration = Duration::from_secs(3);

struct ClaudeDir {
    dir: PathBuf,
    session: PathBuf,
    transcript: PathBuf,
}

impl ClaudeDir {
    /// A data directory whose one session runs as `pid` in `pane` of session "work".
    fn new(name: &str, pid: &str, pane: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-chat-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        std::fs::create_dir_all(dir.join("projects/-work-app")).unwrap();
        let session = dir.join(format!("sessions/{pid}.json"));
        let transcript = dir.join(format!("projects/-work-app/{SESSION_ID}.jsonl"));
        std::fs::copy(format!("{SAMPLE}/transcript.jsonl"), &transcript).unwrap();
        let d = Self {
            dir,
            session,
            transcript,
        };
        d.write_session(pid, pane, "busy");
        d
    }

    fn write_session(&self, pid: &str, pane: &str, status: &str) {
        let mut v: Value = serde_json::from_str(
            &std::fs::read_to_string(format!("{SAMPLE}/session.json")).unwrap(),
        )
        .unwrap();
        v["pid"] = pid.parse::<u64>().unwrap().into();
        v["sessionId"] = SESSION_ID.into();
        v["tmux"] = format!("work:@9.{pane}").into();
        v["status"] = status.into();
        std::fs::write(&self.session, v.to_string()).unwrap();
    }

    fn append(&self, line: &str) {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&self.transcript)
            .unwrap();
        writeln!(f, "{line}").unwrap();
    }

    fn arg(&self) -> String {
        format!("claude-code={}", self.dir.display())
    }
}

impl Drop for ClaudeDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A pane whose foreground program is not a shell, like a running agent.
fn agent_pane(server: &Server) -> (String, String) {
    let out = server.tmux(&[
        "new-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id} #{pane_pid}",
        "sleep 600",
    ]);
    let (pane, pid) = out.split_once(' ').unwrap();
    (pane.to_owned(), pid.to_owned())
}

async fn entries(bridge: &mut Bridge) -> (u64, u64, bool, Vec<String>, Option<AgentState>) {
    loop {
        match bridge.event(WAIT).await.expect("no chat-entries") {
            Message::ChatEntries {
                chat,
                seq,
                reset,
                entries,
                status,
                ..
            } => {
                return (
                    chat,
                    seq,
                    reset,
                    entries.into_iter().map(|e| e.id).collect(),
                    status.map(|s| s.state),
                );
            }
            // Watch traffic, if any, is not what this test is about.
            _ => continue,
        }
    }
}

fn is_ok(msg: &Message) -> bool {
    matches!(msg, Message::Result { error: None, .. })
}

#[tokio::test]
async fn follows_the_agent_in_a_pane() {
    let server = Server::start();
    let (pane, pid) = agent_pane(&server);
    let claude = ClaudeDir::new("follow", &pid, &pane);
    let (mut bridge, hello) = Bridge::start(&server, &["--agent-dir", &claude.arg()]).await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"chat".to_owned()));

    assert!(is_ok(
        &bridge.request(json!({"type": "chat", "pane": pane})).await
    ));
    let first = bridge.last_id();
    let (chat, seq, reset, ids, status) = entries(&mut bridge).await;
    assert_eq!((chat, seq, reset), (first, 1, true));
    assert_eq!(ids.len(), 7);
    assert_eq!(status, Some(AgentState::Running));

    claude.append(r#"{"type":"user","uuid":"later","message":{"content":"and the docs"}}"#);
    let (_, seq, reset, ids, _) = entries(&mut bridge).await;
    assert_eq!((seq, reset, ids), (2, false, vec!["u:later".to_owned()]));

    // A state change alone is sent too: it is how a waiting prompt shows up.
    std::thread::sleep(Duration::from_millis(20));
    claude.write_session(&pid, &pane, "waiting");
    let (_, _, _, ids, status) = entries(&mut bridge).await;
    assert!(ids.is_empty());
    assert_eq!(status, Some(AgentState::Waiting));

    // A second chat replaces the first.
    assert!(is_ok(
        &bridge.request(json!({"type": "chat", "pane": pane})).await
    ));
    let second = bridge.last_id();
    let (chat, seq, reset, _, _) = entries(&mut bridge).await;
    assert_eq!((chat, seq, reset), (second, 1, true));

    assert!(is_ok(&bridge.request(json!({"type": "unchat"})).await));
    claude.append(r#"{"type":"user","uuid":"unseen","message":{"content":"after unchat"}}"#);
    let after = bridge.event(Duration::from_millis(1500)).await;
    assert!(
        !matches!(after, Some(Message::ChatEntries { .. })),
        "got {after:?}"
    );
}

#[tokio::test]
async fn a_pane_without_a_known_agent_is_unsupported() {
    let server = Server::start();
    let shell = server.pane("work");
    let claude = ClaudeDir::new("unsupported", "1", "%999");
    let (mut bridge, _) = Bridge::start(&server, &["--agent-dir", &claude.arg()]).await;
    let Message::Result {
        error: Some(error), ..
    } = bridge.request(json!({"type": "chat", "pane": shell})).await
    else {
        panic!("chat on a plain shell succeeded")
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
}

#[tokio::test]
async fn the_chat_ends_when_the_agent_exits() {
    let server = Server::start();
    let (pane, pid) = agent_pane(&server);
    let claude = ClaudeDir::new("exit", &pid, &pane);
    let (mut bridge, _) = Bridge::start(&server, &["--agent-dir", &claude.arg()]).await;
    assert!(is_ok(
        &bridge.request(json!({"type": "chat", "pane": pane})).await
    ));
    entries(&mut bridge).await;
    std::fs::remove_file(&claude.session).unwrap();
    loop {
        match bridge.event(WAIT).await.expect("no chat-ended") {
            Message::ChatEnded { reason, .. } => {
                assert!(reason.contains("exited"), "{reason}");
                break;
            }
            _ => continue,
        }
    }
    assert!(Path::new(&claude.transcript).exists());
}

#[tokio::test]
async fn follows_codex_by_its_process_and_rollout() {
    let server = Server::start();
    let codex = CodexDir::new("follow");
    let (pane, cwd) = codex.start(&server, &["new-window", "-d"]);
    codex.rollout(&cwd);

    let (mut bridge, _) = Bridge::start(&server, &["--agent-dir", &codex.arg()]).await;
    assert!(is_ok(
        &bridge.request(json!({"type": "chat", "pane": pane})).await
    ));
    let (_, seq, reset, ids, status) = entries(&mut bridge).await;
    assert_eq!((seq, reset), (1, true));
    assert_eq!(
        ids,
        [
            "m:msg_user_2",
            "m:msg_asst_1",
            "t:call_exec_1",
            "t:call_shell_1",
            "m:msg_asst_2"
        ]
    );
    assert_eq!(status, Some(AgentState::Idle));
}
