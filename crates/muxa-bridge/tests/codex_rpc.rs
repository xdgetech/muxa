//! `muxa-bridge codex-rpc`, the developer's way to run ADR 0028's checklist
//! through muxa's own App Server client, against `data/codex-fake.py`.
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use serde_json::Value;

const FAKE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/codex-fake.py");

struct Home(PathBuf);

impl Home {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("muxa-codex-rpc-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("fake")).unwrap();
        Self(dir)
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_muxa-bridge"));
        cmd.args(["codex-rpc", "--codex", FAKE, "--codex-home"])
            .arg(&self.0)
            .args(args)
            .env_remove("CODEX_HOME");
        cmd
    }

    fn rpc(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_thread_starts_and_reads_back_through_the_subcommand() {
    let home = Home::new("start");
    let cwd = home.0.display().to_string();
    let params = format!(r#"{{"cwd":"{cwd}","historyMode":"legacy"}}"#);
    let out = home.rpc(&["thread/start", &params]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let started: Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = started["thread"]["id"].as_str().unwrap();
    assert_eq!(started["thread"]["status"]["type"], "idle");
    assert_eq!(started["thread"]["historyMode"], "legacy");

    let out = home.rpc(&["thread/read", &format!(r#"{{"threadId":"{id}"}}"#)]);
    assert!(out.status.success());
    let read: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(read["thread"]["id"], id);
}

#[test]
fn an_error_answer_and_bad_params_fail() {
    let home = Home::new("error");
    let out = home.rpc(&["thread/read", r#"{"threadId":"nope"}"#]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("error answer"));

    let out = home.rpc(&["thread/read", "[1]"]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn a_held_connection_keeps_an_empty_thread_and_prints_only_ids() {
    let home = Home::new("hold");
    // The fake drops a thread nothing holds this long after it was last held.
    std::fs::write(home.0.join("fake/unload-after"), "0.3").unwrap();
    let cwd = home.0.display().to_string();
    let params = format!(r#"{{"cwd":"{cwd}","historyMode":"legacy"}}"#);
    let mut held = home
        .command(&["--hold", "2", "thread/start", &params])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The result is printed and flushed before the hold begins.
    let mut result = String::new();
    let mut lines = BufReader::new(held.stdout.take().unwrap()).lines();
    for line in lines.by_ref() {
        let line = line.unwrap();
        result.push_str(&line);
        if line == "}" {
            break;
        }
    }
    let started: Value = serde_json::from_str(&result).unwrap();
    let id = started["thread"]["id"].as_str().unwrap().to_owned();
    let read = format!(r#"{{"threadId":"{id}"}}"#);

    std::thread::sleep(Duration::from_millis(800));
    let out = home.rpc(&["thread/read", &read]);
    assert!(out.status.success(), "held: still loaded");

    assert!(held.wait().unwrap().success());
    let mut events = String::new();
    held.stderr
        .take()
        .unwrap()
        .read_to_string(&mut events)
        .unwrap();
    assert!(
        events.contains(&format!("event thread/started thread={id} status=idle")),
        "{events}"
    );
    assert!(events.contains("held 2s"), "{events}");
    assert!(
        !events.contains(&cwd),
        "an event's other fields stay out: {events}"
    );

    std::thread::sleep(Duration::from_millis(800));
    let out = home.rpc(&["thread/read", &read]);
    assert_eq!(out.status.code(), Some(1), "released: gone");
}
