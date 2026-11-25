use std::time::Instant;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Exit, Launch};

use super::*;

const CHECK: &str = "test -f ok";

struct Host {
    base: PathBuf,
    task: PathBuf,
    loops: Arc<TaskLoops>,
    inbox: Arc<Inbox>,
    backend: FakeBackend,
}

impl Host {
    async fn new(name: &str) -> Host {
        let base = std::env::temp_dir().join(format!("muxa-loop-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root/tasks/fix-1")).unwrap();
        let base = base.canonicalize().unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.loop]\nchecks = [\"{CHECK}\"]\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        let config = Arc::new(crate::config::Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(base.join("workspaces.toml")),
            home: None,
        });
        let inbox = Arc::new(Inbox::memory());
        let loops = Arc::new(TaskLoops::new(config, Arc::clone(&inbox)));
        let backend = FakeBackend::new();
        let watcher = (Arc::clone(&loops), backend.clone());
        tokio::spawn(async move { watcher.0.watch(&watcher.1).await });
        Host {
            task: base.join("root/tasks/fix-1"),
            base,
            loops,
            inbox,
            backend,
        }
    }

    async fn agent(&self) -> TerminalInfo {
        self.agent_on(None).await
    }

    /// A Task agent on `subscription`, as its `muxa.subscription` label says.
    async fn agent_on(&self, subscription: Option<&str>) -> TerminalInfo {
        let mut labels = Labels::from([
            (KIND.to_owned(), "task".to_owned()),
            (ROLE.to_owned(), "agent".to_owned()),
            (DIR.to_owned(), self.task.display().to_string()),
            (WORKSPACE.to_owned(), "w".to_owned()),
        ]);
        if let Some(s) = subscription {
            labels.insert(SUBSCRIPTION.to_owned(), s.to_owned());
        }
        let launch = Launch {
            name: Some("fix".into()),
            labels,
            ..Launch::default()
        };
        let t = self.backend.run(&launch).await.unwrap();
        // The watch sees the new agent before a test drives it.
        tokio::time::sleep(Duration::from_millis(50)).await;
        t
    }

    async fn state(&self) -> LoopInfo {
        self.loops.list().await.into_iter().next().expect("a loop")
    }

    /// Waits until the loop is `state` with `attempts`.
    async fn until(&self, state: &str, attempts: u32) -> LoopInfo {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let lp = self.state().await;
            if lp.state == state && lp.attempts == attempts {
                return lp;
            }
            assert!(Instant::now() < deadline, "the loop stayed {lp:?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    fn items(&self) -> Vec<(ItemKind, String)> {
        self.inbox
            .view()
            .items
            .into_iter()
            .map(|i| (i.kind, i.title))
            .collect()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

#[tokio::test]
async fn a_failed_check_types_the_prompt_and_a_pass_makes_a_review() {
    let h = Host::new("pass").await;
    let t = h.agent().await;
    let started = h
        .loops
        .start(&t, &v2::LoopSpec::default(), false)
        .await
        .unwrap();
    assert_eq!(
        (started.check.as_str(), started.state.as_str()),
        (CHECK, "armed")
    );

    h.backend.set_status(&t.id, "done");
    h.until("armed", 1).await;
    let typed = String::from_utf8(h.backend.inputs(&t.id).concat()).unwrap();
    assert_eq!(typed, format!("{}\r", fail_prompt(1)));
    assert!(h.task.join(".muxa/checks/1.log").is_file());
    assert!(h.items().is_empty());

    std::fs::write(h.task.join("ok"), "").unwrap();
    h.backend.set_status(&t.id, "working");
    h.backend.set_status(&t.id, "done");
    let passed = h.until("passed", 1).await;
    assert_eq!(passed.last_check.unwrap().exit, Some(0));
    assert_eq!(h.items(), [(ItemKind::Review, "Review fix".to_owned())]);
}

#[tokio::test]
async fn the_last_failed_attempt_stops_with_a_failed_item() {
    let h = Host::new("limit").await;
    let t = h.agent().await;
    let spec = v2::LoopSpec {
        max_attempts: Some(1),
        ..v2::LoopSpec::default()
    };
    h.loops.start(&t, &spec, false).await.unwrap();
    h.backend.set_status(&t.id, "done");
    let stopped = h.until("stopped", 1).await;
    assert_eq!(stopped.reason.as_deref(), Some("limit"));
    assert!(h.backend.inputs(&t.id).is_empty(), "nothing typed");
    assert_eq!(h.items()[0].0, ItemKind::Failed);
}

#[tokio::test]
async fn a_question_pauses_and_the_user_or_an_exit_stops() {
    let h = Host::new("pause").await;
    let t = h.agent().await;
    h.loops
        .start(&t, &v2::LoopSpec::default(), false)
        .await
        .unwrap();
    h.backend.set_status(&t.id, "blocked");
    let paused = h.until("paused", 0).await;
    assert_eq!(paused.reason.as_deref(), Some("waiting"));
    h.backend.set_status(&t.id, "working");
    h.until("armed", 0).await;

    let stopped = h.loops.stop(t.id.as_str()).await.unwrap();
    assert_eq!(
        (stopped.state.as_str(), stopped.reason.as_deref()),
        ("stopped", Some("user"))
    );
    // A stopped loop checks nothing.
    h.backend.set_status(&t.id, "done");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(h.state().await.attempts, 0);

    h.loops
        .start(&t, &v2::LoopSpec::default(), true)
        .await
        .unwrap();
    let exit = Exit {
        reason: "exited".into(),
        code: Some(0),
        signal: None,
    };
    h.backend.end(&t.id, exit).unwrap();
    let gone = h.until("stopped", 0).await;
    assert_eq!(gone.reason.as_deref(), Some("no-agent"));
}

#[tokio::test]
async fn only_an_allowed_check_starts() {
    let h = Host::new("allowed").await;
    let t = h.agent().await;
    let other = v2::LoopSpec {
        check: Some("rm -rf /".into()),
        ..v2::LoopSpec::default()
    };
    assert!(matches!(
        h.loops.start(&t, &other, false).await,
        Err(TaskError::Invalid(_))
    ));
    assert!(matches!(
        h.loops.resolve("nope", &v2::LoopSpec::default()).await,
        Err(TaskError::Invalid(_))
    ));
    assert!(h.loops.list().await.is_empty());
}

#[tokio::test]
async fn a_restarted_daemon_finds_the_loop_in_the_folder() {
    let h = Host::new("restart").await;
    let t = h.agent().await;
    h.loops
        .start(&t, &v2::LoopSpec::default(), false)
        .await
        .unwrap();
    let again = Arc::new(TaskLoops::new(
        Arc::clone(&h.loops.config),
        Arc::new(Inbox::memory()),
    ));
    let watcher = (Arc::clone(&again), h.backend.clone());
    tokio::spawn(async move { watcher.0.watch(&watcher.1).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let found = again.list().await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].terminal, t.id.as_str());
}

#[tokio::test]
async fn the_fail_prompt_waits_while_the_agents_login_is_full() {
    let h = Host::new("full").await;
    std::fs::write(
        h.base.join("workspaces.toml"),
        format!(
            "[[subscription]]\nname = \"work\"\nagent = \"claude-code\"\nhome = \"{}\"\n\
             [[workspace]]\nname = \"w\"\nroot = \"{}\"\nsubscriptions = [\"work\"]\n\
             [workspace.loop]\nchecks = [\"{CHECK}\"]\n",
            h.base.join(".claude-work").display(),
            h.base.join("root").display()
        ),
    )
    .unwrap();
    let resets = now() + 2;
    let w = crate::limits::Window {
        minutes: 300,
        used_percent: 97.0,
        resets_at: resets,
    };
    crate::limits::record(&h.base.join("state"), "work", &[w], now()).unwrap();
    let t = h.agent_on(Some("work")).await;
    h.loops
        .start(&t, &v2::LoopSpec::default(), false)
        .await
        .unwrap();

    h.backend.set_status(&t.id, "done");
    let held = h.until("paused", 1).await;
    assert_eq!(held.reason.as_deref(), Some("limit-reset"));
    assert_eq!(held.resumes_at, Some(resets));
    assert!(h.backend.inputs(&t.id).is_empty(), "nothing typed yet");

    let armed = h.until("armed", 1).await;
    assert_eq!(armed.resumes_at, None);
    let typed = String::from_utf8(h.backend.inputs(&t.id).concat()).unwrap();
    assert_eq!(typed, format!("{}\r", fail_prompt(1)));
}
