use std::io::Write;

use super::*;

const SAMPLE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/claude-code");
const SESSION_ID: &str = "0b8c3c0e-1111-4a2b-9c3d-000000000001";

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!("{SAMPLE}/{name}")).unwrap()
}

fn normalize(text: &str) -> Vec<ChatEntry> {
    let mut conv = Conversation::default();
    for line in text.lines() {
        conv.apply(line.as_bytes());
    }
    conv.entries
}

fn pane() -> PaneInfo {
    PaneInfo {
        pane: "%7".into(),
        session: "work".into(),
        pid: 100,
        path: "/work/app".into(),
        shell_foreground: false,
    }
}

/// A Claude Code data directory with one session file per (file, pid, tmux, kind).
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-claude-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sessions")).unwrap();
        Self { dir }
    }

    fn session(&self, file: &str, pid: u32, id: &str, tmux: &str, edit: impl Fn(&mut Value)) {
        let mut v: Value = serde_json::from_str(&sample("session.json")).unwrap();
        v["pid"] = pid.into();
        v["sessionId"] = id.into();
        v["tmux"] = tmux.into();
        edit(&mut v);
        std::fs::write(self.dir.join("sessions").join(file), v.to_string()).unwrap();
    }

    fn transcript(&self, id: &str, text: &str) -> PathBuf {
        let dir = self.dir.join("projects").join("-work-app");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{id}.jsonl"));
        std::fs::write(&path, text).unwrap();
        path
    }

    fn open(&self, procs: &Processes) -> Option<Box<dyn ChatSource>> {
        ClaudeAdapter::new(self.dir.clone())
            .open(&pane(), procs)
            .unwrap()
    }
}

/// pid 100 is the pane's shell; 200 runs below it; 300 is elsewhere.
fn procs() -> Processes {
    Processes::parse("1 0\n100 1\n200 100\n300 1\n")
}

#[test]
fn normalizes_the_sample_transcript() {
    let entries = normalize(&sample("transcript.jsonl"));
    let summary: Vec<(&str, ChatKind, &str)> = entries
        .iter()
        .map(|e| (e.id.as_str(), e.kind, e.text.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("u:u1", ChatKind::User, "Fix the failing test"),
            // Lines sharing message.id merge; thinking stays out.
            (
                "a:msg_1",
                ChatKind::Assistant,
                "Running the tests **first**."
            ),
            ("t:toolu_1", ChatKind::Tool, ""),
            // The sidechain reply and the meta line are skipped.
            (
                "a:msg_2",
                ChatKind::Assistant,
                "The test expects a trailing newline."
            ),
            ("t:toolu_2", ChatKind::Tool, ""),
            ("u:u4", ChatKind::Notice, "/model opus"),
            ("u:u6", ChatKind::Notice, "Interrupted"),
        ]
    );
    let bash = entries[2].tool.as_ref().unwrap();
    assert_eq!(
        (bash.name.as_str(), bash.summary.as_str()),
        ("Bash", "cargo test")
    );
    assert_eq!(
        bash.result.as_deref(),
        Some("test result: FAILED. 1 failed")
    );
    assert!(bash.error);
    let edit = entries[4].tool.as_ref().unwrap();
    assert_eq!(edit.summary, "src/lib.rs");
    assert_eq!(edit.result.as_deref(), Some("The file was updated."));
    assert!(!edit.error);
}

#[test]
fn a_result_updates_its_call_and_reports_the_change() {
    let mut conv = Conversation::default();
    let call = r#"{"type":"assistant","uuid":"a","message":{"id":"m","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"a.rs"}}]}}"#;
    let result = r#"{"type":"user","uuid":"u","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"fn main"}]}}"#;
    assert_eq!(conv.apply(call.as_bytes())[0].id, "t:t1");
    let changed = conv.apply(result.as_bytes());
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].id, "t:t1");
    assert_eq!(
        changed[0].tool.as_ref().unwrap().result.as_deref(),
        Some("fn main")
    );
    assert_eq!(conv.entries.len(), 1);
}

#[test]
fn long_text_and_results_are_capped_on_a_char_boundary() {
    let text = "é".repeat(MAX_TEXT);
    let line =
        serde_json::json!({"type":"user","uuid":"u","message":{"content": text}}).to_string();
    let entries = normalize(&line);
    assert!(entries[0].text.len() <= MAX_TEXT + "…".len());
    assert!(entries[0].text.ends_with('…'));
}

#[test]
fn many_entries_keep_only_the_newest() {
    let lines: String = (0..MAX_ENTRIES * 2)
        .map(|i| {
            format!("{{\"type\":\"user\",\"uuid\":\"{i}\",\"message\":{{\"content\":\"p{i}\"}}}}\n")
        })
        .collect();
    let entries = normalize(&lines);
    assert!(entries.len() <= MAX_ENTRIES + MAX_ENTRIES / 5);
    assert_eq!(
        entries.last().unwrap().text,
        format!("p{}", MAX_ENTRIES * 2 - 1)
    );
}

#[test]
fn finds_only_the_live_interactive_instance_of_this_pane() {
    let f = Fixture::new("locate");
    // Exited: its pid no longer runs in the pane.
    f.session(
        "1.json",
        999,
        "11111111-0000-0000-0000-000000000000",
        "work:@1.%7",
        |_| {},
    );
    // Running elsewhere.
    f.session(
        "2.json",
        300,
        "22222222-0000-0000-0000-000000000000",
        "work:@1.%7",
        |_| {},
    );
    // A background task, not the pane's agent.
    f.session(
        "3.json",
        200,
        "33333333-0000-0000-0000-000000000000",
        "work:@1.%7",
        |v| v["kind"] = "bg".into(),
    );
    // Same pane id on another server's session.
    f.session(
        "4.json",
        200,
        "44444444-0000-0000-0000-000000000000",
        "other:@1.%7",
        |_| {},
    );
    // An id that would escape the projects directory.
    f.session("5.json", 200, "../../etc", "work:@1.%7", |_| {});
    assert!(f.open(&procs()).is_none());

    f.session("6.json", 200, SESSION_ID, "work:@1.%7", |_| {});
    let found = find_session(&f.dir, &pane(), &procs()).unwrap().unwrap();
    assert_eq!(found.session_id, SESSION_ID);

    // Two live instances: the one in the pane's directory wins.
    f.session(
        "7.json",
        200,
        "77777777-0000-0000-0000-000000000000",
        "work:@1.%7",
        |v| v["cwd"] = "/elsewhere".into(),
    );
    let found = find_session(&f.dir, &pane(), &procs()).unwrap().unwrap();
    assert_eq!(found.session_id, SESSION_ID);
}

#[test]
fn streams_new_lines_and_reports_status() {
    let f = Fixture::new("stream");
    f.session("200.json", 200, SESSION_ID, "work:@1.%7", |_| {});
    let path = f.transcript(SESSION_ID, &sample("transcript.jsonl"));
    let mut source = f.open(&procs()).unwrap();

    let events = source.poll(&pane()).unwrap();
    let [ChatEvent::Reset { entries, truncated }] = events.as_slice() else {
        panic!("expected one reset, got {events:?}")
    };
    assert_eq!(entries.len(), 7);
    assert!(!truncated);
    assert_eq!(source.status().unwrap().state, AgentState::Running);
    assert!(source.poll(&pane()).unwrap().is_empty());

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, r#"{{"type":"assistant","uuid":"a9","message":{{"id":"msg_2","content":[{{"type":"text","text":"Fixed."}}]}}}}"#).unwrap();
    let events = source.poll(&pane()).unwrap();
    let [ChatEvent::Upsert(entry)] = events.as_slice() else {
        panic!("expected one upsert, got {events:?}")
    };
    assert_eq!(entry.id, "a:msg_2");
    assert_eq!(entry.text, "The test expects a trailing newline.\n\nFixed.");

    // Back at a shell, the state is stale.
    let shell = PaneInfo {
        shell_foreground: true,
        ..pane()
    };
    source.poll(&shell).unwrap();
    assert_eq!(source.status(), None);
}

#[test]
fn a_new_session_id_resets_to_its_transcript() {
    let f = Fixture::new("clear");
    f.session("200.json", 200, SESSION_ID, "work:@1.%7", |_| {});
    f.transcript(SESSION_ID, &sample("transcript.jsonl"));
    let mut source = f.open(&procs()).unwrap();
    source.poll(&pane()).unwrap();

    // `/clear`: same process, new session, its transcript not written yet.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let next = "99999999-0000-0000-0000-000000000000";
    f.session("200.json", 200, next, "work:@1.%7", |v| {
        v["status"] = "idle".into()
    });
    let events = source.poll(&pane()).unwrap();
    assert_eq!(
        events,
        [ChatEvent::Reset {
            entries: vec![],
            truncated: false
        }]
    );
    assert_eq!(source.status().unwrap().state, AgentState::Idle);
    assert!(
        source.poll(&pane()).unwrap().is_empty(),
        "no reset again while waiting"
    );

    f.transcript(
        next,
        "{\"type\":\"user\",\"uuid\":\"n1\",\"message\":{\"content\":\"hello\"}}\n",
    );
    let events = source.poll(&pane()).unwrap();
    let [ChatEvent::Reset { entries, .. }] = events.as_slice() else {
        panic!("expected a reset, got {events:?}")
    };
    assert_eq!(entries[0].text, "hello");
}

#[test]
fn an_exited_instance_ends_the_chat() {
    let f = Fixture::new("exit");
    f.session("200.json", 200, SESSION_ID, "work:@1.%7", |_| {});
    let mut source = f.open(&procs()).unwrap();
    source.poll(&pane()).unwrap();
    std::fs::remove_file(f.dir.join("sessions/200.json")).unwrap();
    assert!(matches!(source.poll(&pane()), Err(Error::Gone(_))));
}
