//! The window follows the device in use (ADR 0020), on throwaway tmux servers
//! with a desktop terminal on a pty.
#![cfg(feature = "tmux-integration")]

mod common;

use std::time::{Duration, Instant};

use common::desktop::Desktop;
use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message};
use muxa_tmux::{Client, Flags, Options};
use serde_json::json;

const SETTLE: Duration = Duration::from_secs(8);

fn size(server: &Server, target: &str) -> String {
    server.tmux(&[
        "display-message",
        "-p",
        "-t",
        target,
        "#{window_width}x#{window_height}",
    ])
}

fn pane_size(server: &Server, pane: &str) -> String {
    server.tmux(&[
        "display-message",
        "-p",
        "-t",
        pane,
        "#{pane_width}x#{pane_height}",
    ])
}

/// Waits for `cond`, reading the bridge's messages meanwhile so its queue
/// never fills. Panics with `what` after `SETTLE`.
async fn wait_for(bridge: &mut Bridge, what: &str, mut cond: impl FnMut() -> bool) {
    let start = Instant::now();
    while start.elapsed() < SETTLE {
        if cond() {
            return;
        }
        let _ = bridge.event(Duration::from_millis(100)).await;
    }
    panic!("timed out waiting for {what}");
}

fn is_ok(msg: &Message) -> bool {
    matches!(msg, Message::Result { error: None, .. })
}

#[tokio::test]
async fn the_device_takes_the_window_and_the_desktop_takes_it_back() {
    let server = Server::start();
    let pane = server.pane("work");
    let mut desktop = Desktop::attach(&server, "work", 160, 40);
    common::wait_until(|| size(&server, "work") == "160x39").await;

    let (mut bridge, hello) = Bridge::start(&server, &[]).await;
    let Message::Hello { capabilities, .. } = hello else {
        panic!("{hello:?}")
    };
    assert!(capabilities.iter().any(|c| c == "viewport"));
    let reply = bridge
        .request(json!({"type": "viewport", "cols": 100, "rows": 30}))
        .await;
    assert!(is_ok(&reply), "{reply:?}");
    bridge.request(json!({"type": "watch", "pane": pane})).await;
    wait_for(&mut bridge, "the device's size", || {
        size(&server, "work") == "100x30"
    })
    .await;

    // The desktop types: the window is the desktop's again.
    desktop.type_key();
    wait_for(&mut bridge, "the desktop's size after it typed", || {
        size(&server, "work") == "160x39"
    })
    .await;

    // The device types: it takes the window back.
    bridge
        .request(json!({"type": "text", "pane": pane, "text": "true"}))
        .await;
    wait_for(&mut bridge, "the device's size after it typed", || {
        size(&server, "work") == "100x30"
    })
    .await;

    // A rotation: a new viewport.
    bridge
        .request(json!({"type": "viewport", "cols": 70, "rows": 40}))
        .await;
    wait_for(&mut bridge, "the new viewport", || {
        size(&server, "work") == "70x40"
    })
    .await;

    // The device leaves: the desktop's size returns.
    bridge.request(json!({"type": "bye"})).await;
    let _ = tokio::time::timeout(common::WAIT, bridge.child.wait()).await;
    common::wait_until(|| size(&server, "work") == "160x39").await;
    desktop.close();
}

#[tokio::test]
async fn the_watched_pane_of_a_split_gets_the_viewport() {
    let server = Server::start();
    // A tmux-agents-like split: a 28-column pane, then two that share the rest.
    server.tmux(&["split-window", "-d", "-h", "-t", "work", "sh"]);
    server.tmux(&["split-window", "-d", "-hbf", "-l", "28", "-t", "work", "sh"]);
    let panes = server.tmux(&["list-panes", "-t", "work", "-F", "#{pane_id}"]);
    let agent = panes.lines().nth(1).expect("the middle pane").to_owned();

    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    bridge
        .request(json!({"type": "viewport", "cols": 90, "rows": 30}))
        .await;
    bridge
        .request(json!({"type": "watch", "pane": agent}))
        .await;
    wait_for(&mut bridge, "the pane at the viewport", || {
        let got = pane_size(&server, &agent);
        let (cols, rows) = got.split_once('x').expect("WxH");
        let cols: u32 = cols.parse().expect("cols");
        rows == "30" && cols.abs_diff(90) <= 1
    })
    .await;
    let window = size(&server, "work");
    let cols: u32 = window
        .split_once('x')
        .expect("WxH")
        .0
        .parse()
        .expect("cols");
    assert!(
        cols > 90,
        "the window {window} did not grow past the viewport"
    );

    // Unwatch gives the size back; with no other client the window keeps it.
    bridge.request(json!({"type": "unwatch"})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(size(&server, "work"), window);
}

#[tokio::test]
async fn only_windows_that_follow_the_latest_client_are_sized() {
    let server = Server::start();
    let pane = server.pane("work");
    server.tmux(&["set-option", "-w", "-t", "work", "window-size", "manual"]);
    let before = server.layout();

    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    bridge
        .request(json!({"type": "viewport", "cols": 120, "rows": 40}))
        .await;
    bridge.request(json!({"type": "watch", "pane": pane})).await;
    bridge
        .request(json!({"type": "text", "pane": pane, "text": "true"}))
        .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(server.layout(), before);
}

#[tokio::test]
async fn a_viewport_outside_the_limits_is_a_bad_request() {
    let server = Server::start();
    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    for (cols, rows) in [(9, 30), (80, 4), (1001, 30)] {
        let reply = bridge
            .request(json!({"type": "viewport", "cols": cols, "rows": rows}))
            .await;
        let Message::Result { error: Some(e), .. } = reply else {
            panic!("{reply:?}")
        };
        assert_eq!(e.code, ErrorCode::BadRequest);
    }
}

#[tokio::test]
async fn the_daemon_client_never_resizes_a_window() {
    let server = Server::start();
    let pane = server.pane("work");
    let opts = Options {
        socket: Some(server.socket.clone()),
        ..Options::default()
    };

    // Alone on the server.
    let (daemon, _notes) =
        Client::attach_with(&opts, None, Flags { no_output: true }).expect("attach");
    for _ in 0..5 {
        daemon.command("list-sessions").await.expect("list");
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(size(&server, "work"), "80x24");

    // Next to a desktop and a sized bridge: when the bridge leaves, the
    // desktop's size returns; the daemon's client holds nothing.
    let desktop = Desktop::attach(&server, "work", 150, 40);
    common::wait_until(|| size(&server, "work") == "150x39").await;
    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    bridge
        .request(json!({"type": "viewport", "cols": 100, "rows": 30}))
        .await;
    bridge.request(json!({"type": "watch", "pane": pane})).await;
    wait_for(&mut bridge, "the device's size", || {
        size(&server, "work") == "100x30"
    })
    .await;
    bridge.request(json!({"type": "bye"})).await;
    let _ = tokio::time::timeout(common::WAIT, bridge.child.wait()).await;
    common::wait_until(|| size(&server, "work") == "150x39").await;
    daemon.command("list-sessions").await.expect("list");
    assert_eq!(size(&server, "work"), "150x39");
    desktop.close();
}
