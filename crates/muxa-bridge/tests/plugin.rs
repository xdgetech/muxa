//! Opt-in: MUXA_TEST_PLUGIN=<plugin entry file> loads a real tmux plugin that
//! reacts to client attach and resize (e.g. tmux-agents' agents.tmux). The
//! bridge must leave the layout it manages untouched.
#![cfg(feature = "tmux-integration")]

mod common;

use std::time::Duration;

use common::{Bridge, Server};
use serde_json::json;

#[tokio::test]
async fn a_layout_plugin_keeps_its_layout() {
    let Ok(plugin) = std::env::var("MUXA_TEST_PLUGIN") else {
        eprintln!("skipped: MUXA_TEST_PLUGIN is not set");
        return;
    };
    let server = Server::start();
    server.tmux(&["run-shell", &plugin]);
    server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "task",
        "-x",
        "200",
        "-y",
        "50",
        "sh",
    ]);
    server.tmux(&["split-window", "-h", "-d", "-t", "task", "sh"]);
    // The plugin's hooks run in the background.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let before = server.layout();
    assert!(
        before.lines().count() > 3,
        "the plugin added no panes; is it the right file?\n{before}"
    );

    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    for target in ["task", "work", "task"] {
        let pane = server.pane(target);
        bridge.request(json!({"type": "watch", "pane": pane})).await;
        bridge.request(json!({"type": "list"})).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        server.layout(),
        before,
        "the plugin changed the layout while the bridge was attached"
    );
    bridge.request(json!({"type": "bye"})).await;
}
