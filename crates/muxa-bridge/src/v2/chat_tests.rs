use std::io::Write;
use std::path::PathBuf;

use muxa_protocol::{ChatKind, PromptKind};
use serde_json::Value;

use super::*;
use crate::agents::claude::ClaudeAdapter;

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/claude-code");
const SESSION_ID: &str = "0b8c3c0e-1111-4a2b-9c3d-000000000002";

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{SAMPLE}/{name}")).unwrap()
}

/// A Claude Code config dir whose one live session (this test process)
/// works in a Task folder.
struct Fixture {
    base: PathBuf,
    config: PathBuf,
    task: PathBuf,
}

impl Fixture {
    fn new(name: &str, status: &str, transcript: &str) -> Fixture {
        let base = std::env::temp_dir().join(format!("muxa-v2-chat-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("config/sessions")).unwrap();
        std::fs::create_dir_all(base.join("root/tasks/fix-1")).unwrap();
        let base = base.canonicalize().unwrap();
        let (config, task) = (base.join("config"), base.join("root/tasks/fix-1"));
        let mut v: Value = serde_json::from_str(&sample("session.json")).unwrap();
        v["pid"] = std::process::id().into();
        v["sessionId"] = SESSION_ID.into();
        v["cwd"] = task.display().to_string().into();
        v["status"] = status.into();
        std::fs::write(config.join("sessions/1.json"), v.to_string()).unwrap();
        let escaped: String = task
            .display()
            .to_string()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let projects = config.join("projects").join(escaped);
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(projects.join(format!("{SESSION_ID}.jsonl")), transcript).unwrap();
        Fixture { base, config, task }
    }

    fn chats(&self) -> Chats {
        Chats::new(Arc::new(Registry::new(vec![Box::new(ClaudeAdapter::new(
            vec![self.config.clone()],
        ))])))
    }

    fn transcript(&self) -> PathBuf {
        let projects = self.config.join("projects");
        let dir = std::fs::read_dir(projects)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        dir.join(format!("{SESSION_ID}.jsonl"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

#[tokio::test]
async fn a_chat_opens_with_the_conversation_then_sends_what_changed() {
    let f = Fixture::new("follow", "busy", &sample("transcript.jsonl"));
    let chats = f.chats();
    let (handle, first) = chats.open(&f.task).await.unwrap();
    assert!(first.reset);
    assert!(!first.entries.is_empty());
    assert_eq!(first.prompt, None);
    assert_eq!(chats.poll(handle).await.unwrap(), None, "nothing changed");

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(f.transcript())
        .unwrap();
    writeln!(
        file,
        r#"{{"type":"user","uuid":"u-new","message":{{"role":"user","content":"Ship it"}}}}"#
    )
    .unwrap();
    let next = chats.poll(handle).await.unwrap().unwrap();
    assert!(!next.reset);
    assert_eq!(next.entries.len(), 1);
    assert_eq!(
        (next.entries[0].kind, next.entries[0].text.as_str()),
        (ChatKind::User, "Ship it")
    );

    // Claude Code exited: its session file is gone.
    std::fs::remove_file(f.config.join("sessions/1.json")).unwrap();
    assert!(chats.poll(handle).await.is_err());
}

#[tokio::test]
async fn a_waiting_agent_offers_the_menus_checked_keys_as_input() {
    let f = Fixture::new("waiting", "waiting", &sample("waiting-permission.jsonl"));
    let (_, first) = f.chats().open(&f.task).await.unwrap();
    let prompt = first.prompt.unwrap();
    assert_eq!(prompt.kind, PromptKind::Permission);
    let options: Vec<(&str, &str, bool)> = prompt
        .options
        .iter()
        .map(|o| (o.label.as_str(), o.input.as_str(), o.cancel))
        .collect();
    assert_eq!(options, [("Allow", "1", false), ("Deny", "\x1b", true)]);
}

#[tokio::test]
async fn a_folder_with_no_live_agent_has_no_chat() {
    let f = Fixture::new("none", "busy", "");
    let elsewhere = f.base.join("root");
    assert!(matches!(
        f.chats().open(&elsewhere).await,
        Err(TaskError::Invalid(_))
    ));
}

#[test]
fn an_unknown_key_drops_the_prompt() {
    let p = muxa_protocol::ChatPrompt {
        entry: "t:1".into(),
        kind: PromptKind::Permission,
        question: None,
        options: vec![muxa_protocol::PromptOption {
            label: "Allow always".into(),
            keys: vec!["Down".into(), "Enter".into()],
            cancel: false,
        }],
    };
    assert_eq!(prompt(p), None);
}

#[tokio::test]
async fn an_entry_changed_in_the_same_poll_keeps_its_place() {
    let f = Fixture::new("order", "busy", &sample("transcript.jsonl"));
    let chats = f.chats();
    let (handle, _) = chats.open(&f.task).await.unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(f.transcript())
        .unwrap();
    // Two calls in parallel; the second finishes first. Claude Code writes
    // each block and each result as its own line.
    for line in [
        r#"{"type":"assistant","uuid":"a1","message":{"id":"m-1","content":[{"type":"text","text":"Looking"}]}}"#,
        r#"{"type":"assistant","uuid":"a2","message":{"id":"m-1","content":[{"type":"tool_use","id":"tu-1","name":"Bash","input":{"command":"ls"}}]}}"#,
        r#"{"type":"assistant","uuid":"a3","message":{"id":"m-1","content":[{"type":"tool_use","id":"tu-2","name":"Bash","input":{"command":"pwd"}}]}}"#,
        r#"{"type":"user","uuid":"u1","message":{"content":[{"type":"tool_result","tool_use_id":"tu-2","content":"/"}]}}"#,
        r#"{"type":"user","uuid":"u2","message":{"content":[{"type":"tool_result","tool_use_id":"tu-1","content":"ok"}]}}"#,
        r#"{"type":"assistant","uuid":"a4","message":{"id":"m-2","content":[{"type":"text","text":"Done"}]}}"#,
    ] {
        writeln!(file, "{line}").unwrap();
    }
    let next = chats.poll(handle).await.unwrap().unwrap();
    let ids: Vec<&str> = next.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["a:m-1", "t:tu-1", "t:tu-2", "a:m-2"]);
    assert_eq!(
        next.entries[1].tool.as_ref().unwrap().result.as_deref(),
        Some("ok")
    );
}
