//! A Codex data directory and a stand-in `codex` program for the bridge tests.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use super::Server;

const SAMPLE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/codex/rollout.jsonl"
);

/// The program is a copy of `sleep`, so that `ps` and tmux both see the name `codex`.
pub struct CodexDir(pub PathBuf);

impl CodexDir {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("muxa-codex-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        let program = dir.join("bin/codex");
        std::fs::copy(which("sleep"), &program).unwrap();
        // macOS kills a copied system binary unless it is signed again.
        if cfg!(target_os = "macos") {
            let signed = Command::new("codesign")
                .args(["-s", "-", "-f"])
                .arg(&program)
                .output()
                .unwrap();
            assert!(signed.status.success(), "codesign failed");
        }
        Self(dir)
    }

    /// Runs the program in a new pane made by `create` (`new-window -d`, ...) and
    /// returns the pane id and its path.
    pub fn start(&self, server: &Server, create: &[&str]) -> (String, String) {
        let command = format!("{} 600", self.0.join("bin/codex").display());
        let mut args = create.to_vec();
        args.extend(["-P", "-F", "#{pane_id}", &command]);
        let pane = server.tmux(&args).trim().to_owned();
        // tmux reads the command and path from the pane's process once it runs.
        std::thread::sleep(Duration::from_millis(300));
        let found = server.tmux(&[
            "display-message",
            "-p",
            "-t",
            &pane,
            "#{pane_current_command} #{pane_current_path}",
        ]);
        let (command, cwd) = found.trim().split_once(' ').unwrap();
        // A multi-call coreutils `sleep` would not run under another name.
        assert_eq!(command, "codex", "the stand-in does not run as codex");
        (pane, cwd.to_owned())
    }

    /// The sample rollout for `cwd`, in today's (UTC) day directory.
    pub fn rollout(&self, cwd: &str) -> PathBuf {
        let day = Command::new("date")
            .args(["-u", "+%Y/%m/%d"])
            .output()
            .unwrap();
        let day = self
            .0
            .join("sessions")
            .join(String::from_utf8(day.stdout).unwrap().trim());
        std::fs::create_dir_all(&day).unwrap();
        let sample = std::fs::read_to_string(SAMPLE).unwrap();
        let mut lines: Vec<String> = sample.lines().map(str::to_owned).collect();
        let mut meta: Value = serde_json::from_str(&lines[0]).unwrap();
        meta["payload"]["cwd"] = cwd.into();
        lines[0] = meta.to_string();
        let path = day.join("rollout-test.jsonl");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    pub fn append(path: &Path, line: &str) {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        writeln!(f, "{line}").unwrap();
    }

    pub fn arg(&self) -> String {
        format!("codex={}", self.0.display())
    }
}

impl Drop for CodexDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn which(program: &str) -> PathBuf {
    let out = Command::new("sh")
        .args(["-c", &format!("command -v {program}")])
        .output()
        .unwrap();
    PathBuf::from(String::from_utf8(out.stdout).unwrap().trim())
}
