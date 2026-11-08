//! `history` sends a pane's scrollback and screen, and changes nothing in the
//! pane, on throwaway tmux servers.
#![cfg(feature = "tmux-integration")]

mod common;

use std::time::Duration;

use common::{Bridge, Server};
use muxa_protocol::{ErrorCode, Message};
use serde_json::json;

/// A pane that printed 200 red lines, most of them now in its scrollback.
async fn printed(server: &Server) -> String {
    let pane = server.pane("work");
    server.tmux(&[
        "send-keys",
        "-t",
        &pane,
        "-l",
        r"i=1; while [ $i -le 200 ]; do printf '\033[31mline %d\033[0m\n' $i; i=$((i+1)); done",
    ]);
    server.tmux(&["send-keys", "-t", &pane, "Enter"]);
    common::wait_until(|| {
        server
            .tmux(&["display-message", "-p", "-t", &pane, "#{history_size}"])
            .parse::<u32>()
            .is_ok_and(|n| n >= 150)
    })
    .await;
    pane
}

async fn history(bridge: &mut Bridge, req: serde_json::Value) -> (Message, Option<Message>) {
    let result = bridge.request(req).await;
    let message = bridge.event(Duration::from_millis(500)).await;
    (result, message)
}

#[tokio::test]
async fn history_sends_the_scrollback_and_the_screen_and_changes_nothing() {
    let server = Server::start();
    let pane = printed(&server).await;
    let before = server.layout();
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    let (result, message) = history(&mut bridge, json!({"type": "history", "pane": pane})).await;
    assert!(
        matches!(result, Message::Result { error: None, .. }),
        "{result:?}"
    );
    let Some(Message::History {
        pane: got,
        cols,
        data,
        truncated,
        ..
    }) = message
    else {
        panic!("{message:?}")
    };
    assert_eq!((got.as_str(), cols, truncated), (pane.as_str(), 80, false));
    let text = String::from_utf8_lossy(&data);
    assert!(
        text.contains("\u{1b}[31mline 1\u{1b}"),
        "colors and the first line"
    );
    assert!(text.contains("line 200"));
    assert!(text.ends_with('\n'));

    assert_eq!(server.layout(), before);
    let mode = server.tmux(&["display-message", "-p", "-t", &pane, "#{pane_in_mode}"]);
    assert_eq!(mode, "0", "no copy mode");
}

#[tokio::test]
async fn fewer_lines_than_the_pane_keeps_are_truncated() {
    let server = Server::start();
    let pane = printed(&server).await;
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    let (_, message) = history(
        &mut bridge,
        json!({"type": "history", "pane": pane, "lines": 10}),
    )
    .await;
    let Some(Message::History {
        data, truncated, ..
    }) = message
    else {
        panic!("{message:?}")
    };
    assert!(truncated);
    let text = String::from_utf8_lossy(&data);
    assert!(!text.contains("line 50\u{1b}"));
    assert!(text.contains("line 200"));
}

#[tokio::test]
async fn lines_out_of_range_and_bad_panes_are_refused() {
    let server = Server::start();
    let pane = server.pane("work");
    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    for (req, code) in [
        (
            json!({"type": "history", "pane": pane, "lines": 0}),
            ErrorCode::BadRequest,
        ),
        (
            json!({"type": "history", "pane": pane, "lines": 10_001}),
            ErrorCode::BadRequest,
        ),
        (
            json!({"type": "history", "pane": "work"}),
            ErrorCode::InvalidPane,
        ),
        (
            json!({"type": "history", "pane": "%999"}),
            ErrorCode::NoPane,
        ),
    ] {
        let reply = bridge.request(req).await;
        let Message::Result { error: Some(e), .. } = reply else {
            panic!("{reply:?}")
        };
        assert_eq!(e.code, code);
    }
}
