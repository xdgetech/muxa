//! `docs` and `doc` end to end, for a pane whose working directory is a git repo.
#![cfg(feature = "tmux-integration")]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::{Bridge, Server};
use muxa_protocol::{DocKind, ErrorCode, Message};
use serde_json::json;

/// Longer than a few document ticks.
const WAIT: Duration = Duration::from_secs(4);

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-docs-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("docs")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(dir.join(".gitignore"), "secret.md\n").unwrap();
        std::fs::write(dir.join("README.md"), "# Readme\n").unwrap();
        std::fs::write(
            dir.join("docs/plan.md"),
            "# Plan\n\n```mermaid\nflowchart LR\n  A --> B\n```\n",
        )
        .unwrap();
        std::fs::write(dir.join("secret.md"), "token\n").unwrap();
        Self { dir }
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A pane running a long command inside `dir`, so its current path is there.
fn pane_in(server: &Server, dir: &Path) -> String {
    let dir = dir.display().to_string();
    server.tmux(&[
        "new-window",
        "-d",
        "-P",
        "-F",
        "#{pane_id}",
        "-c",
        &dir,
        "sleep 600",
    ])
}

/// An empty agent directory: these tests must not read the developer's own.
fn no_agents(name: &str) -> String {
    let dir =
        std::env::temp_dir().join(format!("muxa-docs-it-agents-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    format!("claude-code={}", dir.display())
}

async fn content(bridge: &mut Bridge) -> Message {
    loop {
        match bridge.event(WAIT).await.expect("no document message") {
            msg @ (Message::DocContent { .. } | Message::DocEnded { .. }) => return msg,
            _ => continue,
        }
    }
}

#[tokio::test]
async fn lists_opens_and_follows_a_document() {
    let server = Server::start();
    let repo = Repo::new("follow");
    let pane = pane_in(&server, &repo.dir);
    let (mut bridge, hello) = Bridge::start(&server, &["--agent-dir", &no_agents("follow")]).await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.contains(&"docs".to_owned()));

    let result = bridge.request(json!({"type": "docs", "pane": pane})).await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let Some(Message::DocList {
        docs,
        root,
        entries,
        ..
    }) = bridge.event(WAIT).await
    else {
        panic!("no doc-list")
    };
    assert_eq!(docs, bridge.last_id());
    assert!(root.is_some());
    let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(paths, ["README.md", "docs/plan.md"]);
    assert!(entries.iter().all(|e| e.kind == DocKind::Repo));

    let result = bridge
        .request(json!({"type": "doc", "pane": pane, "path": "docs/plan.md"}))
        .await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let open = bridge.last_id();
    let Message::DocContent { doc, text, .. } = content(&mut bridge).await else {
        panic!()
    };
    assert_eq!(doc, open);
    assert!(text.contains("flowchart LR"));

    // An edit is sent again; an untouched file is not.
    std::fs::write(repo.path("docs/plan.md"), "# Plan v2\n").unwrap();
    let Message::DocContent { text, .. } = content(&mut bridge).await else {
        panic!()
    };
    assert_eq!(text, "# Plan v2\n");

    std::fs::remove_file(repo.path("docs/plan.md")).unwrap();
    assert!(matches!(content(&mut bridge).await, Message::DocEnded { doc, .. } if doc == open));
}

#[tokio::test]
async fn only_listed_documents_can_be_opened() {
    let server = Server::start();
    let repo = Repo::new("refuse");
    let pane = pane_in(&server, &repo.dir);
    let (mut bridge, _) = Bridge::start(&server, &["--agent-dir", &no_agents("refuse")]).await;
    for path in ["secret.md", "../outside.md", "/etc/hosts"] {
        let Message::Result {
            error: Some(error), ..
        } = bridge
            .request(json!({"type": "doc", "pane": pane, "path": path}))
            .await
        else {
            panic!("{path} was opened")
        };
        assert_eq!(error.code, ErrorCode::BadRequest, "{path}");
    }
}
