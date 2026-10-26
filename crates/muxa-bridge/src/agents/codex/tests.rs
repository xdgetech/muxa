use std::io::Write;

use super::*;

const SAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/codex/rollout.jsonl"
);

fn sample() -> String {
    std::fs::read_to_string(SAMPLE).unwrap()
}

fn normalize(text: &str) -> Conversation {
    let mut conv = Conversation::default();
    for line in text.lines() {
        conv.apply(line.as_bytes());
    }
    conv
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

/// pid 100 is the pane's shell; 200 is Codex below it, started a minute ago.
fn procs() -> Processes {
    Processes::parse(
        "1 0 10:00 /sbin/launchd\n100 1 05:00 -zsh\n200 100 01:00 /opt/homebrew/bin/codex\n",
    )
}

/// A Codex data directory with rollouts in today's day directory.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-codex-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    /// The sample with its cwd set to `cwd`, at `sessions/<today>/<name>`.
    fn rollout(&self, name: &str, cwd: &str) -> PathBuf {
        let day = self.dir.join("sessions").join(day_key(SystemTime::now()));
        std::fs::create_dir_all(&day).unwrap();
        let mut lines: Vec<String> = sample().lines().map(str::to_owned).collect();
        let mut meta: Value = serde_json::from_str(&lines[0]).unwrap();
        meta["payload"]["cwd"] = cwd.into();
        lines[0] = meta.to_string();
        let path = day.join(name);
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    fn source(&self, list: impl Fn() -> Result<Processes, Error> + Send + 'static) -> CodexSource {
        let started = started(&pane(), &procs()).unwrap();
        let path = find_rollout(&self.dir, "/work/app", started).unwrap();
        CodexSource::new(self.dir.clone(), started, path, Box::new(list))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn normalizes_the_sample_rollout() {
    let conv = normalize(&sample());
    let summary: Vec<(&str, ChatKind, &str)> = conv
        .log
        .all()
        .iter()
        .map(|e| (e.id.as_str(), e.kind, e.text.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            (
                "m:msg_user_2",
                ChatKind::User,
                "Why does the parser test fail?"
            ),
            ("m:msg_asst_1", ChatKind::Assistant, "I will run the test."),
            ("t:call_exec_1", ChatKind::Tool, ""),
            ("t:call_shell_1", ChatKind::Tool, ""),
            (
                "m:msg_asst_2",
                ChatKind::Assistant,
                "The test expects a trailing newline."
            ),
        ],
        "developer messages, AGENTS.md, environment context and reasoning are left out"
    );
    let exec = conv.log.get("t:call_exec_1").unwrap().tool.clone().unwrap();
    assert_eq!(exec.name, "exec");
    assert_eq!(exec.summary, "cargo test -p parser");
    assert_eq!(
        exec.result.as_deref(),
        Some("test result: FAILED. 1 failed")
    );
    let shell = conv
        .log
        .get("t:call_shell_1")
        .unwrap()
        .tool
        .clone()
        .unwrap();
    assert_eq!(shell.summary, "cat src/parser.rs");
    assert_eq!(shell.result.as_deref(), Some("fn parse() {}"));
    assert_eq!(
        conv.turn,
        Some(Agent {
            state: AgentState::Idle,
            since: Some(1_792_135_828)
        })
    );
}

#[test]
fn a_prompt_that_starts_with_a_tag_is_kept() {
    assert!(injected(
        "<environment_context>\n  <cwd>/a</cwd>\n</environment_context>"
    ));
    assert!(injected("# AGENTS.md instructions for /a\n\nx"));
    assert!(!injected("<div> is not closed here"));
    assert!(!injected("Why is <b>bold</b> broken?"));
}

#[test]
fn turn_events_set_the_state() {
    let started = r#"{"type":"event_msg","payload":{"type":"task_started","started_at":10}}"#;
    let aborted =
        r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#;
    let conv = normalize(started);
    assert_eq!(
        conv.turn.as_ref().map(|a| (a.state, a.since)),
        Some((AgentState::Running, Some(10)))
    );
    let conv = normalize(&format!("{started}\n{aborted}"));
    assert_eq!(
        conv.turn.as_ref().map(|a| (a.state, a.since)),
        Some((AgentState::Idle, None))
    );
}

#[test]
fn finds_the_newest_rollout_in_the_panes_directory() {
    let f = Fixture::new("find");
    f.rollout("rollout-a.jsonl", "/elsewhere");
    let own = f.rollout("rollout-b.jsonl", "/work/app");
    let started = started(&pane(), &procs()).unwrap();
    assert_eq!(
        find_rollout(&f.dir, "/work/app", started).unwrap(),
        Some(own.clone())
    );

    // A rollout from before the process started belongs to an earlier run.
    let later = SystemTime::now() + Duration::from_secs(60);
    assert_eq!(find_rollout(&f.dir, "/work/app", later).unwrap(), None);

    let adapter = CodexAdapter::new(f.dir.clone());
    let found = adapter.find(&pane(), &procs()).unwrap();
    assert_eq!(found.status.map(|a| a.state), Some(AgentState::Idle));
    let shell = Processes::parse("100 1 05:00 -zsh\n");
    assert!(
        adapter.find(&pane(), &shell).is_none(),
        "no codex process, no agent"
    );
    assert!(adapter.open(&pane(), &shell).unwrap().is_none());
}

#[test]
fn day_keys_are_civil_dates() {
    assert_eq!(day_key(UNIX_EPOCH), "1970/01/01");
    assert_eq!(
        day_key(UNIX_EPOCH + Duration::from_secs(1_792_135_818)),
        "2026/10/16"
    );
    assert_eq!(
        day_key(UNIX_EPOCH + Duration::from_secs(951_782_400)),
        "2000/02/29"
    );
}

#[test]
fn streams_new_lines_and_reports_status() {
    let f = Fixture::new("stream");
    let path = f.rollout("rollout-b.jsonl", "/work/app");
    let mut source = f.source(|| Ok(procs()));

    let events = source.poll(&pane()).unwrap();
    let [ChatEvent::Reset { entries, truncated }] = events.as_slice() else {
        panic!("expected one reset, got {events:?}")
    };
    assert_eq!(entries.len(), 5);
    assert!(!truncated);
    assert_eq!(source.status().unwrap().state, AgentState::Idle);
    assert!(source.poll(&pane()).unwrap().is_empty());

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(
        file,
        r#"{{"type":"event_msg","payload":{{"type":"task_started","started_at":1792135900}}}}"#
    )
    .unwrap();
    writeln!(file, r#"{{"type":"response_item","payload":{{"type":"message","id":"msg_user_3","role":"user","content":[{{"type":"input_text","text":"Fix it."}}]}}}}"#).unwrap();
    let events = source.poll(&pane()).unwrap();
    let [ChatEvent::Upsert(entry)] = events.as_slice() else {
        panic!("expected one upsert, got {events:?}")
    };
    assert_eq!(entry.text, "Fix it.");
    assert_eq!(source.status().unwrap().state, AgentState::Running);

    let shell = PaneInfo {
        shell_foreground: true,
        ..pane()
    };
    source.poll(&shell).unwrap();
    assert_eq!(source.status(), None, "back at a shell, the state is stale");
}

#[test]
fn the_first_rollout_is_picked_up_when_codex_writes_it() {
    let f = Fixture::new("first");
    let mut source = f.source(|| Ok(procs()));
    let events = source.poll(&pane()).unwrap();
    assert_eq!(
        events,
        [ChatEvent::Reset {
            entries: vec![],
            truncated: false
        }]
    );
    for _ in 1..CHECK_EVERY {
        assert!(
            source.poll(&pane()).unwrap().is_empty(),
            "no reset again while waiting"
        );
    }
    f.rollout("rollout-b.jsonl", "/work/app");
    let events = source.poll(&pane()).unwrap();
    let [ChatEvent::Reset { entries, .. }] = events.as_slice() else {
        panic!("expected a reset, got {events:?}")
    };
    assert_eq!(entries.len(), 5);
}

#[test]
fn an_exited_codex_ends_the_chat() {
    let f = Fixture::new("exit");
    f.rollout("rollout-b.jsonl", "/work/app");
    let mut source = f.source(|| Ok(Processes::parse("100 1 05:00 -zsh\n")));
    assert!(matches!(source.poll(&pane()), Err(Error::Gone(_))));
}
