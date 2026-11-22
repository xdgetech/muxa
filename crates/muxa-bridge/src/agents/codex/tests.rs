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

/// The folder Codex works in.
const FOLDER: &str = "/work/app";

/// pid 200 is Codex, started a minute ago.
fn procs() -> Processes {
    Processes::parse(
        "1 0 10:00 /sbin/launchd\n100 1 05:00 -zsh\n200 100 01:00 /opt/homebrew/bin/codex\n",
    )
}

/// The host as these tests see it: pid 200 works in `cwd`.
fn cwds_in(cwd: &'static str) -> Cwds {
    Arc::new(move |pids: &[u32]| {
        Ok(pids
            .iter()
            .filter(|p| **p == 200)
            .map(|p| (*p, PathBuf::from(cwd)))
            .collect())
    })
}

fn adapter(dirs: Vec<PathBuf>, cwd: &'static str) -> CodexAdapter {
    CodexAdapter {
        dirs,
        list: || Ok(procs()),
        cwds: cwds_in(cwd),
    }
}

fn started() -> SystemTime {
    started_in(Path::new(FOLDER), &procs(), cwds_in(FOLDER).as_ref())
        .unwrap()
        .unwrap()
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
        self.rollout_at(&day_key(SystemTime::now()), name, cwd)
    }

    /// The same at `sessions/<day>/<name>`, `day` as "YYYY/MM/DD".
    fn rollout_at(&self, day: &str, name: &str, cwd: &str) -> PathBuf {
        let day = self.dir.join("sessions").join(day);
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
        let started = started();
        let path =
            find_rollout(std::slice::from_ref(&self.dir), Path::new(FOLDER), started).unwrap();
        CodexSource::new(
            vec![self.dir.clone()],
            PathBuf::from(FOLDER),
            started,
            path,
            Box::new(list),
            cwds_in(FOLDER),
        )
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
fn finds_the_newest_rollout_in_the_folder() {
    let f = Fixture::new("find");
    f.rollout("rollout-a.jsonl", "/elsewhere");
    let own = f.rollout("rollout-b.jsonl", FOLDER);
    let folder = Path::new(FOLDER);
    assert_eq!(
        find_rollout(std::slice::from_ref(&f.dir), folder, started()).unwrap(),
        Some(own.clone())
    );

    // A rollout from before the process started belongs to an earlier run.
    let later = SystemTime::now() + Duration::from_secs(60);
    assert_eq!(
        find_rollout(std::slice::from_ref(&f.dir), folder, later).unwrap(),
        None
    );

    let mut source = adapter(vec![f.dir.clone()], FOLDER)
        .open_folder(folder, &procs())
        .unwrap()
        .unwrap();
    source.poll().unwrap();
    assert_eq!(source.status().map(|a| a.state), Some(AgentState::Idle));
    let shell = Processes::parse("100 1 05:00 -zsh\n");
    assert!(
        adapter(vec![f.dir.clone()], FOLDER)
            .open_folder(folder, &shell)
            .unwrap()
            .is_none(),
        "no codex process, no agent"
    );
    assert!(
        adapter(vec![f.dir.clone()], "/elsewhere")
            .open_folder(folder, &procs())
            .unwrap()
            .is_none(),
        "a codex working in another folder is not this folder's agent"
    );
}

#[test]
fn a_running_codex_has_a_chat_before_its_first_rollout() {
    let f = Fixture::new("no-rollout-yet");
    f.rollout("rollout-a.jsonl", "/elsewhere");
    let mut source = adapter(vec![f.dir.clone()], FOLDER)
        .open_folder(Path::new(FOLDER), &procs())
        .unwrap()
        .unwrap();
    assert_eq!(
        source.poll().unwrap(),
        [ChatEvent::Reset {
            entries: vec![],
            truncated: false
        }]
    );
    assert_eq!(source.status(), None);
}

#[test]
fn only_the_newest_days_and_files_are_read() {
    let f = Fixture::new("bounded");
    // An old day, past the newest `MAX_DAYS` directories.
    f.rollout_at("2000/01/01", "rollout-old.jsonl", FOLDER);
    for i in 0..MAX_DAYS {
        std::fs::create_dir_all(f.dir.join(format!("sessions/2999/01/{:02}", i + 1))).unwrap();
    }
    assert_eq!(
        find_rollout(std::slice::from_ref(&f.dir), Path::new(FOLDER), UNIX_EPOCH).unwrap(),
        None,
        "the old day is not searched"
    );

    // More recent rollouts than `MAX_FILES` for other folders hide an older one.
    let g = Fixture::new("bounded-files");
    let own = g.rollout("rollout-own.jsonl", FOLDER);
    let since = std::fs::metadata(&own).unwrap().modified().unwrap();
    std::thread::sleep(Duration::from_millis(20));
    for i in 1..MAX_FILES {
        g.rollout(&format!("rollout-{i}.jsonl"), "/elsewhere");
    }
    assert_eq!(
        find_rollout(std::slice::from_ref(&g.dir), Path::new(FOLDER), since).unwrap(),
        Some(own)
    );
    std::thread::sleep(Duration::from_millis(20));
    g.rollout("rollout-last.jsonl", "/elsewhere");
    assert_eq!(
        find_rollout(std::slice::from_ref(&g.dir), Path::new(FOLDER), since).unwrap(),
        None
    );
}

#[test]
fn every_codex_home_is_searched_and_the_newest_wins() {
    let default = Fixture::new("homes-default");
    let company = Fixture::new("homes-company");
    default.rollout("rollout-a.jsonl", "/elsewhere");
    let own = company.rollout("rollout-b.jsonl", FOLDER);
    let started = started();
    let homes = vec![default.dir.clone(), company.dir.clone()];
    let folder = Path::new(FOLDER);
    assert_eq!(
        find_rollout(&homes, folder, started).unwrap(),
        Some(own.clone())
    );
    // The same cwd in both homes: the newer file wins.
    std::thread::sleep(Duration::from_millis(20));
    let newer = default.rollout("rollout-c.jsonl", FOLDER);
    assert_eq!(find_rollout(&homes, folder, started).unwrap(), Some(newer));
    let mut source = adapter(vec![default.dir.clone()], FOLDER)
        .open_folder(folder, &procs())
        .unwrap()
        .unwrap();
    source.poll().unwrap();
    assert!(source.status().is_some());
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
    let path = f.rollout("rollout-b.jsonl", FOLDER);
    let mut source = f.source(|| Ok(procs()));

    let events = source.poll().unwrap();
    let [ChatEvent::Reset { entries, truncated }] = events.as_slice() else {
        panic!("expected one reset, got {events:?}")
    };
    assert_eq!(entries.len(), 5);
    assert!(!truncated);
    assert_eq!(source.status().unwrap().state, AgentState::Idle);
    assert!(source.poll().unwrap().is_empty());

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
    let events = source.poll().unwrap();
    let [ChatEvent::Upsert(entry)] = events.as_slice() else {
        panic!("expected one upsert, got {events:?}")
    };
    assert_eq!(entry.text, "Fix it.");
    assert_eq!(source.status().unwrap().state, AgentState::Running);
}

#[test]
fn the_first_rollout_is_picked_up_when_codex_writes_it() {
    let f = Fixture::new("first");
    let mut source = f.source(|| Ok(procs()));
    let events = source.poll().unwrap();
    assert_eq!(
        events,
        [ChatEvent::Reset {
            entries: vec![],
            truncated: false
        }]
    );
    for _ in 1..CHECK_EVERY {
        assert!(
            source.poll().unwrap().is_empty(),
            "no reset again while waiting"
        );
    }
    f.rollout("rollout-b.jsonl", FOLDER);
    let events = source.poll().unwrap();
    let [ChatEvent::Reset { entries, .. }] = events.as_slice() else {
        panic!("expected a reset, got {events:?}")
    };
    assert_eq!(entries.len(), 5);
}

#[test]
fn an_exited_codex_ends_the_chat() {
    let f = Fixture::new("exit");
    f.rollout("rollout-b.jsonl", FOLDER);
    let mut source = f.source(|| Ok(Processes::parse("100 1 05:00 -zsh\n")));
    assert!(matches!(source.poll(), Err(Error::Gone(_))));
}

#[test]
fn completion_state_does_not_distinguish_same_second_native_turns() {
    let complete = |turn: &str, ordinal: u64| {
        serde_json::json!({
            "ordinal": ordinal,
            "type": "event_msg",
            "payload": {"type": "task_complete", "turn_id": turn, "completed_at": 20}
        })
        .to_string()
    };
    let first = normalize(&complete("old-turn", 10));
    let next = normalize(&complete("close-out-turn", 12));
    assert_eq!(first.turn, next.turn);
    assert_eq!(first.turn.unwrap().since, Some(20));
    // A close-out must use an ordered native observation, not this summary alone.
    assert!(first.log.all().is_empty());
    assert!(next.log.all().is_empty());
}

#[test]
fn abort_and_undated_completion_are_idle_without_completion_time() {
    for event in ["turn_aborted", "task_complete"] {
        let line = serde_json::json!({"type":"event_msg", "payload":{"type":event,"turn_id":"synthetic-turn"}}).to_string();
        let conv = normalize(&line);
        assert_eq!(
            conv.turn,
            Some(Agent {
                state: AgentState::Idle,
                since: None
            })
        );
    }
    let mut conv =
        normalize(r#"{"type":"event_msg","payload":{"type":"task_complete","completed_at":20}}"#);
    conv.apply(br#"{"type":"event_msg","payload":{"type":"task_started","started_at":21}}"#);
    assert_eq!(
        conv.turn,
        Some(Agent {
            state: AgentState::Running,
            since: Some(21)
        })
    );
}
