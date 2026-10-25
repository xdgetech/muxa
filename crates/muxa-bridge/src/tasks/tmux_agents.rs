//! Starts tasks with tmux-agents' `new.sh`, so the task gets its worktree,
//! layout and sidebar exactly as from the desktop.

use std::path::PathBuf;

use muxa_tmux::Client;
use tokio::process::Command;

use super::{LAUNCH_TIMEOUT, Launch, TaskError, TaskLauncher, TaskRequest, last_line, run};

pub struct TmuxAgents {
    script: PathBuf,
    tmux: PathBuf,
    socket: Option<String>,
}

impl TmuxAgents {
    pub fn new(script: PathBuf, tmux: PathBuf, socket: Option<String>) -> Self {
        Self {
            script,
            tmux,
            socket,
        }
    }

    pub(crate) fn command(&self, req: &TaskRequest) -> Command {
        let mut cmd = Command::new(&self.script);
        cmd.arg(&req.repo).arg(&req.branch);
        if let Some(base) = &req.base {
            cmd.arg("--base").arg(base);
        }
        // Without it, the desktop client would jump to the new session.
        cmd.arg("--no-switch");
        // The script calls `tmux`; over SSH, PATH often lacks the directory of
        // the tmux the bridge found.
        if let Some(dir) = self.tmux.parent() {
            let path = std::env::var_os("PATH").unwrap_or_default();
            let joined = std::env::join_paths(
                std::iter::once(dir.to_path_buf()).chain(std::env::split_paths(&path)),
            );
            if let Ok(joined) = joined {
                cmd.env("PATH", joined);
            }
        }
        if let Some(socket) = &self.socket {
            cmd.env("TMUX_AGENTS_SOCKET", socket);
        }
        cmd.env_remove("TMUX");
        cmd
    }
}

impl TaskLauncher for TmuxAgents {
    fn id(&self) -> &'static str {
        "tmux-agents"
    }

    fn launch<'a>(&'a self, _client: &'a Client, req: &'a TaskRequest) -> Launch<'a> {
        Box::pin(async move {
            let out = run(&mut self.command(req), LAUNCH_TIMEOUT, "tmux-agents").await?;
            if !out.status.success() {
                // new.sh reports to the tmux client first, so stderr can be empty.
                let why = last_line(&out.stderr)
                    .map(|l| l.trim_start_matches("agents: ").to_owned())
                    .unwrap_or_else(|| "tmux-agents could not start the task; see its log".into());
                tracing::warn!(status = %out.status, stderr = out.stderr.len(), "tmux-agents new.sh failed");
                return Err(TaskError::Failed(why));
            }
            last_line(&out.stdout)
                .ok_or_else(|| TaskError::Failed("tmux-agents printed no session name".into()))
        })
    }
}
