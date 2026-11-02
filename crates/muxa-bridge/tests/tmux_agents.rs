//! Opt-in contract test with a real tmux-agents (ADR 0017):
//! MUXA_TMUX_AGENTS=<tmux-agents checkout>. tmux-agents owns the worktree and
//! naming rules; the built-in launcher copies them. This fails when they drift,
//! and checks that `new.sh --muxa-task` links the session at creation.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::repo::Repo;
use common::{Bridge, Server};
use muxa_protocol::Message;
use serde_json::{Value, json};

/// Branches that exercise each rule: '/' in the path, '.' in the name.
const BRANCHES: [&str; 3] = ["feat/x", "fix/v1.2", "Plain_1"];
/// Session options both launchers set, compared as they are.
const TAGS: [&str; 3] = ["@agent_repo", "@agent_branch", "@muxa_task"];
const LAUNCH_WAIT: Duration = Duration::from_secs(30);

fn checkout() -> Option<PathBuf> {
    match std::env::var("MUXA_TMUX_AGENTS") {
        Ok(dir) => Some(PathBuf::from(dir)),
        Err(_) => {
            eprintln!("skipped: MUXA_TMUX_AGENTS is not set");
            None
        }
    }
}

/// A server with tmux-agents loaded, without its sidebar or a real agent.
fn agents_server(checkout: &Path) -> Server {
    let server = Server::start();
    for (name, value) in [
        ("@agents_sidebar", "off"),
        ("@agents_notify", "off"),
        ("@agents_check_gh", "off"),
        ("@agents_launch_cmd", "cat"),
    ] {
        server.tmux(&["set-option", "-g", name, value]);
    }
    let entry = checkout.join("agents.tmux");
    server.tmux(&["run-shell", entry.to_str().unwrap()]);
    server
}

/// A state directory with one workspace, and the bridge flags for it.
fn host(name: &str) -> (PathBuf, Vec<String>) {
    let dir = std::env::temp_dir().join(format!("muxa-contract-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("work")).unwrap();
    let config = dir.join("workspaces.toml");
    std::fs::write(
        &config,
        format!(
            "[[workspace]]\nname = \"work\"\nroot = \"{}\"\n",
            dir.join("work").display()
        ),
    )
    .unwrap();
    let args = vec![
        "--state-dir".into(),
        dir.join("state").display().to_string(),
        "--workspaces".into(),
        config.display().to_string(),
    ];
    (dir, args)
}

/// What a launch left behind: session name, worktree relative to the repo
/// root, and the tags. `@muxa_task` is compared as set or not.
fn outcome(server: &Server, repo: &Repo, session: &str) -> Vec<String> {
    let opt =
        |name: &str| server.tmux(&["show-options", "-qv", "-t", &format!("={session}:"), name]);
    let worktree = PathBuf::from(opt("@agent_worktree"));
    let mut out = vec![
        session.to_owned(),
        worktree
            .strip_prefix(&repo.root)
            .unwrap_or(&worktree)
            .display()
            .to_string(),
        PathBuf::from(opt("@agent_repo_dir"))
            .strip_prefix(&repo.root)
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    ];
    for tag in TAGS {
        let value = opt(tag);
        out.push(if tag == "@muxa_task" {
            format!("{tag} set={}", !value.is_empty())
        } else {
            format!("{tag}={value}")
        });
    }
    out
}

async fn create(bridge: &mut Bridge, repo: &Path, branch: &str) -> Value {
    // The real new.sh builds a layout and refreshes tmux-agents' status.
    let result = bridge
        .request_within(
            json!({"type": "create-task", "workspace": "work", "title": "t", "goal": "g", "repo": repo, "branch": branch}),
            LAUNCH_WAIT,
        )
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{branch}: {result:?}"
    );
    while let Some(msg) = bridge.event(Duration::from_millis(300)).await {
        if let Message::TaskSaved { record, .. } = msg {
            return serde_json::to_value(record).unwrap();
        }
    }
    panic!("{branch}: no task-saved")
}

#[tokio::test]
async fn the_builtin_launcher_follows_tmux_agents() {
    let Some(checkout) = checkout() else { return };
    let mut seen = Vec::new();
    for launcher in ["tmux-agents", "builtin"] {
        let server = if launcher == "builtin" {
            Server::start()
        } else {
            agents_server(&checkout)
        };
        let repo = Repo::new(&format!("contract-{launcher}"));
        let (dir, mut args) = host(launcher);
        if launcher == "builtin" {
            args.extend(["--task-launcher", "builtin", "--task-command", "cat"].map(String::from));
        }
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (mut bridge, _) = Bridge::start(&server, &args).await;
        let mut outcomes = Vec::new();
        for branch in BRANCHES {
            let record = create(&mut bridge, &repo.app, branch).await;
            let session = record["session"].as_str().unwrap().to_owned();
            outcomes.push(outcome(&server, &repo, &session));
        }
        drop(bridge);
        let _ = std::fs::remove_dir_all(dir);
        seen.push(outcomes);
    }
    assert_eq!(
        seen[1], seen[0],
        "the built-in launcher (left) no longer follows tmux-agents (right)"
    );
}

#[tokio::test]
async fn new_sh_links_the_session_when_it_creates_it() {
    let Some(checkout) = checkout() else { return };
    let server = agents_server(&checkout);
    assert_eq!(
        server.tmux(&["show-options", "-gqv", "@agents_features"]),
        "muxa-task"
    );
    // The tag is there before anything else could set it: new.sh ran alone.
    let repo = Repo::new("contract-link");
    let new_sh = checkout.join("scripts/new.sh");
    let out = std::process::Command::new(new_sh)
        .args([
            repo.app.to_str().unwrap(),
            "feat/y",
            "--muxa-task",
            "t-0000aa",
            "--no-agent",
            "--no-switch",
        ])
        .env("TMUX_AGENTS_SOCKET", &server.socket)
        .env_remove("TMUX")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "feat-y");
    assert_eq!(
        server.tmux(&["show-options", "-qv", "-t", "=feat-y:", "@muxa_task"]),
        "t-0000aa"
    );

    // Through the bridge: the record's id is on the session it names.
    let (dir, args) = host("link");
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let (mut bridge, _) = Bridge::start(&server, &args).await;
    let record = create(&mut bridge, &repo.app, "feat/z").await;
    assert_eq!(record["session"], "feat-z");
    assert_eq!(
        server.tmux(&["show-options", "-qv", "-t", "=feat-z:", "@muxa_task"]),
        record["id"].as_str().unwrap()
    );
    drop(bridge);
    let _ = std::fs::remove_dir_all(dir);
}
