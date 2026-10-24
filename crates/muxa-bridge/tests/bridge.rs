//! The bridge binary end to end, on throwaway tmux servers.
#![cfg(feature = "tmux-integration")]

mod common;

use std::time::Duration;

use common::{Bridge, Server, Term, drain, wait_until};
use muxa_protocol::{ErrorCode, Message, Role};
use serde_json::json;

const QUIET: Duration = Duration::from_millis(300);

fn error_code(msg: &Message) -> Option<ErrorCode> {
    match msg {
        Message::Result { error, .. } => error.as_ref().map(|e| e.code),
        _ => None,
    }
}

#[tokio::test]
async fn says_hello_then_answers_requests() {
    let server = Server::start();
    let (mut bridge, hello) = Bridge::start(&server, &[]).await;
    let Message::Hello { protocol, tmux, .. } = hello else {
        panic!("first message was {hello:?}")
    };
    assert_eq!(protocol, muxa_protocol::VERSION);
    assert!(!tmux.is_empty());

    assert_eq!(
        error_code(&bridge.request(json!({"type": "ping"})).await),
        None
    );
    let Message::Result {
        sessions: Some(sessions),
        ..
    } = bridge.request(json!({"type": "list"})).await
    else {
        panic!("list returned no sessions")
    };
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].name, "work");
    assert_eq!(sessions[0].panes[0].role, Role::Unknown);
}

#[tokio::test]
async fn rejects_bad_requests_and_keeps_going() {
    let server = Server::start();
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    bridge.send_raw("not json").await;
    assert_eq!(
        error_code(&bridge.result(None).await),
        Some(ErrorCode::BadRequest)
    );
    bridge.send_raw(r#"{"type":"rm-rf","id":77}"#).await;
    assert_eq!(
        error_code(&bridge.result(Some(77)).await),
        Some(ErrorCode::BadRequest)
    );
    bridge
        .send_raw(&format!(
            r#"{{"type":"text","id":78,"pane":"%0","text":"{}"}}"#,
            "x".repeat(5 << 20)
        ))
        .await;
    assert_eq!(
        error_code(&bridge.result(None).await),
        Some(ErrorCode::TooLarge)
    );

    let r = bridge
        .request(json!({"type": "keys", "pane": "%0;kill-server", "keys": ["a"]}))
        .await;
    assert_eq!(error_code(&r), Some(ErrorCode::InvalidPane));
    let r = bridge
        .request(json!({"type": "keys", "pane": "%0", "keys": ["a;kill-server"]}))
        .await;
    assert_eq!(error_code(&r), Some(ErrorCode::UnsupportedKey));
    let r = bridge
        .request(json!({"type": "watch", "pane": "%999"}))
        .await;
    assert_eq!(
        error_code(&r),
        None,
        "watch is accepted; the pane is checked by the snapshot"
    );
    assert!(matches!(
        bridge.event(common::WAIT).await,
        Some(Message::PaneGone { .. })
    ));

    assert_eq!(
        error_code(&bridge.request(json!({"type": "ping"})).await),
        None
    );
    assert!(
        server.tmux(&["list-sessions"]).contains("work"),
        "the server survived"
    );
}

#[tokio::test]
async fn watch_shows_the_screen_and_streams_input_effects() {
    let server = Server::start();
    let pane = server.pane("work");
    server.tmux(&["send-keys", "-t", &pane, "echo before-watch", "Enter"]);
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    assert_eq!(
        error_code(&bridge.request(json!({"type": "watch", "pane": pane})).await),
        None
    );
    let watch = bridge.last_id();
    let mut term = Term::new();
    let first = bridge.event(common::WAIT).await.expect("screen");
    assert!(
        matches!(
            first,
            Message::Screen {
                cols: 80,
                rows: 24,
                ..
            }
        ),
        "{first:?}"
    );
    term.apply(&first, watch);
    assert!(term.rows().iter().any(|r| r == "before-watch"));

    bridge
        .request(json!({"type": "text", "pane": pane, "text": "echo 'é😀 \"$HOME\"'"}))
        .await;
    bridge
        .request(json!({"type": "keys", "pane": pane, "keys": ["Enter"]}))
        .await;
    drain(&mut bridge, &mut term, watch, QUIET).await;
    assert!(
        term.rows().iter().any(|r| r == "é😀 \"$HOME\""),
        "{:?}",
        term.rows()
    );
    assert_eq!(term.rows(), server.capture(&pane));
    assert_eq!(term.cursor(), server.cursor(&pane));
}

#[tokio::test]
async fn keys_and_bracketed_paste_reach_the_pane() {
    let server = Server::start();
    let pane = server.pane("work");
    let command = || {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{pane_current_command}",
        ])
    };
    // /bin/sh reports as "bash" on macOS.
    let shell = command();
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    bridge
        .request(json!({"type": "text", "pane": pane, "text": "sleep 100"}))
        .await;
    bridge
        .request(json!({"type": "keys", "pane": pane, "keys": ["Enter"]}))
        .await;
    wait_until(|| {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{pane_current_command}",
        ]) == "sleep"
    })
    .await;
    bridge
        .request(json!({"type": "keys", "pane": pane, "keys": ["C-c"]}))
        .await;
    wait_until(|| command() == shell).await;

    // Not bracketed: the program did not ask for it.
    bridge
        .request(json!({"type": "text", "pane": pane, "text": "clear; cat -v"}))
        .await;
    bridge
        .request(json!({"type": "keys", "pane": pane, "keys": ["Enter"]}))
        .await;
    wait_until(|| {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{pane_current_command}",
        ]) == "cat"
    })
    .await;
    bridge
        .request(json!({"type": "paste", "pane": pane, "text": "plain\n"}))
        .await;
    wait_until(|| server.capture(&pane).iter().any(|r| r.contains("plain"))).await;
    assert!(!server.capture(&pane).iter().any(|r| r.contains("200~")));
    bridge
        .request(json!({"type": "keys", "pane": pane, "keys": ["C-c"]}))
        .await;

    // Bracketed: the program enabled it.
    bridge
        .request(
            json!({"type": "text", "pane": pane, "text": "clear; printf '\\033[?2004h'; cat -v"}),
        )
        .await;
    bridge
        .request(json!({"type": "keys", "pane": pane, "keys": ["Enter"]}))
        .await;
    wait_until(|| {
        server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{bracket_paste_flag}",
        ]) == "1"
    })
    .await;
    bridge
        .request(json!({"type": "paste", "pane": pane, "text": "fenced\n"}))
        .await;
    wait_until(|| {
        server
            .capture(&pane)
            .iter()
            .any(|r| r.contains("^[[200~fenced"))
    })
    .await;
    assert!(
        server.tmux(&["list-buffers"]).is_empty(),
        "paste left a buffer behind"
    );
}

#[tokio::test]
async fn follows_the_pane_across_sessions_resizes_and_exit() {
    let server = Server::start();
    server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "other",
        "-x",
        "100",
        "-y",
        "30",
        "sh",
    ]);
    let other = server.pane("other");
    let (mut bridge, _) = Bridge::start(&server, &[]).await;

    // A pane in a session the bridge is not attached to.
    bridge
        .request(json!({"type": "watch", "pane": other}))
        .await;
    let watch = bridge.last_id();
    let mut term = Term::new();
    let screen = bridge.event(common::WAIT).await.expect("screen");
    assert!(
        matches!(
            screen,
            Message::Screen {
                cols: 100,
                rows: 30,
                ..
            }
        ),
        "{screen:?}"
    );
    term.apply(&screen, watch);
    server.tmux(&["send-keys", "-t", &other, "echo in-other", "Enter"]);
    drain(&mut bridge, &mut term, watch, QUIET).await;
    assert!(
        term.rows().iter().any(|r| r == "in-other"),
        "no output after switching sessions"
    );

    // The desktop resizes the window: a new screen at the new size.
    server.tmux(&["resize-window", "-t", &other, "-x", "90", "-y", "20"]);
    let mut resized = false;
    while let Some(msg) = bridge.event(common::WAIT).await {
        if let Message::Screen {
            cols: 90, rows: 20, ..
        } = msg
        {
            resized = true;
            break;
        }
    }
    assert!(resized, "no screen at the new size");

    server.tmux(&["kill-pane", "-t", &other]);
    let mut gone = false;
    while let Some(msg) = bridge.event(common::WAIT).await {
        if let Message::PaneGone { watch: w, .. } = msg {
            assert_eq!(w, watch);
            gone = true;
            break;
        }
    }
    assert!(gone, "no pane-gone after the pane exited");
}

#[tokio::test]
async fn never_changes_the_layout() {
    let server = Server::start();
    server.tmux(&["split-window", "-h", "-d", "-t", "work", "sh"]);
    server.tmux(&[
        "new-session",
        "-d",
        "-s",
        "other",
        "-x",
        "120",
        "-y",
        "40",
        "sh",
    ]);
    server.tmux(&["split-window", "-v", "-d", "-t", "other", "sh"]);
    let before = server.layout();

    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    for target in ["work", "other", "work.1", "other.1"] {
        let pane = server.pane(target);
        bridge.request(json!({"type": "watch", "pane": pane})).await;
        bridge
            .request(json!({"type": "text", "pane": pane, "text": "true"}))
            .await;
        bridge
            .request(json!({"type": "keys", "pane": pane, "keys": ["Enter"]}))
            .await;
        bridge.request(json!({"type": "list"})).await;
    }
    bridge.request(json!({"type": "bye"})).await;
    let _ = tokio::time::timeout(common::WAIT, bridge.child.wait()).await;
    assert_eq!(server.layout(), before);
}

#[tokio::test]
async fn exits_on_bye_idle_and_closed_input() {
    let server = Server::start();

    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    assert_eq!(
        error_code(&bridge.request(json!({"type": "bye"})).await),
        None
    );
    let status = tokio::time::timeout(common::WAIT, bridge.child.wait())
        .await
        .expect("exit after bye")
        .unwrap();
    assert!(status.success());

    let (mut bridge, _) = Bridge::start(&server, &["--idle-timeout", "1"]).await;
    let Some(Message::Bye { reason }) = bridge.event(common::WAIT).await else {
        panic!("no bye when idle")
    };
    assert_eq!(reason, "idle timeout");
    let status = tokio::time::timeout(common::WAIT, bridge.child.wait())
        .await
        .expect("exit when idle")
        .unwrap();
    assert!(status.success());

    let (bridge, _) = Bridge::start(&server, &[]).await;
    let Bridge {
        mut child, stdin, ..
    } = bridge;
    drop(stdin);
    let status = tokio::time::timeout(common::WAIT, child.wait())
        .await
        .expect("exit on closed input")
        .unwrap();
    assert!(status.success());
    assert!(
        server.tmux(&["list-clients"]).is_empty(),
        "the control client stayed attached"
    );
}
