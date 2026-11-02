//! The Task Loop (ADR 0019) end to end: a real daemon, a throwaway tmux
//! server, and the fake agent of `data/loop-agent.py` in the Task's pane.
//! Records are written as files, so only the loop goes through the socket.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use common::daemon::{Conn, Daemon};
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, LoopReason, LoopState, Message, TaskLoop, TaskRecord, TaskStatus};
use serde_json::{Value, json};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/loop-agent.py");
const PTY_CLIENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/lib/pty-client.py"
);
/// Content that must never reach the daemon's log (invariant 5).
const GOAL_MARK: &str = "goal-marker-7c1e";
const FAIL_PROMPT: &str = "The check failed.";
/// A loop reaches its end well within this, on a loaded machine too.
const LOOP_WAIT: Duration = Duration::from_secs(120);

static NEXT: AtomicU32 = AtomicU32::new(1);

/// A Task with a running fake agent, and the daemon that runs its loop.
struct Host {
    server: Server,
    base: PathBuf,
    /// The Task's directory, where the agent and the check run.
    dir: PathBuf,
    task: String,
    pane: String,
    session: String,
    daemon: Option<Daemon>,
}

impl Host {
    /// Sets up the Task and its agent; `files` steer the agent (see the script).
    /// The daemon is not started yet.
    fn new(name: &str, files: &[&str]) -> Host {
        let server = Server::start();
        let base =
            std::env::temp_dir().join(format!("muxa-loops-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        let dir = root.join("tasks/fix");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(base.join("state")).unwrap();
        let (base, dir) = (base.canonicalize().unwrap(), dir.canonicalize().unwrap());
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n\
                 [workspace.agent]\nenv = {{ LOOP_MARK = \"from-workspace\" }}\n\
                 [workspace.loop]\nchecks = \"any\"\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        for f in files {
            let (name, text) = f.split_once('=').unwrap_or((f, ""));
            std::fs::write(dir.join(name), text).unwrap();
        }
        let task = format!("t-{:06x}", 0xa000 + NEXT.fetch_add(1, Ordering::SeqCst));
        let mut host = Host {
            server,
            base,
            dir,
            task,
            pane: String::new(),
            session: String::new(),
            daemon: None,
        };
        host.write_record(json!({}));
        let made = host.server.tmux(&[
            "new-session",
            "-d",
            "-P",
            "-F",
            "#{session_id} #{pane_id}",
            "-s",
            "task",
            "-x",
            "80",
            "-y",
            "24",
            "-c",
            &host.dir.display().to_string(),
            "python3",
            AGENT,
        ]);
        let (session, pane) = made.split_once(' ').expect("session and pane");
        (host.session, host.pane) = (session.to_owned(), pane.to_owned());
        host.server
            .tmux(&["set-option", "-t", &host.session, "@muxa_task", &host.task]);
        host.wait(Duration::from_secs(20), "the agent to start", |h| {
            h.option("@agent_state").is_some()
        });
        host
    }

    /// `<state>/<id>.json`, with `more` fields on top.
    fn write_record(&self, more: Value) {
        let mut r = json!({
            "version": 1, "id": self.task, "rev": 1, "workspace": "w",
            "title": "Fix the login", "goal": format!("Make it work. {GOAL_MARK}"),
            "status": "open", "createdAt": 1, "updatedAt": 1,
            "dir": self.dir.display().to_string(),
        });
        if let (Value::Object(r), Value::Object(more)) = (&mut r, more) {
            r.extend(more);
        }
        std::fs::write(
            self.base.join(format!("state/{}.json", self.task)),
            r.to_string(),
        )
        .unwrap();
    }

    fn flags(&self) -> Vec<String> {
        vec![
            "--state-dir".into(),
            self.base.join("state").display().to_string(),
            "--workspaces".into(),
            self.base.join("workspaces.toml").display().to_string(),
        ]
    }

    async fn start_daemon(&mut self) {
        let flags = self.flags();
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        self.daemon = Some(Daemon::start(&self.server, &flags).await);
    }

    async fn restart_daemon(&mut self) {
        let flags = self.flags();
        let flags: Vec<&str> = flags.iter().map(String::as_str).collect();
        let d = self.daemon.as_mut().expect("a daemon");
        d.start_again(&self.server, &flags).await;
    }

    fn daemon(&mut self) -> &mut Daemon {
        self.daemon.as_mut().expect("a daemon")
    }

    async fn conn(&mut self) -> Conn {
        self.daemon().connect().await
    }

    async fn start_loop(&mut self, check: &str, max_attempts: u32, timeout: u64) -> TaskRecord {
        let task = self.task.clone();
        let answer = self
            .conn()
            .await
            .request(json!({"type": "start-loop", "task": task,
                "loop": {"check": check, "maxAttempts": max_attempts, "timeout": timeout}}))
            .await;
        match answer.as_slice() {
            [
                Message::TaskSaved { record, .. },
                Message::Result { error: None, .. },
            ] => record.clone(),
            other => panic!("start-loop: {other:?}"),
        }
    }

    fn record(&self) -> Value {
        let text = std::fs::read_to_string(self.base.join(format!("state/{}.json", self.task)))
            .expect("read the record");
        serde_json::from_str(&text).expect("a record")
    }

    fn task_loop(&self) -> TaskLoop {
        serde_json::from_value(self.record()["loop"].clone()).expect("a loop")
    }

    fn option(&self, name: &str) -> Option<String> {
        let v = self
            .server
            .tmux(&["show-options", "-pqv", "-t", &self.pane, name]);
        (!v.is_empty()).then_some(v)
    }

    fn wait(&self, limit: Duration, what: &str, cond: impl Fn(&Host) -> bool) {
        let deadline = Instant::now() + limit;
        while !cond(self) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; loop: {}; turns: {:?}",
                self.record()["loop"],
                self.turns()
            );
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    async fn wait_loop(&self, what: &str, cond: impl Fn(&TaskLoop) -> bool) -> TaskLoop {
        let deadline = Instant::now() + LOOP_WAIT;
        loop {
            if let Ok(lp) = serde_json::from_value::<TaskLoop>(self.record()["loop"].clone()) {
                if cond(&lp) {
                    return lp;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; loop: {}; turns: {:?}",
                self.record()["loop"],
                self.turns()
            );
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    fn turns(&self) -> Vec<String> {
        read_lines(&self.dir.join("turns.log"))
    }

    fn fail_prompts(&self) -> usize {
        self.turns()
            .iter()
            .filter(|t| t.contains(FAIL_PROMPT))
            .count()
    }

    /// Types a line into the agent's pane from outside any client, as a script would.
    fn user_types(&self, text: &str) {
        self.server
            .tmux(&["send-keys", "-t", &self.pane, "-l", text]);
        self.server.tmux(&["send-keys", "-t", &self.pane, "Enter"]);
    }

    /// The daemon's log holds none of the Task's content (invariant 5).
    fn assert_log_clean(&mut self, also: &[&str]) {
        let log = self.daemon().log();
        assert!(!log.is_empty(), "the daemon logged nothing");
        for mark in [GOAL_MARK, FAIL_PROMPT].iter().chain(also) {
            assert!(!log.contains(mark), "the daemon log holds {mark:?}:\n{log}");
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        // A check left by a crashed daemon writes its pid; its group goes too.
        if let Ok(pid) = std::fs::read_to_string(self.dir.join("check.pid")) {
            let _ = Command::new("kill")
                .args(["-KILL", "--", &format!("-{}", pid.trim())])
                .output();
        }
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn read_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Fake check 1: fails once, the agent fixes it after the fail prompt, the
/// Task goes to review after 2 attempts. The check sees the workspace's env.
async fn fail_then_fix(name: &str, end: &str) {
    let mut h = Host::new(name, &[&format!("end={end}"), "fix"]);
    h.start_daemon().await;
    let before = h.server.layout();
    let check = "echo out-$((6*7))-marker; test -f fixed && test \"$LOOP_MARK\" = from-workspace";
    let started = h.start_loop(check, 3, 60).await;
    assert_eq!(
        started.r#loop.as_ref().map(|l| l.state),
        Some(LoopState::Armed)
    );
    let brief = std::fs::read_to_string(h.dir.join(".muxa/TASK.md")).expect("TASK.md");
    assert!(brief.contains("## Check") && brief.contains("test -f fixed"));
    h.user_types("Read .muxa/TASK.md and start.");
    let lp = h
        .wait_loop("a pass", |l| {
            l.state != LoopState::Armed && l.state != LoopState::Checking
        })
        .await;
    assert_eq!(lp.state, LoopState::Passed, "{lp:?}");
    assert_eq!(lp.attempts, 1);
    assert_eq!(
        lp.last_check.as_ref().map(|c| (c.attempt, c.exit)),
        Some((2, Some(0)))
    );
    let r = h.record();
    assert_eq!(r["status"], "review");
    assert_eq!(r["note"], "The check passed on attempt 2.");
    assert_eq!(h.fail_prompts(), 1);
    assert!(h.turns().contains(&"The check failed. Its output is in .muxa/checks/1.log. Fix the cause, then end your turn.".to_owned()));
    assert!(
        std::fs::read_to_string(h.dir.join(".muxa/checks/1.log"))
            .unwrap()
            .contains("out-42-marker")
    );
    let report = std::fs::read_to_string(h.dir.join(".muxa/CHECK.md")).unwrap();
    assert!(report.contains("- Attempt: 2") && report.contains("passed"));
    assert_eq!(h.server.layout(), before, "the loop changed no layout");
    h.assert_log_clean(&["out-42-marker", "6*7", "LOOP_MARK", "from-workspace"]);
    h.daemon().shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fail_then_a_fix_passes_when_turns_end_done() {
    fail_then_fix("done", "done").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fail_then_a_fix_passes_when_turns_end_idle() {
    fail_then_fix("idle", "idle").await;
}

/// Fake check 2: `false` stops at the limit; the fail prompt was typed
/// maxAttempts - 1 times.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_that_always_fails_stops_at_the_limit() {
    let mut h = Host::new("limit", &[]);
    h.start_daemon().await;
    let before = h.server.layout();
    h.start_loop("false", 3, 60).await;
    h.user_types("start");
    let lp = h
        .wait_loop("the limit", |l| l.state == LoopState::Stopped)
        .await;
    assert_eq!(lp.reason, Some(LoopReason::Limit), "{lp:?}");
    assert_eq!(lp.attempts, 3);
    assert_eq!(h.fail_prompts(), 2);
    assert_eq!(h.record()["status"], "open");
    assert!(
        h.record()["note"]
            .as_str()
            .unwrap_or("")
            .contains("3 times")
    );
    assert_eq!(h.server.layout(), before);
    h.assert_log_clean(&[]);
}

/// Fake check 3: past its timeout the check stops the loop, and its whole
/// process group is gone.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_past_its_timeout_stops_the_loop() {
    let mut h = Host::new("timeout", &[]);
    h.start_daemon().await;
    let before = h.server.layout();
    h.start_loop(
        "sleep 60 & echo $! > child.pid; echo $$ > check.pid; sleep 60",
        3,
        10,
    )
    .await;
    h.user_types("start");
    let lp = h
        .wait_loop("the timeout", |l| l.state == LoopState::Stopped)
        .await;
    assert_eq!(lp.reason, Some(LoopReason::Timeout), "{lp:?}");
    assert_eq!(lp.last_check.as_ref().and_then(|c| c.exit), None);
    for file in ["check.pid", "child.pid"] {
        let pid = std::fs::read_to_string(h.dir.join(file)).unwrap();
        h.wait(Duration::from_secs(5), "the check's group to end", |_| {
            !alive(pid.trim())
        });
    }
    assert_eq!(h.server.layout(), before);
    h.assert_log_clean(&["sleep 60"]);
}

/// Fake check 4: a waiting agent pauses the loop and nothing is typed; the
/// test answers as the user, the turn ends, the check runs, the loop goes on.
#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_agent_pauses_the_loop_until_the_user_answers() {
    let mut h = Host::new("wait", &["wait"]);
    h.start_daemon().await;
    let before = h.server.layout();
    h.start_loop("test -f fixed", 3, 60).await;
    h.user_types("start");
    let lp = h
        .wait_loop("a pause", |l| l.state == LoopState::Paused)
        .await;
    assert_eq!(lp.reason, Some(LoopReason::Waiting));
    tokio::time::sleep(Duration::from_secs(8)).await;
    assert_eq!(h.turns(), ["start"], "nothing typed while the agent waits");
    assert_eq!(h.task_loop().state, LoopState::Paused);
    std::fs::write(h.dir.join("fixed"), "").unwrap();
    h.user_types("1");
    let lp = h
        .wait_loop("a pass", |l| l.state == LoopState::Passed)
        .await;
    assert_eq!(
        (lp.attempts, lp.last_check.map(|c| c.attempt)),
        (0, Some(1))
    );
    assert_eq!(h.server.layout(), before);
    h.assert_log_clean(&[]);
}

/// Fake check 5: a daemon killed during a check, started again: the loop is
/// armed, and the attempt did not count.
#[tokio::test(flavor = "multi_thread")]
async fn a_crash_during_a_check_does_not_count_the_attempt() {
    let mut h = Host::new("crash", &[]);
    h.start_daemon().await;
    h.start_loop("echo $$ > check.pid; sleep 30; false", 3, 600)
        .await;
    h.user_types("start");
    h.wait_loop("a check", |l| l.state == LoopState::Checking)
        .await;
    h.wait(Duration::from_secs(10), "the check to start", |h| {
        h.dir.join("check.pid").is_file()
    });
    h.daemon().crash().await;
    h.daemon().wait().await;
    h.restart_daemon().await;
    let lp = h
        .wait_loop("armed again", |l| l.state == LoopState::Armed)
        .await;
    assert_eq!(lp.attempts, 0, "{lp:?}");
    assert_eq!(h.fail_prompts(), 0);
    h.daemon().shutdown().await;
}

/// Fake check 6: `daemon stop` stops every loop for the user, and a new
/// daemon does not resume them.
#[tokio::test(flavor = "multi_thread")]
async fn daemon_stop_stops_the_loops_for_good() {
    let mut h = Host::new("stop", &[]);
    h.start_daemon().await;
    h.start_loop("false", 3, 60).await;
    let status = h.daemon().shutdown().await;
    assert!(status.success());
    let lp = h.task_loop();
    assert_eq!(
        (lp.state, lp.reason),
        (LoopState::Stopped, Some(LoopReason::User))
    );
    h.restart_daemon().await;
    h.user_types("start");
    tokio::time::sleep(Duration::from_secs(10)).await;
    let lp = h.task_loop();
    assert_eq!(
        (lp.state, lp.reason),
        (LoopState::Stopped, Some(LoopReason::User))
    );
    assert!(!h.dir.join(".muxa/checks").exists(), "no check ran");
    h.daemon().shutdown().await;
}

/// Fake check 7: a daemon killed during a start-prompt wait and started
/// again within 30 s types the prompt once.
#[tokio::test(flavor = "multi_thread")]
async fn a_crash_during_a_prompt_wait_types_the_prompt_once() {
    let mut h = Host::new("prompt", &["busy"]);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    h.write_record(json!({
        "runs": [{"id": 1, "startedAt": now, "reason": "start"}],
        "pendingPrompt": {"kind": "start", "since": now, "run": 1},
    }));
    h.start_daemon().await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    h.daemon().crash().await;
    h.daemon().wait().await;
    h.restart_daemon().await;
    // The agent finishes its turn after both daemons began to wait.
    let at = (now + 3).to_string();
    h.server
        .tmux(&["set-option", "-p", "-t", &h.pane, "@agent_state_at", &at]);
    h.server
        .tmux(&["set-option", "-p", "-t", &h.pane, "@agent_state", "done"]);
    h.wait(Duration::from_secs(30), "the start prompt", |h| {
        !h.turns().is_empty()
    });
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert_eq!(h.turns(), ["Read .muxa/TASK.md and start."]);
    assert!(h.record().get("pendingPrompt").is_none());
    h.daemon().shutdown().await;
}

/// Fake check 8: a person typing on the Task's session holds the fail
/// prompt back until the client has been quiet for 5 s.
#[tokio::test(flavor = "multi_thread")]
async fn a_person_typing_holds_the_fail_prompt_back() {
    let mut h = Host::new("typing", &[]);
    h.start_daemon().await;
    h.start_loop("while [ ! -f go ]; do sleep 0.2; done; false", 2, 600)
        .await;
    h.user_types("start");
    h.wait_loop("a check", |l| l.state == LoopState::Checking)
        .await;
    // A desktop terminal of the window's size on the session; it types "x" after 1 s.
    let mut desktop = Command::new("python3")
        .args([
            "-I",
            PTY_CLIENT,
            &h.server.socket,
            &h.session,
            "80",
            "25",
            "40",
            "1",
        ])
        .spawn()
        .expect("start a desktop client");
    let desktop_activity = |h: &Host| -> Option<u64> {
        let listed = h
            .server
            .tmux(&["list-clients", "-F", "#{client_activity} #{client_flags}"]);
        listed
            .lines()
            .filter(|l| !l.contains("control-mode"))
            .find_map(|l| l.split(' ').next()?.parse().ok())
    };
    h.wait(Duration::from_secs(10), "the desktop client", |h| {
        desktop_activity(h).is_some()
    });
    let attached = desktop_activity(&h).unwrap_or(0);
    h.wait(Duration::from_secs(10), "the desktop key press", |h| {
        desktop_activity(h).is_some_and(|a| a > attached)
    });
    let before = h.server.layout();
    let typed_at = desktop_activity(&h).unwrap_or(0);
    std::fs::write(h.dir.join("go"), "").unwrap();
    h.wait(Duration::from_secs(60), "the fail prompt", |h| {
        h.fail_prompts() == 1
    });
    let prompt_at: u64 = read_lines(&h.dir.join("times.log"))
        .iter()
        .find(|l| l.contains(FAIL_PROMPT))
        .and_then(|l| l.split(' ').next()?.parse().ok())
        .expect("the prompt's time");
    assert!(
        prompt_at >= typed_at + 5,
        "typed at {prompt_at}, the person typed at {typed_at}"
    );
    assert_eq!(h.server.layout(), before, "the loop changed no layout");
    let _ = desktop.kill();
    let _ = desktop.wait();
    h.daemon().shutdown().await;
}

/// A bridge offers the loop only with a daemon of its own version, and
/// forwards loop requests to it. Without one, they are `unsupported`.
#[tokio::test(flavor = "multi_thread")]
async fn a_bridge_forwards_loops_to_the_daemon_and_refuses_them_without_one() {
    let mut h = Host::new("bridge", &[]);
    let mut flags = h.flags();
    let none = common::daemon::short_dir();
    flags.extend(["--daemon-dir".into(), none.display().to_string()]);
    let args: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (mut bridge, hello) = Bridge::start(&h.server, &args).await;
    assert!(!capabilities(&hello).contains(&"task-loop".to_owned()));
    let task = h.task.clone();
    let refused = bridge
        .request(json!({"type": "start-loop", "task": task, "loop": {"check": "true"}}))
        .await;
    match refused {
        Message::Result { error: Some(e), .. } => {
            assert_eq!(e.code, ErrorCode::Unsupported);
            assert!(
                e.message.contains("muxa-bridge daemon install"),
                "{}",
                e.message
            );
        }
        other => panic!("{other:?}"),
    }
    drop(bridge);

    h.start_daemon().await;
    let mut flags = h.flags();
    flags.extend(["--daemon-dir".into(), h.daemon().dir.display().to_string()]);
    let args: Vec<&str> = flags.iter().map(String::as_str).collect();
    let (mut bridge, hello) = Bridge::start(&h.server, &args).await;
    assert!(capabilities(&hello).contains(&"task-loop".to_owned()));
    let record = saved(
        &mut bridge,
        json!({"type": "start-loop", "task": task, "loop": {"check": "true"}}),
    )
    .await;
    let lp = record.r#loop.expect("a loop");
    assert_eq!(
        (lp.state, lp.max_attempts, lp.timeout),
        (LoopState::Armed, 3, 600)
    );
    let result = bridge.request(json!({"type": "list-tasks"})).await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let workspaces = loop {
        match bridge.event(Duration::from_secs(5)).await {
            Some(Message::TaskList { workspaces, .. }) => break workspaces,
            Some(_) => continue,
            None => panic!("no task-list"),
        }
    };
    assert_eq!(
        workspaces[0].checks,
        Some(muxa_protocol::WorkspaceChecks::Any)
    );
    let record = saved(&mut bridge, json!({"type": "stop-loop", "task": task})).await;
    let lp = record.r#loop.expect("a loop");
    assert_eq!(
        (lp.state, lp.reason),
        (LoopState::Stopped, Some(LoopReason::User))
    );
    assert_eq!(record.status, TaskStatus::Open);
    h.daemon().shutdown().await;
}

/// Sends `req` and returns the record of its `task-saved`.
async fn saved(bridge: &mut Bridge, req: Value) -> TaskRecord {
    let result = bridge.request(req).await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    loop {
        match bridge.event(Duration::from_secs(5)).await {
            Some(Message::TaskSaved { record, .. }) => return record,
            Some(_) => continue,
            None => panic!("no task-saved"),
        }
    }
}

fn capabilities(hello: &Message) -> Vec<String> {
    match hello {
        Message::Hello { capabilities, .. } => capabilities.clone(),
        other => panic!("{other:?}"),
    }
}
