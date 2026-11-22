use std::path::{Path, PathBuf};

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Exit, SessionBackend};
use serde_json::json;

use super::*;

struct Host {
    base: PathBuf,
    runner: Runner,
}

impl Host {
    fn new(name: &str) -> Host {
        Host::with(name, "")
    }

    /// `extra` is added to the workspace's `.muxa/routines.toml`.
    fn with(name: &str, extra: &str) -> Host {
        Host::with_workspace(name, extra, "")
    }

    /// `workspace` is added to the workspace's table in `workspaces.toml`.
    fn with_workspace(name: &str, extra: &str, workspace: &str) -> Host {
        let base = std::env::temp_dir().join(format!("muxa-runner-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root/.muxa")).unwrap();
        std::fs::create_dir_all(base.join("state")).unwrap();
        let base = base.canonicalize().unwrap();
        std::fs::write(base.join("root/.muxa/brief.md"), "Read the sources.\n").unwrap();
        std::fs::write(
            base.join("root/.muxa/routines.toml"),
            format!(
                "[[routine]]\nname = \"Brief\"\ninstructions = \"brief.md\"\nenabled = true\n{extra}"
            ),
        )
        .unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.routines]\nenabled = true\n{workspace}\
                 [[worker]]\nname = \"Cx\"\nagent = \"codex\"\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        let config = Arc::new(crate::config::Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(base.join("workspaces.toml")),
            home: None,
        });
        Host {
            runner: Runner::new(config, "claude".into()),
            base,
        }
    }

    fn state(&self) -> RoutineState {
        self.state_of("Brief")
    }

    fn state_of(&self, name: &str) -> RoutineState {
        RoutineStore::open(&self.base.join("state"))
            .unwrap()
            .get("w", name)
            .unwrap()
    }

    async fn started(&self, backend: &FakeBackend) -> TerminalInfo {
        self.runner
            .start(backend, "w", "Brief", When::Now(now()))
            .await
            .unwrap();
        backend.list().await.unwrap().into_iter().next().unwrap()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn write_output(t: &TerminalInfo, output: serde_json::Value) {
    let dir = PathBuf::from(&t.labels["muxa.dir"]).join(".muxa/routine");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("output.json"), output.to_string()).unwrap();
}

fn good() -> serde_json::Value {
    json!({"version": 1, "items": [{"kind": "briefing", "key": "k1", "title": "News", "body": "b"}]})
}

#[tokio::test]
async fn a_run_starts_its_agent_in_its_folder_with_the_prompt() {
    let h = Host::new("start");
    let backend = FakeBackend::new();
    let t = h.started(&backend).await;
    let launch = &backend.launches()[0];
    assert_eq!(launch.argv[..2], ["/bin/sh", "-c"]);
    let dir = launch.cwd.clone().unwrap();
    // A workspace without [[workspace.mcp]]: the agent keeps its own login's servers.
    assert_eq!(launch.argv[2], "claude 'Read .muxa/TASK.md and start.'");
    assert!(!dir.join(".muxa/mcp.json").exists());
    assert!(
        dir.starts_with(h.base.join("root/routines")),
        "{}",
        dir.display()
    );
    let brief = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(brief.starts_with("# Brief now-"), "{brief}");
    assert!(brief.contains("Read the sources."));
    assert_eq!(t.labels["muxa.kind"], "routine");
    let open = h.state().running.unwrap();
    assert_eq!(open.task.as_deref(), Some(t.labels["muxa.run"].as_str()));
    // One open run per Routine.
    assert!(matches!(
        h.runner
            .start(&backend, "w", "Brief", When::Now(now()))
            .await,
        Err(StartError::Busy(_))
    ));
}

#[tokio::test]
async fn a_good_output_publishes_and_ends_the_agent() {
    let h = Host::new("publish");
    let backend = FakeBackend::new();
    let t = h.started(&backend).await;
    write_output(&t, good());
    h.runner.turn_ended(&backend, &t).await;
    let state = h.state();
    assert!(state.running.is_none());
    assert_eq!(state.last_success.unwrap().published, 1);
    assert_eq!(state.entries[0].entry.title, "News");
    assert!(!backend.get(&t.id).await.unwrap().live, "the agent ended");
}

#[tokio::test]
async fn a_bad_output_types_the_fix_prompt_then_gives_up() {
    let h = Host::new("retry");
    let backend = FakeBackend::new();
    let t = h.started(&backend).await;
    write_output(&t, json!({"version": 1, "items": [{"kind": "nope"}]}));
    h.runner.turn_ended(&backend, &t).await;
    let typed: Vec<u8> = backend.inputs(&t.id).concat();
    let typed = String::from_utf8(typed).unwrap();
    assert_eq!(typed, format!("{}\r", fail_prompt(1)));
    assert!(h.state().running.is_some(), "still open");
    for _ in 1..MAX_ATTEMPTS {
        h.runner.turn_ended(&backend, &t).await;
    }
    let state = h.state();
    assert!(state.running.is_none());
    assert_eq!(
        state.last_failure.unwrap().reason,
        format!("The output check failed {MAX_ATTEMPTS} times.")
    );
    assert!(!backend.get(&t.id).await.unwrap().live);
}

#[tokio::test]
async fn an_agent_that_ends_or_is_gone_fails_the_run() {
    let h = Host::new("ended");
    let backend = FakeBackend::new();
    let t = h.started(&backend).await;
    backend
        .end(
            &t.id,
            Exit {
                reason: "exited".into(),
                code: Some(0),
                signal: None,
            },
        )
        .unwrap();
    h.runner.agent_ended(&t).await;
    assert_eq!(
        h.state().last_failure.unwrap().reason,
        "The agent ended without a publish."
    );

    // A run whose agent is not live any more fails at the next tick.
    let t = h.started(&backend).await;
    let _ = t;
    let other = FakeBackend::new();
    h.runner.tick(&other).await;
    assert_eq!(
        h.state().last_failure.unwrap().reason,
        "The run's agent is gone."
    );
}

#[tokio::test]
async fn the_tick_starts_a_scheduled_run_once() {
    let h = Host::with(
        "tick",
        "[[routine]]\nname = \"Daily\"\ninstructions = \"brief.md\"\n\
         at = \"00:00\"\ntimezone = \"UTC\"\nenabled = true\n",
    );
    let backend = FakeBackend::new();
    // The first look records the latest past occurrence without running it.
    h.runner.tick(&backend).await;
    assert!(backend.launches().is_empty());
    let before = jiff::Zoned::now()
        .with_time_zone(jiff::tz::TimeZone::UTC)
        .date()
        .checked_sub(jiff::Span::new().days(2))
        .unwrap();
    RoutineStore::open(&h.base.join("state"))
        .unwrap()
        .update::<_, StateError>("w", "Daily", |s| {
            s.last_occurrence = Some(before.to_string());
            Ok(())
        })
        .unwrap();
    h.runner.tick(&backend).await;
    h.runner.tick(&backend).await;
    let launches = backend.launches();
    assert_eq!(launches.len(), 1, "one run for the missed days");
    let t = backend.list().await.unwrap().into_iter().next().unwrap();
    assert!(t.labels[OCCURRENCE].ends_with("T00:00"), "{:?}", t.labels);
    assert_eq!(t.labels[ROUTINE], "Daily");
    assert!(h.state_of("Daily").running.is_some());
}

#[tokio::test]
async fn a_codex_routine_is_refused_and_kept_as_a_failure() {
    let h = Host::with(
        "codex",
        "[[routine]]\nname = \"Codex\"\ninstructions = \"brief.md\"\n\
         worker = \"Cx\"\nenabled = true\n",
    );
    let backend = FakeBackend::new();
    let refused = h
        .runner
        .start(&backend, "w", "Codex", When::Now(now()))
        .await;
    assert!(
        matches!(refused, Err(StartError::Unsupported(_))),
        "{refused:?}"
    );
    assert!(backend.launches().is_empty());
    assert!(h.state_of("Codex").last_failure.is_some());
}

#[tokio::test]
async fn a_publish_removes_the_previous_published_runs_folder() {
    let h = Host::new("prune");
    let backend = FakeBackend::new();
    let first = h.started(&backend).await;
    write_output(&first, good());
    h.runner.turn_ended(&backend, &first).await;
    backend.remove(&first.id).await.unwrap();
    let second = h.started(&backend).await;
    write_output(&second, good());
    h.runner.turn_ended(&backend, &second).await;
    assert_eq!(h.state().last_success.unwrap().task, second.labels[RUN]);
    assert!(!Path::new(&first.labels["muxa.dir"]).exists());
    assert!(Path::new(&second.labels["muxa.dir"]).exists());
}

#[test]
fn a_worker_gets_the_prompt_as_its_last_argument() {
    let native = LaunchCommand::Native {
        program: "claude",
        args: vec!["--agent".into(), "reviewer".into()],
    };
    assert_eq!(argv(&native, "go"), ["claude", "--agent", "reviewer", "go"]);
    assert_eq!(
        argv(&LaunchCommand::Shell("c".into()), "it's"),
        ["/bin/sh", "-c", "c 'it'\\''s'"]
    );
}

#[tokio::test]
async fn a_run_gets_only_the_mcp_servers_its_routine_names() {
    let h = Host::with_workspace(
        "mcp",
        "mcp = [\"docs\"]\n",
        "[[workspace.mcp]]\nname = \"docs\"\ncommand = \"docs-mcp\"\n\
         [[workspace.mcp]]\nname = \"web\"\nurl = \"https://web.example/mcp\"\n",
    );
    let backend = FakeBackend::new();
    h.started(&backend).await;
    let launch = &backend.launches()[0];
    let dir = launch.cwd.clone().unwrap();
    assert!(
        launch.argv[2].contains("--strict-mcp-config 'Read .muxa/TASK.md and start.'"),
        "{}",
        launch.argv[2]
    );
    let config = std::fs::read_to_string(dir.join(".muxa/mcp.json")).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&config).unwrap(),
        json!({"mcpServers": {"docs": {"type": "stdio", "command": "docs-mcp", "args": [], "env": {}}}})
    );
    let usage = crate::usage::read(&h.base.join("state")).unwrap().of("w");
    assert_eq!(
        usage.mcp.keys().collect::<Vec<_>>(),
        ["docs"],
        "the run's start is noted for the servers it got"
    );
}
