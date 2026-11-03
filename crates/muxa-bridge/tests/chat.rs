//! `chat` end to end: the bridge follows a fake Claude Code instance whose
//! session file points at a pane on a throwaway tmux server.
#![cfg(feature = "tmux-integration")]

mod common;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use common::codex::CodexDir;
use common::{Bridge, Server};
use muxa_protocol::{AgentState, ChatPrompt, ErrorCode, Message, PromptKind};
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

    /// Replaces the transcript with another sample.
    fn sample(&self, name: &str) {
        std::fs::copy(format!("{SAMPLE}/{name}"), &self.transcript).unwrap();
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

/// The next chat-entries: its entry ids and its prompt.
async fn prompted(bridge: &mut Bridge) -> (Vec<String>, Option<ChatPrompt>) {
    loop {
        if let Message::ChatEntries {
            entries, prompt, ..
        } = bridge.event(WAIT).await.expect("no chat-entries")
        {
            return (entries.into_iter().map(|e| e.id).collect(), prompt);
        }
    }
}

#[tokio::test]
async fn a_waiting_permission_is_sent_as_a_prompt_until_answered() {
    let server = Server::start();
    let (pane, pid) = agent_pane(&server);
    let claude = ClaudeDir::new("prompt", &pid, &pane);
    claude.sample("waiting-permission.jsonl");
    claude.write_session(&pid, &pane, "waiting");
    let (mut bridge, hello) = Bridge::start(&server, &["--agent-dir", &claude.arg()]).await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"chat-prompt".to_owned()));

    assert!(is_ok(
        &bridge.request(json!({"type": "chat", "pane": pane})).await
    ));
    let (_, prompt) = prompted(&mut bridge).await;
    let prompt = prompt.expect("a prompt");
    assert_eq!(prompt.entry, "t:toolu_p1");
    assert_eq!(prompt.kind, PromptKind::Permission);
    assert_eq!(prompt.options[0].keys, ["1"]);

    claude.append(r#"{"type":"user","uuid":"u2","message":{"role":"user","content":[{"tool_use_id":"toolu_p1","type":"tool_result","content":"ok"}]}}"#);
    let (ids, prompt) = prompted(&mut bridge).await;
    assert_eq!(ids, ["t:toolu_p1"]);
    assert_eq!(prompt, None);
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
async fn a_replaced_process_rebinds_chat_and_drops_the_old_transcript() {
    let server = Server::start();
    let (pane, pid) = agent_pane(&server);
    let old = ClaudeDir::new("replacement-old", &pid, &pane);
    let mut next = ClaudeDir::new("replacement-new", "999999", &pane);
    std::fs::write(
        &next.transcript,
        concat!(
            r#"{"type":"user","uuid":"replacement","message":{"content":"new run"}}"#,
            "\n"
        ),
    )
    .unwrap();
    let (mut bridge, _) = Bridge::start(
        &server,
        &["--agent-dir", &old.arg(), "--agent-dir", &next.arg()],
    )
    .await;
    assert!(is_ok(
        &bridge.request(json!({"type": "chat", "pane": pane})).await
    ));
    let (chat, _, _, _, _) = entries(&mut bridge).await;

    server.tmux(&["respawn-pane", "-k", "-t", &pane, "sleep 600"]);
    let new_pid = server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_pid}"]);
    assert_ne!(pid, new_pid);
    std::fs::remove_file(&next.session).unwrap();
    next.session = next.dir.join(format!("sessions/{new_pid}.json"));
    next.write_session(&new_pid, &pane, "idle");

    let (next_chat, seq, reset, ids, status) = entries(&mut bridge).await;
    assert_eq!(next_chat, chat);
    assert!(seq > 1);
    assert!(reset);
    assert_eq!(ids, ["u:replacement"]);
    assert_eq!(status, Some(AgentState::Idle));
    assert!(
        old.transcript.exists(),
        "old files stay on disk but are no longer polled"
    );
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

fn managed_record(server: &Server, pane: &str, pid: &str, state: &Path) -> Value {
    let session = server.tmux(&["display-message", "-p", "-t", pane, "#{session_id}"]);
    server.tmux(&["set-option", "-t", &session, "@muxa_task", "t-c1a001"]);
    let started = std::process::Command::new("ps")
        .args(["-p", pid, "-o", "lstart="])
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert!(started.status.success());
    let started = String::from_utf8(started.stdout).unwrap().trim().to_owned();
    std::fs::create_dir_all(state).unwrap();
    json!({
        "version":1,"id":"t-c1a001","rev":1,"workspace":"work","title":"Synthetic managed chat",
        "goal":"Fixture","status":"open","createdAt":1,"updatedAt":1,
        "runs":[{"id":2,"startedAt":1,"reason":"restart","restartObservation":{
            "version":1,"operation":"restart-chat","session":session[1..].parse::<u32>().unwrap(),
            "pane":pane[1..].parse::<u32>().unwrap(),"replacement":{"pid":pid.parse::<u32>().unwrap(),"started":started},
            "source":null
        }}]
    })
}

fn write_managed(state: &Path, record: &Value) {
    let staged = state.join("record.tmp");
    std::fs::write(&staged, serde_json::to_vec(record).unwrap()).unwrap();
    std::fs::rename(staged, state.join("t-c1a001.json")).unwrap();
}

#[tokio::test]
async fn an_incomplete_named_start_blocks_chat_and_clears_tagged_state() {
    let server = Server::start();
    let (pane, pid) = agent_pane(&server);
    let claude = ClaudeDir::new("incomplete-named-start", &pid, &pane);
    let state = claude.dir.join("state");
    server.tmux(&["set-option", "-p", "-t", &pane, "@muxa_role", "agent"]);
    server.tmux(&["set-option", "-p", "-t", &pane, "@agent_state", "waiting"]);
    let mut confirmed = managed_record(&server, &pane, &pid, &state);
    confirmed["worker"] = "Reviewer".into();
    confirmed["runs"][0]["worker"] = "Reviewer".into();
    let mut no_run = confirmed.clone();
    no_run["runs"] = json!([]);
    write_managed(&state, &no_run);
    let (mut bridge, _) = Bridge::start(
        &server,
        &[
            "--agent-dir",
            &claude.arg(),
            "--state-dir",
            state.to_str().unwrap(),
        ],
    )
    .await;
    let mut no_baseline = confirmed.clone();
    no_baseline["runs"][0]
        .as_object_mut()
        .unwrap()
        .remove("restartObservation");
    for (index, incomplete) in [no_run, no_baseline].into_iter().enumerate() {
        write_managed(&state, &incomplete);
        assert!(
            matches!(bridge.request(json!({"type":"chat","pane":pane})).await,
            Message::Result { error:Some(error),.. } if error.code == ErrorCode::Unsupported)
        );
        let Message::Result {
            sessions: Some(sessions),
            ..
        } = bridge.request(json!({"type":"list"})).await
        else {
            panic!("session list missing");
        };
        let listed = sessions
            .iter()
            .flat_map(|session| &session.panes)
            .find(|candidate| candidate.id == pane)
            .unwrap();
        assert!(
            listed.agent.is_none(),
            "an incomplete named start cannot reuse pane tags"
        );

        write_managed(&state, &confirmed);
        assert!(is_ok(
            &bridge.request(json!({"type":"chat","pane":pane})).await
        ));
        let (chat, _, reset, _, _) = entries(&mut bridge).await;
        assert!(reset);
        write_managed(&state, &incomplete);
        let marker = format!("unconfirmed-{index}");
        claude.append(&json!({"type":"user","uuid":marker,"message":{"content":"synthetic unconfirmed entry"}}).to_string());
        loop {
            match bridge
                .event(WAIT)
                .await
                .expect("incomplete named start did not end Chat")
            {
                Message::ChatEnded { chat: ended, .. } => {
                    assert_eq!(ended, chat);
                    break;
                }
                Message::ChatEntries { entries, .. } => assert!(
                    !entries
                        .iter()
                        .any(|entry| entry.id == format!("u:{marker}"))
                ),
                _ => {}
            }
        }
    }
}

#[tokio::test]
async fn managed_restart_ends_old_chat_until_a_fresh_process_bound_session_exists() {
    let server = Server::start();
    let (pane, pid) = agent_pane(&server);
    let claude = ClaudeDir::new("managed-restart", &pid, &pane);
    let state = claude.dir.join("state");
    let (mut bridge, _) = Bridge::start(
        &server,
        &[
            "--agent-dir",
            &claude.arg(),
            "--state-dir",
            state.to_str().unwrap(),
        ],
    )
    .await;
    assert!(is_ok(
        &bridge.request(json!({"type":"chat","pane":pane})).await
    ));
    let (chat, _, _, _, _) = entries(&mut bridge).await;
    server.tmux(&["respawn-pane", "-k", "-t", &pane, "sleep 600"]);
    let new_pid = server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_pid}"]);
    assert_ne!(new_pid, pid);
    let mut record = managed_record(&server, &pane, &new_pid, &state);
    record["runs"][0]["restartObservation"]["source"] = json!({
        "agent":"claude-code","session":SESSION_ID,"transcript":claude.transcript,
    });
    write_managed(&state, &record);
    // Even a retained native session file rewritten with the new PID is old evidence.
    claude.write_session(&new_pid, &pane, "idle");
    claude.append(
        r#"{"type":"user","uuid":"old-after-restart","message":{"content":"old fixture"}}"#,
    );
    loop {
        match bridge.event(WAIT).await.expect("managed chat did not end") {
            Message::ChatEnded { chat: ended, .. } => {
                assert_eq!(ended, chat);
                break;
            }
            Message::ChatEntries { entries, .. } => {
                assert!(
                    !entries
                        .iter()
                        .any(|entry| entry.id == "u:old-after-restart")
                );
            }
            _ => {}
        }
    }
    let Message::Result {
        error: Some(error), ..
    } = bridge.request(json!({"type":"chat","pane":pane})).await
    else {
        panic!("old native session was opened for a managed replacement");
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    let next_id = "0b8c3c0e-1111-4a2b-9c3d-000000000003";
    let next_transcript = claude
        .dir
        .join(format!("projects/-work-app/{next_id}.jsonl"));
    std::fs::write(
        &next_transcript,
        concat!(
            r#"{"type":"user","uuid":"managed-fresh","message":{"content":"fresh fixture"}}"#,
            "\n"
        ),
    )
    .unwrap();
    let mut native: Value =
        serde_json::from_slice(&std::fs::read(&claude.session).unwrap()).unwrap();
    native["sessionId"] = next_id.into();
    // Identity proof permits Chat even when the adapter reports no normalized state.
    native["status"] = "not-a-known-state".into();
    std::fs::write(&claude.session, serde_json::to_vec(&native).unwrap()).unwrap();
    assert!(is_ok(
        &bridge.request(json!({"type":"chat","pane":pane})).await
    ));
    let (_, _, reset, ids, status) = entries(&mut bridge).await;
    assert!(reset);
    assert_eq!(ids, ["u:managed-fresh"]);
    assert_eq!(status, None);
    // A native session can change below the same pane root without replacing its PID.
    let newest_id = "0b8c3c0e-1111-4a2b-9c3d-000000000004";
    let newest_transcript = claude
        .dir
        .join(format!("projects/-work-app/{newest_id}.jsonl"));
    std::fs::write(&newest_transcript, concat!(r#"{"type":"user","uuid":"managed-newest","message":{"content":"next native fixture"}}"#, "\n")).unwrap();
    native["sessionId"] = newest_id.into();
    std::fs::write(
        claude.dir.join("sessions/newest.json"),
        serde_json::to_vec(&native).unwrap(),
    )
    .unwrap();
    let mut retired = std::fs::OpenOptions::new()
        .append(true)
        .open(&next_transcript)
        .unwrap();
    writeln!(
        retired,
        r#"{{"type":"user","uuid":"retired-native","message":{{"content":"retired fixture"}}}}"#
    )
    .unwrap();
    let (_, _, reset, ids, _) = entries(&mut bridge).await;
    assert!(reset);
    assert_eq!(ids, ["u:managed-newest"]);
    let mut unknown = record.clone();
    unknown["runs"][0]["restartObservation"] = json!({"version":99});
    write_managed(&state, &unknown);
    loop {
        if matches!(
            bridge
                .event(WAIT)
                .await
                .expect("unknown observation did not end chat"),
            Message::ChatEnded { .. }
        ) {
            break;
        }
    }
    let Message::Result {
        error: Some(error), ..
    } = bridge.request(json!({"type":"chat","pane":pane})).await
    else {
        panic!("unknown managed observation was accepted");
    };
    assert_eq!(error.code, ErrorCode::Unsupported);
    unknown = record.clone();
    unknown["restartOperation"] = json!({"version":99});
    write_managed(&state, &unknown);
    assert!(
        matches!(bridge.request(json!({"type":"chat","pane":pane})).await,
        Message::Result { error: Some(error), .. } if error.code == ErrorCode::Unsupported)
    );
    unknown = record;
    unknown["runs"][0]["outcome"] = "unconfirmed".into();
    write_managed(&state, &unknown);
    assert!(
        matches!(bridge.request(json!({"type":"chat","pane":pane})).await,
        Message::Result { error: Some(error), .. } if error.code == ErrorCode::Unsupported)
    );
}

#[tokio::test]
async fn a_managed_restart_does_not_open_a_codex_cwd_time_match() {
    let server = Server::start();
    let codex = CodexDir::new("managed-weak-proof");
    let (pane, cwd) = codex.start(&server, &["new-window", "-d"]);
    codex.rollout(&cwd);
    let pid = server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_pid}"]);
    let state = codex.0.join("state");
    let record = managed_record(&server, &pane, &pid, &state);
    write_managed(&state, &record);
    let (mut bridge, _) = Bridge::start(
        &server,
        &[
            "--agent-dir",
            &codex.arg(),
            "--state-dir",
            state.to_str().unwrap(),
        ],
    )
    .await;
    assert!(
        matches!(bridge.request(json!({"type":"chat","pane":pane})).await,
        Message::Result { error: Some(error), .. } if error.code == ErrorCode::Unsupported)
    );
}
