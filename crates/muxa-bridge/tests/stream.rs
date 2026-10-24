//! The snapshot/stream cut under load: watch a pane at a random moment while
//! a generator writes to it, then compare the emulated screen with tmux's.
//! MUXA_STREAM_RUNS sets the number of runs (default 50).
//! MUXA_RECORD_SCENARIOS=1 rewrites protocol/fixtures/scenarios/.
#![cfg(feature = "tmux-integration")]

mod common;

use std::time::{Duration, Instant};

use common::{Bridge, Server, Term};
use muxa_protocol::Message;
use rand::RngExt;
use serde_json::json;

/// Colors, absolute cursor moves, wide (CJK) characters, erase-line, a
/// scroll region, carriage returns and the alternate screen, in a loop.
/// The region is reset before each screen switch, as well-behaved TUIs do:
/// tmux keeps one region per terminal, vt100 (and SwiftTerm) one per screen.
/// A region set is followed by an explicit cursor move: vt100 homes the
/// cursor to the region's top, tmux to the screen's (docs/findings).
const GENERATOR: &str = r#"i=0
while :; do
  i=$((i+1)); k=$((i % 140))
  case $((i % 6)) in
    0) printf '\033[%d;%dH\033[3%dmcup%d\033[0m' $((i % 20 + 1)) $((i % 60 + 1)) $((i % 7)) $i ;;
    1) printf '\033[1mbold %d 日本語 ✓ é\033[0m\n' $i ;;
    2) printf 'line %d tail\033[K\n' $i ;;
    3) printf '\033[4%dm bg \033[0m %d\r' $((i % 7)) $i ;;
    4) [ $k -eq 4 ] && printf '\033[r\033[?1049h\033[2J\033[Halt %d\n' $i
       [ $k -eq 30 ] && printf '\033[3;18r\033[H'
       [ $k -eq 100 ] && printf '\033[r' ;;
    5) [ $k -eq 75 ] && printf '\033[r\033[?1049l' ;;
  esac
done
"#;

const QUIET: Duration = Duration::from_millis(400);

#[tokio::test(flavor = "multi_thread")]
async fn screen_plus_stream_matches_tmux() {
    let runs: usize = std::env::var("MUXA_STREAM_RUNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let record = std::env::var("MUXA_RECORD_SCENARIOS").is_ok();
    let server = Server::start();
    let pane = server.pane("work");
    let script = std::env::temp_dir().join(format!("muxa-gen-{}.sh", server.socket));
    std::fs::write(&script, GENERATOR).unwrap();
    let (mut bridge, _) = Bridge::start(&server, &[]).await;
    let mut rng = rand::rng();

    for run in 0..runs {
        let start = format!(
            "printf '\\033[?1049l\\033[r\\033[0m'; clear; sh {}",
            script.display()
        );
        server.tmux(&["send-keys", "-t", &pane, &start, "Enter"]);
        tokio::time::sleep(Duration::from_millis(rng.random_range(0..150))).await;

        bridge.request(json!({"type": "watch", "pane": pane})).await;
        let watch = bridge.last_id();
        let mut term = Term::new();
        let mut recorded = Vec::new();
        // Recorded scenarios stream briefly, to keep the fixtures small.
        let window = if record { 5..15 } else { 50..300 };
        let until = Instant::now() + Duration::from_millis(rng.random_range(window));
        while Instant::now() < until {
            if let Some(msg) = bridge.event(Duration::from_millis(50)).await
                && term.apply(&msg, watch)
            {
                recorded.push(msg);
            }
        }
        server.tmux(&["send-keys", "-t", &pane, "C-c"]);
        recorded.extend(common::drain(&mut bridge, &mut term, watch, QUIET).await);

        let (want, cursor) = (server.capture(&pane), server.cursor(&pane));
        assert!(
            term.rows() == want && term.cursor() == cursor,
            "run {run}: screen differs\n--- emulated (cursor {:?})\n{}\n--- tmux (cursor {cursor:?})\n{}",
            term.cursor(),
            term.rows().join("\n"),
            want.join("\n"),
        );
        if record && run < 2 {
            write_scenario(&format!("stream-{run}"), &recorded, &want, cursor);
        }
    }
    let _ = std::fs::remove_file(script);
}

fn write_scenario(name: &str, messages: &[Message], rows: &[String], cursor: (u16, u16)) {
    let dir = format!(
        "{}/../../protocol/fixtures/scenarios",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::create_dir_all(&dir).unwrap();
    let mut text =
        json!({"scenario": name, "expectRows": rows, "expectCursor": [cursor.0, cursor.1]})
            .to_string();
    for m in messages {
        text.push('\n');
        text.push_str(&serde_json::to_string(m).unwrap());
    }
    text.push('\n');
    std::fs::write(format!("{dir}/{name}.jsonl"), text).unwrap();
}
