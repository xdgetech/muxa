//! Starts tasks with tmux-agents' `new.sh`, so the task gets its worktree,
//! layout and sidebar exactly as from the desktop.

use std::path::PathBuf;

use muxa_tmux::Client;
use tokio::process::Command;

use muxa_protocol::CloseMode;

use super::{
    Closing, LAUNCH_TIMEOUT, Launch, TaskError, TaskLauncher, TaskRequest, TaskSession, last_line,
    run,
};

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
        let mut cmd = self.script("new.sh");
        cmd.arg(&req.repo).arg(&req.branch);
        if let Some(base) = &req.base {
            cmd.arg("--base").arg(base);
        }
        // Without it, the desktop client would jump to the new session.
        cmd.arg("--no-switch");
        cmd
    }

    pub(crate) fn close_command(&self, task: &TaskSession, mode: CloseMode) -> Command {
        let mut cmd = self.script("close.sh");
        match mode {
            CloseMode::Close => {}
            CloseMode::Keep => {
                cmd.arg("--keep");
            }
            CloseMode::Force => {
                cmd.arg("--force");
            }
        }
        cmd.arg(&task.name);
        cmd
    }

    /// A script beside `new.sh`, with the environment tmux-agents expects.
    fn script(&self, name: &str) -> Command {
        let path = self.script.with_file_name(name);
        let mut cmd = Command::new(path);
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

    async fn run_script(&self, mut cmd: Command, what: &str) -> Result<Vec<u8>, TaskError> {
        let out = run(&mut cmd, LAUNCH_TIMEOUT, "tmux-agents").await?;
        if out.status.success() {
            return Ok(out.stdout);
        }
        // tmux-agents reports to the tmux client first, so stderr can be empty.
        let why = last_line(&out.stderr)
            .map(|l| l.trim_start_matches("agents: ").to_owned())
            .unwrap_or_else(|| format!("tmux-agents could not {what}; see its log"));
        tracing::warn!(status = %out.status, stderr = out.stderr.len(), "tmux-agents {what} failed");
        Err(TaskError::Failed(why))
    }
}

impl TaskLauncher for TmuxAgents {
    fn id(&self) -> &'static str {
        "tmux-agents"
    }

    fn launch<'a>(&'a self, _client: &'a Client, req: &'a TaskRequest) -> Launch<'a> {
        Box::pin(async move {
            let out = self.run_script(self.command(req), "start the task").await?;
            last_line(&out)
                .ok_or_else(|| TaskError::Failed("tmux-agents printed no session name".into()))
        })
    }

    fn close<'a>(
        &'a self,
        _client: &'a Client,
        task: &'a TaskSession,
        mode: CloseMode,
    ) -> Closing<'a> {
        Box::pin(async move {
            self.run_script(self.close_command(task, mode), "close the task")
                .await
                .map(drop)
        })
    }
}
