//! Codex threads muxa owns (ADR 0028) end to end: a real daemon on a
//! throwaway tmux server runs a Routine on Codex, played by
//! `data/codex-fake.py`, which serves the App Server protocol and a viewer
//! for `codex resume`. It never calls a model and never reads `~/.codex`.
//! Checked: one thread per run, stored before the pane starts; turns go by
//! id and nothing is typed; no input while the thread is not idle; an
//! uncertain send is not retried; the state comes from the thread; logs
//! hold no content; the user's layout does not change.
#![cfg(feature = "tmux-integration")]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use common::daemon::{Conn, Daemon};
use common::{Bridge, Server};
use muxa_protocol::{AgentState, Message, RoutineInfo};
use serde_json::{Value, json};

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/codex-fake.py");
/// Content that must never reach the daemon's log (invariant 5).
const MARKS: [&str; 2] = ["instruction-mark-c0d3", "title-mark-7e11"];
const START: &str = "Read .muxa/TASK.md and start.";
const RUN_WAIT: Duration = Duration::from_secs(90);

struct Host {
    base: PathBuf,
}

impl Host {
    fn new(name: &str) -> Host {
        let base =
            std::env::temp_dir().join(format!("muxa-codex-threads-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        for d in ["root/.muxa", "state", "home/fake", "bin"] {
            std::fs::create_dir_all(base.join(d)).unwrap();
        }
        let base = base.canonicalize().unwrap();
        // The program's name is `codex`, as a workspace's command names it.
        let program = base.join("bin/codex");
        std::fs::copy(FAKE, &program).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let muxa = base.join("root/.muxa");
        std::fs::write(muxa.join("brief.md"), format!("Read. {}\n", MARKS[0])).unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[codex]\nowned_threads = true\n\
                 [[workspace]]\nname = \"w\"\nroot = \"{root}\"\n\
                 [workspace.agent]\ncommand = \"{program}\"\n\
                 env = {{ CODEX_HOME = \"{home}\" }}\n\
                 [workspace.routines]\nenabled = true\n",
                root = base.join("root").display(),
                program = program.display(),
                home = base.join("home").display(),
            ),
        )
        .unwrap();
        std::fs::write(
            muxa.join("routines.toml"),
            "[[routine]]\nname = \"Brief\"\ninstructions = \"brief.md\"\nenabled = true\n\
             [routine.output]\nitems = 5\n",
        )
        .unwrap();
        Host { base }
    }

    fn flags(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.base.join("state").display().to_string(),
            "--workspaces".into(),
            self.base.join("workspaces.toml").display().to_string(),
            "--task-launcher".into(),
            "builtin".into(),
        ]
    }

    fn fake(&self, name: &str) -> PathBuf {
        self.base.join("home/fake").join(name)
    }

    fn steer(&self, name: &str, text: &str) {
        std::fs::write(self.fake(name), text).unwrap();
    }

    fn unsteer(&self, name: &str) {
        std::fs::remove_file(self.fake(name)).unwrap();
    }

    fn log(&self, name: &str) -> Vec<String> {
        std::fs::read_to_string(self.fake(name))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The first turn writes a passing output with one briefing.
    fn passing_turn(&self) {
        let output = json!({"version": 1, "items": [{
            "kind": "briefing", "key": "k1", "title": format!("Moved {}", MARKS[1]),
            "links": [{"url": "https://example.com/1"}]}]});
        let turn = json!({"path": ".muxa/routine/output.json", "content": output});
        self.steer("turn-1.json", &turn.to_string());
    }

    fn record(&self, id: &str) -> Value {
        let text = std::fs::read_to_string(self.base.join(format!("state/{id}.json"))).unwrap();
        serde_json::from_str(&text).unwrap()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

async fn brief(conn: &mut Conn) -> RoutineInfo {
    match conn
        .request(json!({"type": "list-routines"}))
        .await
        .as_slice()
    {
        [
            Message::RoutineList { entries, .. },
            Message::Result { error: None, .. },
        ] => entries.iter().find(|r| r.name == "Brief").cloned().unwrap(),
        other => panic!("list-routines: {other:?}"),
    }
}

/// Starts the Routine now and returns its run's record id.
async fn run_now(conn: &mut Conn) -> String {
    let answer = conn
        .request(json!({"type": "run-routine", "workspace": "w", "routine": "Brief"}))
        .await;
    match answer.as_slice() {
        [
            Message::RoutineSaved { entry, .. },
            Message::Result { error: None, .. },
        ] => entry.running.clone().and_then(|r| r.task).expect("a run"),
        other => panic!("run-routine: {other:?}"),
    }
}

async fn until(what: &str, wait: Duration, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + wait;
    while !cond() {
        assert!(Instant::now() < deadline, "timed out: {what}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

/// The desktop's own session, as `list-panes` shows it.
fn desktop(server: &Server) -> String {
    server.tmux(&[
        "list-panes",
        "-t",
        "=work:",
        "-F",
        "#{window_layout} #{pane_id} #{pane_active} #{pane_width}x#{pane_height} #{pane_pid}",
    ])
}

/// The run's agent pane state, as a bridge lists it.
async fn state(bridge: &mut Bridge, task: &str) -> Option<AgentState> {
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("no sessions")
    };
    let session = sessions
        .iter()
        .find(|s| s.task_id.as_deref() == Some(task))?;
    session
        .panes
        .iter()
        .find_map(|p| p.agent.as_ref().map(|a| a.state))
}

fn thread_of(record: &Value) -> String {
    let thread = &record["codexThread"];
    assert_eq!(thread["version"], 1, "{record}");
    assert_eq!(thread["run"], 1, "{record}");
    thread["id"].as_str().expect("a thread id").to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routine_runs_on_its_own_thread_and_nothing_is_typed() {
    let server = Server::start();
    let h = Host::new("run");
    h.passing_turn();
    // The thread reads active until the test lets it go idle.
    h.steer("status", "active");
    let flags = h.flags();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&server, &flags).await;
    let mut conn = daemon.connect().await;
    let (mut bridge, _) = Bridge::start(&server, &flags).await;
    let before = desktop(&server);

    let run = run_now(&mut conn).await;
    let record = h.record(&run);
    let thread = thread_of(&record);
    let starts = h.log("starts.log");
    assert_eq!(starts.len(), 1, "one thread per run: {starts:?}");
    let start: Value = serde_json::from_str(&starts[0]).unwrap();
    assert_eq!(start["historyMode"], "legacy");
    assert_eq!(start["cwd"], record["dir"]);
    until("the viewer resumes the thread", RUN_WAIT, || {
        !h.log("resume.log").is_empty()
    })
    .await;
    assert_eq!(h.log("resume.log"), std::slice::from_ref(&thread));

    // Not idle: no input, and the state is the thread's.
    let mut seen = None;
    for _ in 0..20 {
        seen = state(&mut bridge, &run).await;
        if seen == Some(AgentState::Running) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(seen, Some(AgentState::Running));
    h.steer("status", "approval");
    let mut seen = None;
    for _ in 0..20 {
        seen = state(&mut bridge, &run).await;
        if seen == Some(AgentState::Waiting) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(seen, Some(AgentState::Waiting), "an approval is waiting");
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(h.log("turns.log").is_empty(), "no input while not idle");

    h.unsteer("status");
    let done = {
        let deadline = Instant::now() + RUN_WAIT;
        loop {
            let r = brief(&mut conn).await;
            if r.running.is_none() {
                break r;
            }
            assert!(Instant::now() < deadline, "the run did not end: {r:?}");
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    };
    assert_eq!(done.last_success.expect("a publish").task, run);
    assert_eq!(h.log("turns.log"), [format!("{thread}\t{START}")]);
    assert!(h.log("typed.log").is_empty(), "nothing typed into the pane");
    assert_eq!(h.log("starts.log").len(), 1);
    let record = h.record(&run);
    let first = &record["runs"][0];
    assert_eq!(first["agent"], "codex", "{record}");
    assert_eq!(first["agentSession"], thread.as_str(), "{record}");

    assert_eq!(desktop(&server), before, "the desktop's layout changed");
    let log = daemon.log();
    for mark in MARKS.iter().chain([&START]) {
        assert!(!log.contains(mark), "{mark} reached the log");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_uncertain_turn_is_never_sent_again() {
    let server = Server::start();
    let h = Host::new("uncertain");
    h.passing_turn();
    h.steer("drop-turn", "");
    let flags = h.flags();
    let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
    let daemon = Daemon::start(&server, &flags).await;
    let mut conn = daemon.connect().await;

    let run = run_now(&mut conn).await;
    let thread = thread_of(&h.record(&run));
    until("the turn is received", RUN_WAIT, || {
        !h.log("turns.log").is_empty()
    })
    .await;
    // Several sender polls and loop ticks later, still one.
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(h.log("turns.log"), [format!("{thread}\t{START}")]);
    assert!(h.log("typed.log").is_empty());
    let record = h.record(&run);
    assert_eq!(
        record["restartOperation"]["phase"], "delivering",
        "the claim stays for recovery: {record}"
    );
    assert!(brief(&mut conn).await.running.is_some());
    assert_eq!(h.log("starts.log").len(), 1);
}
