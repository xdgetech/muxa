//! Test harness: a throwaway tmux server and the real bridge binary, driven
//! over its stdin/stdout like a client would.
#![allow(dead_code)]

use std::collections::VecDeque;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use muxa_protocol::Message;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};

static NEXT: AtomicU32 = AtomicU32::new(0);
pub const WAIT: Duration = Duration::from_secs(5);

/// An isolated tmux server, killed on drop. One session "work", 80x24.
pub struct Server {
    pub socket: String,
}

impl Server {
    pub fn start() -> Self {
        let socket = format!(
            "muxa-test-b{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let server = Server { socket };
        server.tmux(&[
            "-f",
            "/dev/null",
            "new-session",
            "-d",
            "-s",
            "work",
            "-x",
            "80",
            "-y",
            "24",
            "sh",
        ]);
        server.tmux(&["set-option", "-g", "default-shell", "/bin/sh"]);
        server
    }

    pub fn tmux(&self, args: &[&str]) -> String {
        let out = Command::new("tmux")
            .arg("-L")
            .arg(&self.socket)
            .args(args)
            .env_remove("TMUX")
            .output()
            .expect("run tmux");
        assert!(
            out.status.success(),
            "tmux {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
    }

    pub fn pane(&self, target: &str) -> String {
        self.tmux(&["display-message", "-p", "-t", target, "#{pane_id}"])
    }

    /// Every pane's position, size and process, for before/after comparison.
    pub fn layout(&self) -> String {
        self.tmux(&[
            "list-panes",
            "-a",
            "-F",
            "#{window_id} #{window_layout} #{window_active} #{pane_id} #{pane_active} #{pane_left},#{pane_top} #{pane_width}x#{pane_height} #{pane_pid}",
        ])
    }

    pub fn capture(&self, pane: &str) -> Vec<String> {
        trim_rows(
            self.tmux(&["capture-pane", "-p", "-t", pane])
                .lines()
                .map(str::to_owned)
                .collect(),
        )
    }

    pub fn cursor(&self, pane: &str) -> (u16, u16) {
        let s = self.tmux(&[
            "display-message",
            "-p",
            "-t",
            pane,
            "#{cursor_y} #{cursor_x}",
        ]);
        let (y, x) = s.split_once(' ').expect("cursor");
        (y.parse().expect("y"), x.parse().expect("x"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["-L", &self.socket, "kill-server"])
            .output();
    }
}

/// The bridge binary on a test server.
pub struct Bridge {
    pub child: Child,
    pub stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    /// Non-result messages read while waiting for a result.
    events: VecDeque<Message>,
}

impl Bridge {
    pub async fn start(server: &Server, extra: &[&str]) -> (Bridge, Message) {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_muxa-bridge"))
            .args(["--socket", &server.socket])
            .args(extra)
            .env_remove("TMUX")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("start bridge");
        let stdin = child.stdin.take().expect("stdin");
        let lines = BufReader::new(child.stdout.take().expect("stdout")).lines();
        let mut bridge = Bridge {
            child,
            stdin,
            lines,
            next_id: 100,
            events: VecDeque::new(),
        };
        let hello = bridge.read().await.expect("hello");
        (bridge, hello)
    }

    pub async fn send_raw(&mut self, line: &str) {
        self.stdin.write_all(line.as_bytes()).await.expect("write");
        self.stdin.write_all(b"\n").await.expect("write");
        self.stdin.flush().await.expect("flush");
    }

    /// Sends a request (an id is added) and returns its result.
    pub async fn request(&mut self, mut req: Value) -> Message {
        self.next_id += 1;
        let id = self.next_id;
        req["id"] = json!(id);
        self.send_raw(&req.to_string()).await;
        self.result(Some(id)).await
    }

    pub async fn result(&mut self, id: Option<u64>) -> Message {
        loop {
            let msg = self.read().await.expect("bridge closed before the result");
            match &msg {
                Message::Result { id: got, .. } if *got == id => return msg,
                _ => self.events.push_back(msg),
            }
        }
    }

    /// The next non-result message, or `None` after `wait` without one.
    pub async fn event(&mut self, wait: Duration) -> Option<Message> {
        if let Some(msg) = self.events.pop_front() {
            return Some(msg);
        }
        loop {
            let msg = tokio::time::timeout(wait, self.read()).await.ok()??;
            if !matches!(msg, Message::Result { .. }) {
                return Some(msg);
            }
        }
    }

    pub fn last_id(&self) -> u64 {
        self.next_id
    }

    async fn read(&mut self) -> Option<Message> {
        let line = tokio::time::timeout(WAIT, self.lines.next_line())
            .await
            .ok()?
            .ok()??;
        Some(serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad message {line}: {e}")))
    }
}

/// Rows without trailing spaces, and without trailing empty rows.
pub fn trim_rows(rows: Vec<String>) -> Vec<String> {
    let mut rows: Vec<String> = rows.into_iter().map(|r| r.trim_end().to_owned()).collect();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

fn size(n: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(n.max(1)).expect("at least 1")
}

/// A terminal emulator fed the way a client feeds one.
pub struct Term {
    pub parser: vt100::Parser,
    pub watch: u64,
}

impl Term {
    pub fn new() -> Self {
        Term {
            parser: vt100::Parser::new(size(1), size(1), 0),
            watch: 0,
        }
    }

    /// Applies a screen/output message for `watch`; true when it was one.
    pub fn apply(&mut self, msg: &Message, watch: u64) -> bool {
        match msg {
            Message::Screen {
                watch: w,
                cols,
                rows,
                data,
                ..
            } if *w == watch => {
                self.parser = vt100::Parser::new(size(*rows), size(*cols), 0);
                self.parser.process(data);
                self.watch = watch;
                true
            }
            Message::Output { watch: w, data, .. } if *w == watch && self.watch == watch => {
                self.parser.process(data);
                true
            }
            _ => false,
        }
    }

    pub fn rows(&self) -> Vec<String> {
        let screen = self.parser.screen();
        let (_, cols) = screen.size();
        trim_rows(screen.rows(0, cols.get()).collect())
    }

    pub fn cursor(&self) -> (u16, u16) {
        self.parser.screen().cursor_position()
    }
}

/// Feeds every screen/output for `watch` until nothing arrives for `quiet`.
pub async fn drain(
    bridge: &mut Bridge,
    term: &mut Term,
    watch: u64,
    quiet: Duration,
) -> Vec<Message> {
    let mut seen = Vec::new();
    while let Some(msg) = bridge.event(quiet).await {
        if term.apply(&msg, watch) {
            seen.push(msg);
        }
    }
    seen
}

pub async fn wait_until(mut cond: impl FnMut() -> bool) {
    for _ in 0..50 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("condition not met in 5 s");
}
