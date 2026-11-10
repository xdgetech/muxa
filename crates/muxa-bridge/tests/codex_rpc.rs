//! `muxa-bridge codex-rpc`, the developer's way to run ADR 0028's checklist
//! through muxa's own App Server client, against `data/codex-fake.py`.
use std::path::PathBuf;
use std::process::{Command, Output};

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

    fn rpc(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_muxa-bridge"))
            .args(["codex-rpc", "--codex", FAKE, "--codex-home"])
            .arg(&self.0)
            .args(args)
            .env_remove("CODEX_HOME")
            .output()
            .unwrap()
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
