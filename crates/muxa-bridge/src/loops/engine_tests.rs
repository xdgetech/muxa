//! The engine against a real, throwaway tmux server and the fake agent of
//! `tests/data/loop-agent.py`, without the daemon around it.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use super::*;
use crate::records::{NewRecord, Status};

const AGENT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/loop-agent.py");

struct Tmux(String);

impl Tmux {
    fn start(name: &str) -> Self {
        let t = Tmux(format!("muxa-test-loops-{}-{name}", std::process::id()));
        t.run(&["-f", "/dev/null", "new-session", "-d", "-s", "spare", "sh"]);
        t
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .arg("-L")
            .arg(&self.0)
            .args(args)
            .env_remove("TMUX")
            .output()
            .expect("run tmux");
        assert!(
            out.status.success(),
            "tmux {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
    }
}

impl Drop for Tmux {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-L")
            .arg(&self.0)
            .arg("kill-server")
            .output();
    }
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-engine-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

struct Host {
    tmux: Tmux,
    config: Arc<Config>,
    dir: PathBuf,
    task: String,
    pane: String,
}

/// A Task with a loop on `check`, its session running the fake agent.
fn host(name: &str, check: &str, max_attempts: u32, timeout: u64) -> Host {
    let base = temp(name);
    let root = base.join("root");
    let dir = root.join("tasks/t");
    std::fs::create_dir_all(&dir).unwrap();
    let workspaces = base.join("workspaces.toml");
    std::fs::write(
        &workspaces,
        format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.loop]\nchecks = \"any\"\n",
            root.display()
        ),
    )
    .unwrap();
    let config = Arc::new(Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(workspaces),
        home: None,
    });
    let store = Store::open(base.join("state")).unwrap();
    let record = store
        .create(NewRecord {
            workspace: "w".into(),
            title: "T".into(),
            dir: Some(dir.clone()),
            ..NewRecord::default()
        })
        .unwrap();
    let tmux = Tmux::start(name);
    let pane = tmux.run(&[
        "new-session",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-s",
        "task",
        "-c",
        &dir.display().to_string(),
        "python3",
        AGENT,
    ]);
    tmux.run(&["set-option", "-t", "task", "@muxa_task", &record.id]);
    let h = Host {
        tmux,
        config,
        dir,
        task: record.id,
        pane,
    };
    h.wait_for(|| h.option("@agent_state").is_some(), "the agent to start");
    let spec = LoopSpec {
        check: check.into(),
        max_attempts: Some(max_attempts),
        timeout: Some(timeout),
    };
    start(&h.config, &h.task, spec, false).unwrap();
    h
}

impl Host {
    fn option(&self, name: &str) -> Option<String> {
        let v = self
            .tmux
            .run(&["show-options", "-pqv", "-t", &self.pane, name]);
        (!v.is_empty()).then_some(v)
    }

    fn wait_for(&self, cond: impl Fn() -> bool, what: &str) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !cond() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn record(&self) -> Record {
        Store::open(self.config.state_dir.clone().unwrap())
            .unwrap()
            .get(&self.task)
            .unwrap()
    }

    fn turns(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("turns.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn user_types(&self, text: &str) {
        self.tmux.run(&["send-keys", "-t", &self.pane, "-l", text]);
        self.tmux.run(&["send-keys", "-t", &self.pane, "Enter"]);
    }

    fn client(&self) -> (Client, muxa_tmux::Notifications) {
        let opts = muxa_tmux::Options {
            tmux: "tmux".into(),
            socket: Some(self.tmux.0.clone()),
        };
        Client::attach(&opts, None).unwrap()
    }
}

/// Ticks until the loop leaves the active states, or `limit` passes.
async fn run_until_ended(h: &Host, limit: Duration) -> Loop {
    let (client, mut notes) = h.client();
    tokio::spawn(async move { while notes.recv().await.is_some() {} });
    let mut engine = Engine::new(Arc::clone(&h.config), Arc::new(Registry::new(vec![])), None);
    let deadline = Instant::now() + limit;
    loop {
        engine.tick(Some(&client)).await;
        let lp = h.record().task_loop().unwrap().unwrap();
        if !lp.active() || Instant::now() > deadline {
            engine.stop_checks().await;
            return lp;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

fn layout(h: &Host) -> String {
    h.tmux.run(&[
        "list-windows",
        "-a",
        "-F",
        "#{window_id} #{window_layout} #{window_active}",
    ])
}

async fn fixes_itself(name: &str, end: &str) {
    let h = host(name, "test -f fixed", 3, 60);
    std::fs::write(h.dir.join("end"), end).unwrap();
    std::fs::write(h.dir.join("fix"), "").unwrap();
    let before = layout(&h);
    h.user_types("Read .muxa/TASK.md and start.");
    let lp = run_until_ended(&h, Duration::from_secs(90)).await;
    assert_eq!(lp.state, LoopState::Passed, "{lp:?}");
    assert_eq!(lp.attempts, 1);
    assert_eq!(
        lp.last_check.as_ref().map(|c| (c.attempt, c.exit)),
        Some((2, Some(0)))
    );
    let r = h.record();
    assert_eq!(r.status, Status::Review);
    assert_eq!(r.note.as_deref(), Some("The check passed on attempt 2."));
    let turns = h.turns();
    assert_eq!(turns.len(), 2, "{turns:?}");
    assert_eq!(turns[1], typing::fail_prompt(1));
    assert!(h.dir.join(".muxa/checks/1.log").is_file());
    let report = std::fs::read_to_string(h.dir.join(".muxa/CHECK.md")).unwrap();
    assert!(report.contains("- Attempt: 2") && report.contains("passed"));
    let brief = std::fs::read_to_string(h.dir.join(".muxa/TASK.md")).unwrap();
    assert!(brief.contains("## Check") && brief.contains("test -f fixed"));
    assert_eq!(layout(&h), before);
    assert_eq!(h.option("@muxa_role").as_deref(), Some("agent"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fail_then_a_fix_passes_with_done() {
    fixes_itself("done", "done").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_fail_then_a_fix_passes_with_idle() {
    fixes_itself("idle", "idle").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_that_always_fails_stops_at_the_limit() {
    let h = host("limit", "false", 3, 60);
    h.user_types("start");
    let lp = run_until_ended(&h, Duration::from_secs(120)).await;
    assert_eq!(
        (lp.state.clone(), lp.reason.clone()),
        (LoopState::Stopped, Some(Reason::Limit)),
        "{lp:?}"
    );
    assert_eq!(lp.attempts, 3);
    let prompts = h
        .turns()
        .iter()
        .filter(|t| t.starts_with("The check failed"))
        .count();
    assert_eq!(prompts, 2, "maxAttempts - 1 fail prompts");
    assert_eq!(h.record().status, Status::Open);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_waiting_agent_pauses_the_loop_and_nothing_is_typed() {
    let h = host("wait", "test -f fixed", 3, 60);
    std::fs::write(h.dir.join("wait"), "").unwrap();
    h.user_types("start");
    let (client, mut notes) = h.client();
    tokio::spawn(async move { while notes.recv().await.is_some() {} });
    let mut engine = Engine::new(Arc::clone(&h.config), Arc::new(Registry::new(vec![])), None);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        engine.tick(Some(&client)).await;
        let lp = h.record().task_loop().unwrap().unwrap();
        if lp.state == LoopState::Paused {
            assert_eq!(lp.reason, Some(Reason::Waiting));
            break;
        }
        assert!(Instant::now() < deadline, "never paused: {lp:?}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    for _ in 0..12 {
        engine.tick(Some(&client)).await;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(h.turns(), ["start"], "nothing typed while waiting");
    // The user answers; the turn ends, the check runs and fails, the loop goes on.
    std::fs::write(h.dir.join("fixed"), "").unwrap();
    h.user_types("1");
    drop(engine);
    let lp = run_until_ended(&h, Duration::from_secs(60)).await;
    assert_eq!(lp.state, LoopState::Passed, "{lp:?}");
    assert_eq!(lp.attempts, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_past_its_timeout_stops_and_leaves_nothing_running() {
    let h = host("timeout", "echo $$ > check.pid; sleep 60", 3, 10);
    h.user_types("start");
    let lp = run_until_ended(&h, Duration::from_secs(60)).await;
    assert_eq!(lp.reason, Some(Reason::Timeout), "{lp:?}");
    assert_eq!(lp.last_check.as_ref().and_then(|c| c.exit), None);
    let pid = std::fs::read_to_string(h.dir.join("check.pid")).unwrap();
    let alive = Command::new("kill")
        .args(["-0", pid.trim()])
        .output()
        .unwrap()
        .status
        .success();
    assert!(!alive, "the check's shell is gone");
}
