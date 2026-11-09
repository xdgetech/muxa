//! Runtime worker acceptance on isolated tmux servers with synthetic agents.

use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// The bound on waiting for a fixture agent. Generous: a loaded machine
/// starts processes slowly, and a met condition returns at once.
const WAIT: Duration = Duration::from_secs(30);

struct Fixture {
    dir: PathBuf,
    options: muxa_tmux::Options,
}

impl Fixture {
    fn new() -> Self {
        let id = getrandom::u32().unwrap();
        let dir = std::env::temp_dir().join(format!("muxa-worker-create-{id:08x}"));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let path = Command::new("which").arg("tmux").output().unwrap();
        assert!(path.status.success());
        let options = muxa_tmux::Options {
            tmux: PathBuf::from(String::from_utf8(path.stdout).unwrap().trim()),
            socket: Some(format!("muxa-test-worker-create-{id:08x}")),
        };
        let fixture = Self { dir, options };
        // `env` is written last and renamed into place: the tests wait for
        // it, then read `argv` and `env` whole.
        // The server itself cannot resolve a real native agent if PATH handling regresses.
        let bin = fixture.dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(bin.join("claude"), "#!/bin/sh\nprintf '%s\\n' \"$$\" >> starts\nprintf '%s\\000' \"$@\" > argv\nprintf '%s' \"$MUXA_FIXTURE_ENV\" > env.tmp\nmv env.tmp env\nexec /bin/sleep 600\n").unwrap();
        std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        let output = fixture
            .command()
            .args([
                "-f",
                "/dev/null",
                "new-session",
                "-d",
                "-s",
                "fixture",
                "/bin/sleep 600",
            ])
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fixture
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.options.tmux);
        command
            .args(["-L", self.options.socket.as_deref().unwrap()])
            .env_remove("TMUX");
        command
    }

    async fn started(&self) -> (Env, Record, muxa_tmux::PaneId) {
        let env = self.env().await;
        std::fs::write(self.dir.join("instructions.md"), "Synthetic instructions.").unwrap();
        let (record, error) = create(
            &env,
            NewTask {
                workspace: "work".into(),
                worker: Some("Reviewer".into()),
                title: "Worker test".into(),
                goal: "Synthetic acceptance fixture".into(),
                ..Default::default()
            },
        )
        .await;
        assert!(error.is_none(), "{error:?}");
        let record = record.unwrap();
        env.pending.finish(Duration::ZERO).await;
        let pane = muxa_tmux::list_sessions(&env.client)
            .await
            .unwrap()
            .into_iter()
            .find(|s| s.task_id.as_deref() == Some(record.id.as_str()))
            .unwrap()
            .panes[0]
            .id;
        let dir = record.dir.as_ref().unwrap();
        tokio::time::timeout(WAIT, async {
            while !dir.join("env").exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        (env, record, pane)
    }

    fn process(&self, pane: muxa_tmux::PaneId) -> Vec<u8> {
        let out = self.command().args(["display-message", "-p", "-t", &pane.to_string(),
                "#{session_id} #{window_id} #{pane_id} #{pane_pid} #{pane_width}x#{pane_height} #{window_layout}"]).output().unwrap();
        assert!(out.status.success());
        out.stdout
    }

    async fn second_client(&self, env: &Env) -> Env {
        let (client, _notes) = Client::attach(&self.options, None).unwrap();
        Env {
            client: Arc::new(client),
            pending: Pending::default(),
            ..env.clone()
        }
    }

    async fn intercepted_client(&self, env: &Env, record: &Record, refuse_respawn: bool) -> Env {
        let proxy = self.dir.join("prepared-proxy.py");
        let record_path = self.dir.join("records").join(format!("{}.json", record.id));
        let code = format!(
            r#"#!/usr/bin/env python3
import json, os, subprocess, sys, threading, time
args = sys.argv[1:]
if '-L' not in args or not args[args.index('-L') + 1].startswith('muxa-test-'):
    sys.exit(2)
real = {real}
if '-C' not in args:
    os.execv(real, [real] + args)
child = subprocess.Popen([real] + args, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def feed():
    held = False
    for line in sys.stdin.buffer:
        if {refuse} and line.startswith(b'respawn-pane '):
            with open({refused}, 'ab') as marker:
                marker.write(b'refused\n')
            # A real tmux error reply proves no replacement happened in this fixture.
            line = b'display-message -p -t %4294967295\n'
        if not {refuse} and not held and line.startswith(b'list-panes '):
            with open({record}) as source:
                phase = json.load(source).get('restartOperation', {{}}).get('phase')
            if phase == 'prepared':
                held = True
                with open({blocked}, 'wb') as marker:
                    marker.write(b'held')
                deadline = time.monotonic() + 60
                while not os.path.exists({release}):
                    if time.monotonic() > deadline:
                        child.terminate()
                        return
                    time.sleep(0.01)
        try:
            child.stdin.write(line)
            child.stdin.flush()
        except BrokenPipeError:
            return
    child.terminate()
threading.Thread(target=feed, daemon=True).start()
while True:
    chunk = os.read(child.stdout.fileno(), 65536)
    if not chunk:
        break
    sys.stdout.buffer.write(chunk)
    sys.stdout.buffer.flush()
sys.exit(child.wait())
"#,
            real = serde_json::to_string(&self.options.tmux).unwrap(),
            refuse = if refuse_respawn { "True" } else { "False" },
            refused = serde_json::to_string(&self.dir.join("respawn-refused")).unwrap(),
            record = serde_json::to_string(&record_path).unwrap(),
            blocked = serde_json::to_string(&self.dir.join("prepared-blocked")).unwrap(),
            release = serde_json::to_string(&self.dir.join("prepared-release")).unwrap(),
        );
        std::fs::write(&proxy, code).unwrap();
        std::fs::set_permissions(&proxy, std::fs::Permissions::from_mode(0o700)).unwrap();
        let options = muxa_tmux::Options {
            tmux: proxy,
            ..self.options.clone()
        };
        let (client, _notes) = Client::attach(&options, None).unwrap();
        Env {
            client: Arc::new(client),
            pending: Pending::default(),
            ..env.clone()
        }
    }

    async fn env(&self) -> Env {
        let config = self.dir.join("workspaces.toml");
        std::fs::write(&config, format!("[[workspace]]\nname = 'work'\nroot = '{}'\n[workspace.agent.env]\nMUXA_FIXTURE_ENV = 'synthetic workspace env'\n[[worker]]\nname = 'Reviewer'\nagent = 'claude-code'\nargs = ['--agent', 'reviewer']\ninstructions = '{}'\n", self.dir.display(), self.dir.join("instructions.md").display())).unwrap();
        let (client, _notes) = Client::attach(&self.options, None).unwrap();
        Env {
            client: Arc::new(client),
            tasks: Arc::new(tasks::Config {
                launcher: Some(tasks::Choice::Builtin),
                command: "/bin/sleep 600".into(),
                tmux: self.options.tmux.clone(),
                socket: self.options.socket.clone(),
            }),
            records: Arc::new(Config {
                state_dir: Some(self.dir.join("records")),
                workspaces: Some(config),
                home: None,
            }),
            agents: Arc::new(Registry::new(vec![])),
            pending: Pending::default(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.command().arg("kill-server").output();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn worker_create_validates_before_writes_and_records_the_requested_assignment() {
    let fixture = Fixture::new();
    let env = fixture.env().await;
    let request = NewTask {
        workspace: "work".into(),
        worker: Some("Reviewer".into()),
        title: "Review".into(),
        goal: "Synthetic fixture".into(),
        ..Default::default()
    };
    for invalid in [None, Some(vec![0xff]), Some(vec![b'x'; 16 * 1024 + 1])] {
        let path = fixture.dir.join("instructions.md");
        if let Some(bytes) = invalid {
            std::fs::write(path, bytes).unwrap();
        }
        let (record, error) = create(&env, request.clone()).await;
        assert!(record.is_none());
        assert!(error.is_some());
        assert!(!fixture.dir.join("records").exists());
        assert!(!fixture.dir.join("tasks").exists());
    }
    std::fs::write(
        fixture.dir.join("instructions.md"),
        "Synthetic worker instructions.",
    )
    .unwrap();
    let (record, error) = create(&env, request).await;
    assert!(error.is_none(), "{error:?}");
    let record = record.unwrap();
    assert_eq!(record.worker(), Some("Reviewer"));
    assert_eq!(record.runs[0].worker().as_deref(), Some("Reviewer"));
    let dir = record.dir.as_ref().unwrap();
    tokio::time::timeout(WAIT, async {
        while !dir.join("env").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(dir.join("argv")).unwrap(),
        b"--agent\0reviewer\0"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("env")).unwrap(),
        "synthetic workspace env"
    );
    assert!(
        std::fs::read_to_string(dir.join(".muxa/TASK.md"))
            .unwrap()
            .contains("## Worker\n\nReviewer\n\nSynthetic worker instructions.")
    );
    env.pending.finish(Duration::ZERO).await;
}
#[tokio::test]
async fn named_restart_refreshes_command_then_clears_assignment_in_the_same_pane() {
    use super::super::restart::restart_record;
    let fixture = Fixture::new();
    let env = fixture.env().await;
    let instruction_file = fixture.dir.join("instructions.md");
    std::fs::write(&instruction_file, "First worker instructions.").unwrap();
    let (record, error) = create(
        &env,
        NewTask {
            workspace: "work".into(),
            worker: Some("Reviewer".into()),
            title: "Handoff".into(),
            goal: "Synthetic restart fixture".into(),
            ..Default::default()
        },
    )
    .await;
    assert!(error.is_none(), "{error:?}");
    let record = record.unwrap();
    env.pending.finish(Duration::ZERO).await;
    let dir = record.dir.as_ref().unwrap().clone();
    let session = muxa_tmux::list_sessions(&env.client)
        .await
        .unwrap()
        .into_iter()
        .find(|session| session.task_id.as_deref() == Some(record.id.as_str()))
        .unwrap();
    let pane = session.panes[0].id;
    let shape = || {
        let out = fixture.command().args([
                "display-message", "-p", "-t", &pane.to_string(),
                "#{session_id} #{window_id} #{pane_id} #{pane_width}x#{pane_height} #{window_layout}",
            ]).output().unwrap();
        assert!(out.status.success());
        out.stdout
    };
    let pid = || {
        let out = fixture
            .command()
            .args([
                "display-message",
                "-p",
                "-t",
                &pane.to_string(),
                "#{pane_pid}",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        out.stdout
    };
    let before = shape();
    let first_pid = pid();
    let task_file = dir.join(".muxa/TASK.md");
    let notes = "## Progress\n\n私の notes must stay byte exact.\n";
    let mut text = std::fs::read_to_string(&task_file).unwrap();
    let at = text.find("## Progress\n").unwrap();
    text.truncate(at);
    text.push_str(notes);
    std::fs::write(&task_file, &text).unwrap();
    let config = fixture.dir.join("workspaces.toml");
    let edited = std::fs::read_to_string(&config)
        .unwrap()
        .replace("'reviewer'", "'reviewer-edited'");
    std::fs::write(&config, edited).unwrap();
    std::fs::write(&instruction_file, "Edited worker instructions.").unwrap();
    // Wait for the first fake process before removing its completion markers.
    tokio::time::timeout(WAIT, async {
        while !dir.join("env").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    std::fs::remove_file(dir.join("env")).unwrap();
    std::fs::remove_file(dir.join("argv")).unwrap();
    let old_rev = record.rev;
    let restarted = restart_record(&env, record, true, Some("Reviewer".into()), Some(old_rev))
        .await
        .unwrap();
    env.pending.finish(Duration::ZERO).await;
    tokio::time::timeout(WAIT, async {
        while !dir.join("env").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(dir.join("argv")).unwrap(),
        b"--agent\0reviewer-edited\0"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("env")).unwrap(),
        "synthetic workspace env"
    );
    assert_eq!(shape(), before);
    assert_ne!(pid(), first_pid);
    assert_eq!(restarted.worker(), Some("Reviewer"));
    assert_eq!(restarted.runs.len(), 2);
    assert_eq!(restarted.runs[1].worker().as_deref(), Some("Reviewer"));
    let text = std::fs::read_to_string(&task_file).unwrap();
    assert!(text.contains("Edited worker instructions."));
    assert!(!text.contains("First worker instructions."));
    assert!(text.ends_with(notes));
    let handoff_file = dir.join(".muxa/HANDOFF.md");
    let handoff = std::fs::read(&handoff_file).unwrap();
    let current_pid = pid();
    let stale = restart_record(
        &env,
        restarted.clone(),
        true,
        Some(muxa_protocol::DEFAULT_WORKER.into()),
        Some(old_rev),
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);
    assert_eq!(pid(), current_pid);
    assert_eq!(shape(), before);
    assert_eq!(std::fs::read_to_string(&task_file).unwrap(), text);
    assert_eq!(std::fs::read(&handoff_file).unwrap(), handoff);
    let stored = env.records.store().unwrap().get(&restarted.id).unwrap();
    assert_eq!(stored.rev, restarted.rev);
    assert_eq!(stored.runs.len(), 2);
    let rev = restarted.rev;
    let cleared = restart_record(
        &env,
        restarted,
        true,
        Some(muxa_protocol::DEFAULT_WORKER.into()),
        Some(rev),
    )
    .await
    .unwrap();
    env.pending.finish(Duration::ZERO).await;
    assert_eq!(shape(), before);
    assert_ne!(pid(), current_pid);
    assert_eq!(cleared.worker(), None);
    assert_eq!(cleared.runs.len(), 3);
    assert_eq!(cleared.runs[2].worker().as_deref(), Some("default"));
    let text = std::fs::read_to_string(&task_file).unwrap();
    assert!(!text.contains("## Worker"));
    assert!(!text.contains("Edited worker instructions."));
    assert!(text.ends_with(notes));
    assert_eq!(cleared.id, stored.id);
    assert_eq!(cleared.session, stored.session);
}
#[tokio::test]
async fn concurrent_named_restarts_use_one_claim_and_a_stale_loser_cannot_retry() {
    use super::super::restart::restart_record;
    use crate::records::Store;
    let fixture = Fixture::new();
    let (env, record, pane) = fixture.started().await;
    let other = fixture.second_client(&env).await;
    let dir = record.dir.as_ref().unwrap();
    let brief = std::fs::read(dir.join(".muxa/TASK.md")).unwrap();
    let before = fixture.process(pane);
    let store = Store::open(fixture.dir.join("records")).unwrap();
    let held = store.claim(&record.id, record.rev).unwrap();
    let blocked = restart_record(
        &other,
        record.clone(),
        true,
        Some("Reviewer".into()),
        Some(record.rev),
    )
    .await
    .unwrap_err();
    assert_eq!(blocked.code, ErrorCode::Conflict);
    assert_eq!(fixture.process(pane), before);
    assert_eq!(std::fs::read(dir.join(".muxa/TASK.md")).unwrap(), brief);
    assert_eq!(store.get(&record.id).unwrap(), record);
    assert!(!dir.join(".muxa/HANDOFF.md").exists());
    drop(held);
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let first = {
        let barrier = Arc::clone(&barrier);
        let env = env.clone();
        let record = record.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            restart_record(
                &env,
                record.clone(),
                true,
                Some("Reviewer".into()),
                Some(record.rev),
            )
            .await
        })
    };
    let second = {
        let barrier = Arc::clone(&barrier);
        let other = other.clone();
        let record = record.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            restart_record(
                &other,
                record.clone(),
                true,
                Some("Reviewer".into()),
                Some(record.rev),
            )
            .await
        })
    };
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    env.pending.finish(Duration::ZERO).await;
    other.pending.finish(Duration::ZERO).await;
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    let (winner, loser) = match (first, second) {
        (Ok(winner), Err(loser)) | (Err(loser), Ok(winner)) => (winner, loser),
        _ => panic!("exactly one restart must succeed"),
    };
    assert_eq!(loser.code, ErrorCode::Conflict);
    assert_eq!(winner.runs.len(), 2);
    assert_ne!(fixture.process(pane), before);
    let current = fixture.process(pane);
    let files = (
        std::fs::read(dir.join(".muxa/TASK.md")).unwrap(),
        std::fs::read(dir.join(".muxa/HANDOFF.md")).unwrap(),
    );
    let stale = restart_record(&other, record.clone(), true, None, Some(record.rev))
        .await
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);
    assert_eq!(fixture.process(pane), current);
    assert_eq!(store.get(&record.id).unwrap(), winner);
    assert_eq!(std::fs::read(dir.join(".muxa/TASK.md")).unwrap(), files.0);
    assert_eq!(
        std::fs::read(dir.join(".muxa/HANDOFF.md")).unwrap(),
        files.1
    );
    tokio::time::timeout(WAIT, async {
        while std::fs::read_to_string(dir.join("starts"))
            .unwrap()
            .lines()
            .count()
            < 2
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("starts"))
            .unwrap()
            .lines()
            .count(),
        2
    );
}

#[tokio::test]
async fn named_restart_rejects_changed_source_before_dispatch() {
    use super::super::restart::restart_record;
    let fixture = Fixture::new();
    let (env, record, pane) = fixture.started().await;
    let proxied = fixture.intercepted_client(&env, &record, false).await;
    let dir = record.dir.as_ref().unwrap();
    let brief = std::fs::read(dir.join(".muxa/TASK.md")).unwrap();
    let attempt = {
        let proxied = proxied.clone();
        let record = record.clone();
        tokio::spawn(async move {
            restart_record(
                &proxied,
                record.clone(),
                true,
                Some("Reviewer".into()),
                Some(record.rev),
            )
            .await
        })
    };
    tokio::time::timeout(WAIT, async {
        while !fixture.dir.join("prepared-blocked").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let during = env.records.store().unwrap().get(&record.id).unwrap();
    assert_eq!(during.extra["restartOperation"]["phase"], "prepared");
    muxa_tmux::respawn_pane_command(
        &env.client,
        pane,
        &dir.to_string_lossy(),
        &[],
        &["/bin/sleep".into(), "600".into()],
    )
    .await
    .unwrap();
    let replacement = fixture.process(pane);
    std::fs::write(fixture.dir.join("prepared-release"), b"release").unwrap();
    let error = attempt.await.unwrap().unwrap_err();
    assert_eq!(error.code, ErrorCode::Conflict);
    assert_eq!(fixture.process(pane), replacement);
    assert_eq!(std::fs::read(dir.join(".muxa/TASK.md")).unwrap(), brief);
    assert!(!dir.join(".muxa/HANDOFF.md").exists());
    let cancelled = env.records.store().unwrap().get(&record.id).unwrap();
    assert!(!cancelled.extra.contains_key("restartOperation"));
    assert_eq!(cancelled.runs, record.runs);
    assert_eq!(cancelled.worker(), record.worker());
    assert_eq!(
        std::fs::read_to_string(dir.join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    proxied.pending.finish(Duration::ZERO).await;
}

#[tokio::test]
async fn named_restart_instruction_errors_leave_a_running_task_untouched() {
    use super::super::restart::restart_record;
    let fixture = Fixture::new();
    let (env, record, pane) = fixture.started().await;
    let dir = record.dir.as_ref().unwrap();
    let before = fixture.process(pane);
    let brief = std::fs::read(dir.join(".muxa/TASK.md")).unwrap();
    let instructions = fixture.dir.join("instructions.md");
    for invalid in [
        None,
        Some(vec![0xff]),
        Some(vec![b'x'; 16 * 1024 + 1]),
        Some(b"bad\0text".to_vec()),
    ] {
        if let Some(bytes) = invalid {
            std::fs::write(&instructions, bytes).unwrap();
        } else {
            std::fs::remove_file(&instructions).unwrap();
        }
        let error = restart_record(
            &env,
            record.clone(),
            true,
            Some("Reviewer".into()),
            Some(record.rev),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::BadRequest);
        assert_eq!(fixture.process(pane), before);
        assert_eq!(std::fs::read(dir.join(".muxa/TASK.md")).unwrap(), brief);
        assert!(!dir.join(".muxa/HANDOFF.md").exists());
        assert!(!dir.join(".muxa/restarts").exists());
        assert_eq!(
            env.records.store().unwrap().get(&record.id).unwrap(),
            record
        );
    }
}
#[tokio::test]
async fn a_refused_named_respawn_keeps_dispatch_uncertain_and_never_retries() {
    use super::super::restart::{restart_record, summary};
    let fixture = Fixture::new();
    let (env, record, pane) = fixture.started().await;
    let proxied = fixture.intercepted_client(&env, &record, true).await;
    let dir = record.dir.as_ref().unwrap();
    let before = fixture.process(pane);
    let brief = std::fs::read(dir.join(".muxa/TASK.md")).unwrap();
    let error = restart_record(
        &proxied,
        record.clone(),
        true,
        Some("Reviewer".into()),
        Some(record.rev),
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::TaskFailed);
    assert!(error.message.contains("uncertain"));
    assert_eq!(fixture.process(pane), before);
    assert_eq!(std::fs::read(dir.join(".muxa/TASK.md")).unwrap(), brief);
    assert!(!dir.join(".muxa/HANDOFF.md").exists());
    let store = env.records.store().unwrap();
    let uncertain = store.get(&record.id).unwrap();
    assert_eq!(uncertain.runs, record.runs);
    assert_eq!(uncertain.worker(), record.worker());
    let operation = summary(&uncertain).unwrap();
    assert_eq!(operation.phase, "dispatching");
    assert_eq!(operation.actions, vec!["leave-unconfirmed"]);
    let retry = restart_record(&env, uncertain.clone(), true, None, Some(uncertain.rev))
        .await
        .unwrap_err();
    assert_eq!(retry.code, ErrorCode::Conflict);
    assert_eq!(store.get(&record.id).unwrap(), uncertain);
    assert_eq!(fixture.process(pane), before);
    assert_eq!(
        std::fs::read_to_string(fixture.dir.join("respawn-refused")).unwrap(),
        "refused\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("starts"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    proxied.pending.finish(Duration::ZERO).await;
}

#[tokio::test]
async fn named_start_waits_for_process_bound_readiness_and_delivers_once() {
    let fixture = Fixture::new();
    let mut env = fixture.env().await;
    let python = Command::new("which").arg("python3").output().unwrap();
    assert!(python.status.success());
    let python = String::from_utf8(python.stdout).unwrap();
    std::fs::write(fixture.dir.join("bin/claude"), format!("#!{}\nimport os, pathlib, sys\npathlib.Path('ready.tmp').write_text(str(os.getpid()))\nos.replace('ready.tmp', 'ready')\nfor line in sys.stdin:\n    with open('input', 'a') as output:\n        output.write(line)\n", python.trim())).unwrap();
    let native = fixture.dir.join("native");
    std::fs::create_dir_all(native.join("sessions")).unwrap();
    env.agents = Arc::new(Registry::new(vec![Box::new(
        crate::agents::claude::ClaudeAdapter::new(vec![native.clone()]),
    )]));
    std::fs::write(
        fixture.dir.join("instructions.md"),
        "Synthetic startup instructions.",
    )
    .unwrap();
    let (record, error) = create(
        &env,
        NewTask {
            workspace: "work".into(),
            worker: Some("Reviewer".into()),
            title: "Startup proof".into(),
            goal: "Synthetic prompt fixture".into(),
            ..Default::default()
        },
    )
    .await;
    assert!(error.is_none(), "{error:?}");
    let record = record.unwrap();
    let baseline = record
        .open_run()
        .unwrap()
        .restart_observation()
        .unwrap()
        .unwrap();
    assert!(baseline.operation.starts_with("start-"));
    assert!(baseline.source.is_none());
    let dir = record.dir.as_ref().unwrap();
    tokio::time::timeout(WAIT, async {
        while !dir.join("ready").exists() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("ready")).unwrap(),
        baseline.replacement.pid.to_string()
    );
    let pane = muxa_tmux::PaneId::new(baseline.pane);
    muxa_tmux::set_pane_option(&env.client, pane, "@agent_state", "idle")
        .await
        .unwrap();
    muxa_tmux::set_pane_option(
        &env.client,
        pane,
        "@agent_state_at",
        &crate::records::now().to_string(),
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        !dir.join("input").exists(),
        "tags alone cannot authorize the first input"
    );
    let session = record.session.as_ref().unwrap();
    let session_id = "0b8c3c0e-1111-4a2b-9c3d-000000000005";
    let mut state = serde_json::json!({
        "kind":"interactive", "pid":baseline.replacement.pid,"sessionId":session_id,
        "tmux":format!("{session}:@0.{pane}"),"cwd":dir,"status":"waiting"
    });
    let state_file = native.join("sessions/startup.json");
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    tokio::time::sleep(Duration::from_millis(1200)).await;
    assert!(
        !dir.join("input").exists(),
        "a real native permission wait cannot receive the prompt"
    );
    state["status"] = "idle".into();
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    tokio::time::timeout(WAIT, async {
        loop {
            let current = env.records.store().unwrap().get(&record.id).unwrap();
            if current.pending_prompt().is_none()
                && !current.extra.contains_key("restartOperation")
                && std::fs::read_to_string(dir.join("input")).is_ok_and(|s| s.ends_with('\n'))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("input")).unwrap(),
        "Read .muxa/TASK.md and start.\n"
    );
    env.pending.finish(Duration::from_secs(1)).await;
    let saved = env.records.store().unwrap().get(&record.id).unwrap();
    assert_eq!(saved.runs.len(), 1);
    assert_eq!(saved.runs[0].agent.as_deref(), Some("claude-code"));
    assert_eq!(saved.runs[0].agent_session.as_deref(), Some(session_id));
    assert_eq!(saved.worker(), Some("Reviewer"));
    assert!(
        saved
            .open_run()
            .unwrap()
            .restart_observation()
            .unwrap()
            .is_some()
    );
    let mut current = saved;
    let mut expected = "Read .muxa/TASK.md and start.\n".to_owned();
    for (resume, next_session) in [
        (false, "0b8c3c0e-1111-4a2b-9c3d-000000000006"),
        (true, "0b8c3c0e-1111-4a2b-9c3d-000000000007"),
    ] {
        let rev = current.rev;
        current = super::super::restart::restart_record(&env, current, true, None, Some(rev))
            .await
            .unwrap();
        let replacement = current
            .open_run()
            .unwrap()
            .restart_observation()
            .unwrap()
            .unwrap();
        if resume {
            env.pending.finish(Duration::ZERO).await;
            assert!(
                env.records
                    .store()
                    .unwrap()
                    .get(&record.id)
                    .unwrap()
                    .pending_prompt()
                    .is_some()
            );
            let context = super::super::Context {
                tmux: Arc::new(super::super::Attached(Arc::clone(&env.client))),
                tasks: Arc::clone(&env.tasks),
                records: Arc::clone(&env.records),
                agents: Arc::clone(&env.agents),
                pending: env.pending.clone(),
                daemon: false,
            };
            // Independent waiters resume the same durable, still-unclaimed prompt.
            super::super::resume_prompts(&context).await;
            super::super::resume_prompts(&context).await;
        }
        tokio::time::timeout(WAIT, async {
            while std::fs::read_to_string(dir.join("ready")).unwrap()
                != replacement.replacement.pid.to_string()
            {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        state["pid"] = replacement.replacement.pid.into();
        state["sessionId"] = next_session.into();
        state["status"] = "idle".into();
        std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
        expected.push_str("Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.\n");
        tokio::time::timeout(WAIT, async {
            loop {
                let record = env.records.store().unwrap().get(&record.id).unwrap();
                if record.pending_prompt().is_none()
                    && !record.extra.contains_key("restartOperation")
                    && std::fs::read_to_string(dir.join("input")).unwrap() == expected
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        env.pending.finish(Duration::from_secs(1)).await;
        current = env.records.store().unwrap().get(&record.id).unwrap();
        assert_eq!(
            current.open_run().unwrap().agent_session.as_deref(),
            Some(next_session)
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("input")).unwrap(),
            expected
        );
    }
    assert_eq!(current.runs.len(), 3);

    let config_file = fixture.dir.join("workspaces.toml");
    let config = std::fs::read_to_string(&config_file)
        .unwrap()
        .replace("[[worker]]", "[workspace.loop]\nchecks = 'any'\n[[worker]]");
    std::fs::write(
        &config_file,
        format!("{config}\n[worker.loop]\ncheck = 'false'\nmax_attempts = 1\ntimeout = 10\n"),
    )
    .unwrap();
    let spec = crate::records::LoopSpec {
        check: "printf '%s' \"$MUXA_FIXTURE_ENV\" > check-env; test -f fixed".into(),
        max_attempts: Some(4),
        timeout: Some(23),
    };
    let store = env.records.store().unwrap();
    let legacy = store
        .create(NewRecord {
            workspace: "work".into(),
            worker: Some("Reviewer".into()),
            title: "Legacy worker".into(),
            dir: Some(dir.clone()),
            ..Default::default()
        })
        .unwrap();
    assert!(crate::loops::start(&env.records, &legacy.id, spec.clone(), false).is_err());
    assert_eq!(store.get(&legacy.id).unwrap(), legacy);
    let started = crate::loops::start(&env.records, &current.id, spec.clone(), false).unwrap();
    assert_eq!(started.task_loop().unwrap().unwrap().check, spec.check);
    current = store
        .update(&started.id, started.rev, |record| {
            let mut lp = record.task_loop().unwrap().unwrap();
            lp.attempts = 1;
            lp.state = crate::records::LoopState::Paused;
            lp.reason = Some(crate::records::Reason::Waiting);
            record.set_loop(Some(&lp)).unwrap();
        })
        .unwrap();
    let previous = current.task_loop().unwrap().unwrap();
    let rev = current.rev;
    current = super::super::restart::restart_record(
        &env,
        current,
        true,
        Some("Reviewer".into()),
        Some(rev),
    )
    .await
    .unwrap();
    assert_eq!(current.task_loop().unwrap().unwrap(), previous);
    let baseline = current
        .open_run()
        .unwrap()
        .restart_observation()
        .unwrap()
        .unwrap();
    state["pid"] = baseline.replacement.pid.into();
    state["sessionId"] = "0b8c3c0e-1111-4a2b-9c3d-000000000008".into();
    state["status"] = "idle".into();
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    expected.push_str("Read .muxa/TASK.md and .muxa/HANDOFF.md, then continue.\n");
    tokio::time::timeout(WAIT, async {
        loop {
            let record = store.get(&current.id).unwrap();
            if record.pending_prompt().is_none()
                && !record.extra.contains_key("restartOperation")
                && std::fs::read_to_string(dir.join("input")).unwrap() == expected
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    env.pending.finish(Duration::from_secs(1)).await;
    let armed = store
        .get(&current.id)
        .unwrap()
        .task_loop()
        .unwrap()
        .unwrap();
    assert_eq!(armed.state, crate::records::LoopState::Armed);
    assert_eq!(armed.check, previous.check);
    assert_eq!(armed.max_attempts, 4);
    assert_eq!(armed.timeout, 23);
    assert_eq!(armed.attempts, 1);
    let text = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(text.contains("Synthetic startup instructions."));
    assert!(text.contains(&spec.check));

    let mut engine =
        crate::loops::Engine::new(Arc::clone(&env.records), Arc::clone(&env.agents), None);
    state["status"] = "busy".into();
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    engine.tick(Some(&env.client)).await;
    state["status"] = "idle".into();
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    let failure = crate::loops::typing::fail_prompt(2);
    expected.push_str(&format!("{failure}\n"));
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            engine.tick(Some(&env.client)).await;
            let record = store.get(&current.id).unwrap();
            let lp = record.task_loop().unwrap().unwrap();
            if lp.state == crate::records::LoopState::Armed
                && lp.attempts == 2
                && std::fs::read_to_string(dir.join("input")).unwrap() == expected
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("check-env")).unwrap(),
        "synthetic workspace env"
    );
    let failed_check = store
        .get(&current.id)
        .unwrap()
        .task_loop()
        .unwrap()
        .unwrap();
    assert_eq!(failed_check.check, spec.check);
    assert_eq!(failed_check.max_attempts, 4);
    assert_eq!(failed_check.timeout, 23);
    assert_eq!(failed_check.last_check.unwrap().exit, Some(1));
    std::fs::write(dir.join("fixed"), b"synthetic fix").unwrap();
    state["status"] = "busy".into();
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    engine.tick(Some(&env.client)).await;
    state["status"] = "idle".into();
    std::fs::write(&state_file, serde_json::to_vec(&state).unwrap()).unwrap();
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            engine.tick(Some(&env.client)).await;
            if store.get(&current.id).unwrap().status == Status::Review {
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .unwrap();
    engine.stop_checks().await;
    let reviewed = store.get(&current.id).unwrap();
    let passed = reviewed.task_loop().unwrap().unwrap();
    assert_eq!(passed.state, crate::records::LoopState::Passed);
    assert_eq!(passed.check, spec.check);
    assert_eq!(passed.max_attempts, 4);
    assert_eq!(passed.timeout, 23);
    assert_eq!(passed.attempts, 2);
    assert_eq!(passed.last_check.unwrap().exit, Some(0));
    assert_eq!(
        std::fs::read_to_string(dir.join("input")).unwrap(),
        expected
    );
    assert_eq!(reviewed.worker(), Some("Reviewer"));
}

#[tokio::test]
async fn named_start_holds_its_claim_and_rejects_a_changed_launch_process() {
    let fixture = Fixture::new();
    let mut env = fixture.env().await;
    std::fs::write(
        fixture.dir.join("instructions.md"),
        "Synthetic startup guard.",
    )
    .unwrap();
    let proxy = fixture.dir.join("startup-proxy.py");
    let code = format!(
        r#"#!/usr/bin/env python3
import os, subprocess, sys, threading, time
args = sys.argv[1:]
if '-L' not in args or not args[args.index('-L') + 1].startswith('muxa-test-'):
    sys.exit(2)
real = {real}
if '-C' not in args:
    os.execv(real, [real] + args)
child = subprocess.Popen([real] + args, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
def feed():
    for line in sys.stdin.buffer:
        if line.startswith(b'set-option ') and b'@muxa_task' in line:
            with open({blocked}, 'wb') as marker:
                marker.write(b'held')
            deadline = time.monotonic() + 60
            while not os.path.exists({release}):
                if time.monotonic() > deadline:
                    child.terminate()
                    return
                time.sleep(0.01)
        try:
            child.stdin.write(line)
            child.stdin.flush()
        except BrokenPipeError:
            return
    child.terminate()
threading.Thread(target=feed, daemon=True).start()
while True:
    chunk = os.read(child.stdout.fileno(), 65536)
    if not chunk:
        break
    sys.stdout.buffer.write(chunk)
    sys.stdout.buffer.flush()
sys.exit(child.wait())
"#,
        real = serde_json::to_string(&fixture.options.tmux).unwrap(),
        blocked = serde_json::to_string(&fixture.dir.join("startup-blocked")).unwrap(),
        release = serde_json::to_string(&fixture.dir.join("startup-release")).unwrap(),
    );
    std::fs::write(&proxy, code).unwrap();
    std::fs::set_permissions(&proxy, std::fs::Permissions::from_mode(0o700)).unwrap();
    let options = muxa_tmux::Options {
        tmux: proxy,
        ..fixture.options.clone()
    };
    let (client, _notes) = Client::attach(&options, None).unwrap();
    env.client = Arc::new(client);
    let attempt = {
        let env = env.clone();
        tokio::spawn(async move {
            create(
                &env,
                NewTask {
                    workspace: "work".into(),
                    worker: Some("Reviewer".into()),
                    title: "Startup race".into(),
                    goal: "Synthetic fixture".into(),
                    ..Default::default()
                },
            )
            .await
        })
    };
    tokio::time::timeout(WAIT, async {
        while !fixture.dir.join("startup-blocked").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let store = env.records.store().unwrap();
    let record = store.list().unwrap().pop().unwrap();
    assert!(matches!(
        store.claim(&record.id, record.rev),
        Err(crate::records::StoreError::Busy(_))
    ));
    assert!(record.runs.is_empty());
    assert!(record.pending_prompt().is_none());
    let (outside, _notes) = Client::attach(&fixture.options, None).unwrap();
    let session = muxa_tmux::list_sessions(&outside)
        .await
        .unwrap()
        .into_iter()
        .find(|session| session.name != "fixture")
        .unwrap();
    let pane = session.panes[0].id;
    let dir = &session.panes[0].path;
    assert!(!PathBuf::from(dir).join(".muxa/TASK.md").exists());
    muxa_tmux::respawn_pane_command(
        &outside,
        pane,
        dir,
        &[],
        &["/bin/sleep".into(), "600".into()],
    )
    .await
    .unwrap();
    let replacement = fixture.process(pane);
    std::fs::write(fixture.dir.join("startup-release"), b"release").unwrap();
    let (failed, error) = attempt.await.unwrap();
    assert_eq!(error.unwrap().code, ErrorCode::TaskFailed);
    let failed = failed.unwrap();
    assert!(
        failed
            .note
            .as_ref()
            .is_some_and(|note| note.contains("process changed"))
    );
    assert!(failed.runs.is_empty());
    assert!(failed.pending_prompt().is_none());
    assert_eq!(fixture.process(pane), replacement);
    assert_eq!(store.get(&record.id).unwrap(), failed);
    assert!(
        store.claim(&record.id, failed.rev).is_ok(),
        "the failed startup released its claim"
    );
    env.pending.finish(Duration::ZERO).await;
}
