//! A git repo and a stand-in for tmux-agents' `new.sh`, for the task tests.

use std::path::{Path, PathBuf};
use std::process::Command;

/// `<tmp>/repos/app` with one commit, removed on drop with its worktrees.
pub struct Repo {
    pub root: PathBuf,
    pub app: PathBuf,
}

impl Repo {
    pub fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("muxa-tasks-it-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = root.join("repos/app");
        std::fs::create_dir_all(&app).unwrap();
        let app = app.canonicalize().unwrap();
        git(&app, &["init", "-q", "-b", "main"]);
        std::fs::write(app.join("README.md"), "# app\n").unwrap();
        git(&app, &["add", "."]);
        git(
            &app,
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "-m",
                "init",
            ],
        );
        Self {
            root: root.canonicalize().unwrap(),
            app,
        }
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `new.sh` stand-in: records its arguments and socket, then does what
/// tmux-agents does that muxa relies on: a worktree for the branch at
/// `<repos>/.worktrees/<repo>-<branch>`, a session `fake-task` there tagged
/// `@agent_worktree`, with its pane tagged as the agent (`@agents_role ai`).
/// With `fail`, it fails like tmux-agents' `die`.
pub fn fake_new_sh(dir: &Path, fail: bool) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let script = dir.join("new.sh");
    let log = dir.join("args");
    let body = if fail {
        "echo 'agents: could not create a worktree for feat/x: boom' >&2\nexit 1\n"
    } else {
        r#"common="$(git -C "$1" rev-parse --path-format=absolute --git-common-dir)"
main="${common%/.git}"
wt="$(dirname "$main")/.worktrees/$(basename "$main")-$(printf '%s' "$2" | tr / -)"
[ -d "$wt" ] || git -C "$main" worktree add -q -b "$2" "$wt" >/dev/null 2>&1 || exit 1
t() { tmux -L "$TMUX_AGENTS_SOCKET" "$@"; }
if ! t has-session -t =fake-task 2>/dev/null; then
  t new-session -d -s fake-task -c "$wt" 'sleep 600'
  t set-option -t =fake-task: @agent_worktree "$wt"
  t set-option -p -t =fake-task: @agents_role ai
fi
echo fake-task
"#
    };
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" \"socket=$TMUX_AGENTS_SOCKET\" > '{}'\n{body}",
            log.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}
