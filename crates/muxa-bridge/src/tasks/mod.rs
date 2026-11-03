//! Task launchers: start a task, a worktree on a branch and a session with an
//! agent in it, for a client. tmux-agents when it is installed, the built-in
//! launcher otherwise. See `docs/agents.md` and ADR 0007.

pub mod ask;
pub mod branch;
pub mod builtin;
pub mod tmux_agents;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Output;
use std::time::Duration;

use muxa_protocol::CloseMode;
use muxa_tmux::{Client, SessionId};
use tokio::process::Command;

use crate::records::AgentSettings;

/// A whole launch, worktree and session, at most.
pub const LAUNCH_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_REF: usize = 200;
const MAX_MESSAGE: usize = 300;

/// A checked `new-task` request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskRequest {
    /// Absolute; inside a git repo.
    pub repo: PathBuf,
    pub branch: String,
    pub base: Option<String>,
    /// The task record the session runs, when a launcher can tag it at creation.
    pub record: Option<String>,
    /// The workspace's agent command and environment (ADR 0016).
    pub agent: Option<AgentSettings>,
}

#[derive(Debug, thiserror::Error)]
pub enum TaskError {
    /// The request is wrong; the message says what to change.
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Failed(String),
    #[error(transparent)]
    Tmux(#[from] muxa_tmux::Error),
}

pub type Launch<'a> = Pin<Box<dyn Future<Output = Result<Launched, TaskError>> + Send + 'a>>;
pub type Closing<'a> = Pin<Box<dyn Future<Output = Result<(), TaskError>> + Send + 'a>>;

/// The session a launch made or found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    pub id: SessionId,
    pub name: String,
    /// Where the agent works: the worktree, or a task directory.
    pub dir: PathBuf,
}

/// A task's session, as its tags describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskSession {
    pub id: SessionId,
    pub name: String,
    pub worktree: PathBuf,
    /// The main checkout; None for sessions tagged before it was recorded.
    pub repo_dir: Option<PathBuf>,
}

/// A way to start tasks. One instance serves every request.
pub trait TaskLauncher: Send + Sync {
    /// Stable, for logs: `tmux-agents`.
    fn id(&self) -> &'static str;
    /// Starts the task, or finds the one already running for the branch, and
    /// returns its session. It must never move an attached client.
    fn launch<'a>(&'a self, client: &'a Client, req: &'a TaskRequest) -> Launch<'a>;
    /// Closes the task's session, and removes its worktree as `mode` allows.
    /// A worktree with uncommitted changes is never removed; the branch is kept.
    fn close<'a>(
        &'a self,
        client: &'a Client,
        task: &'a TaskSession,
        mode: CloseMode,
    ) -> Closing<'a>;
}

/// The task session with this id; only a session with a worktree is a task.
pub async fn find_task(client: &Client, id: &str) -> Result<TaskSession, TaskError> {
    let id: SessionId = id
        .parse()
        .map_err(|_| TaskError::Invalid(format!("{id:?} is not a session id")))?;
    let sessions = muxa_tmux::list_sessions(client).await?;
    let s = sessions
        .into_iter()
        .find(|s| s.id == id)
        .ok_or_else(|| TaskError::Invalid(format!("session {id} does not exist")))?;
    let worktree = s.worktree.ok_or_else(|| {
        TaskError::Invalid(format!("{} is not a task: it has no worktree", s.name))
    })?;
    Ok(TaskSession {
        id,
        name: s.name,
        worktree: worktree.into(),
        repo_dir: s.repo_dir.map(PathBuf::from),
    })
}

/// The session whose `@agent_worktree` is `worktree`. Never by name (invariant 3).
pub async fn session_for_worktree(
    client: &Client,
    worktree: &Path,
) -> Result<Option<muxa_tmux::Session>, TaskError> {
    let sessions = muxa_tmux::list_sessions(client).await?;
    Ok(sessions.into_iter().find(|s| {
        s.worktree
            .as_deref()
            .is_some_and(|w| same_dir(Path::new(w), worktree))
    }))
}

/// Two spellings of one directory, e.g. through a symlinked /tmp.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// How the bridge starts tasks, from its flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// `--task-launcher`: forces a launcher; None picks one per request.
    pub launcher: Option<Choice>,
    /// The agent the built-in launcher starts.
    pub command: String,
    pub tmux: PathBuf,
    pub socket: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    Builtin,
    /// tmux-agents' `new.sh`.
    TmuxAgents(PathBuf),
}

/// The tmux option where tmux-agents publishes its scripts directory.
const AGENTS_SCRIPTS: &str = "@agents_scripts";
/// What this tmux-agents' scripts accept beyond the first contract (ADR 0017).
const AGENTS_FEATURES: &str = "@agents_features";

/// The launcher for one request. Asked each time, so installing tmux-agents
/// needs no bridge restart.
pub async fn pick(config: &Config, client: &Client) -> Box<dyn TaskLauncher> {
    let choice = match &config.launcher {
        Some(c) => c.clone(),
        None => match muxa_tmux::global_option(client, AGENTS_SCRIPTS).await {
            Ok(Some(dir)) => {
                let script = Path::new(&dir).join("new.sh");
                if script.is_file() {
                    Choice::TmuxAgents(script)
                } else {
                    tracing::warn!(
                        dir,
                        "{AGENTS_SCRIPTS} has no new.sh; using the built-in launcher"
                    );
                    Choice::Builtin
                }
            }
            Ok(None) => Choice::Builtin,
            Err(e) => {
                tracing::warn!(error = %e, "cannot read {AGENTS_SCRIPTS}");
                Choice::Builtin
            }
        },
    };
    match choice {
        Choice::Builtin => Box::new(builtin::Builtin::new(config.command.clone())),
        Choice::TmuxAgents(script) => Box::new(tmux_agents::TmuxAgents::new(
            script,
            config.tmux.clone(),
            config.socket.clone(),
            tags_at_creation(client).await,
        )),
    }
}

/// Whether tmux-agents' `new.sh` takes `--muxa-task`. Older ones reject
/// unknown flags, so they are only given it when they say so.
async fn tags_at_creation(client: &Client) -> bool {
    match muxa_tmux::global_option(client, AGENTS_FEATURES).await {
        Ok(features) => features.is_some_and(|f| has_feature(&f, "muxa-task")),
        Err(e) => {
            tracing::warn!(error = %e, "cannot read {AGENTS_FEATURES}");
            false
        }
    }
}

/// `@agents_features` is a space-separated list.
pub fn has_feature(features: &str, name: &str) -> bool {
    features.split_whitespace().any(|f| f == name)
}

/// The launcher for `req`. A workspace's agent environment needs the built-in
/// launcher: tmux-agents cannot pass it to its agent yet (ADR 0016).
pub async fn pick_for(
    config: &Config,
    client: &Client,
    req: &TaskRequest,
) -> Box<dyn TaskLauncher> {
    if req.agent.is_some() {
        return Box::new(builtin::Builtin::new(config.command.clone()));
    }
    pick(config, client).await
}

/// Checks a `new-task` request at the boundary.
pub async fn validate(
    repo: &str,
    branch: &str,
    base: Option<&str>,
) -> Result<TaskRequest, TaskError> {
    let repo = PathBuf::from(repo);
    if !repo.is_absolute() || !repo.is_dir() {
        return Err(TaskError::Invalid(format!(
            "{} is not a directory; give the repo's absolute path",
            repo.display()
        )));
    }
    check_ref(&repo, branch, "branch").await?;
    if let Some(base) = base {
        check_ref(&repo, base, "base").await?;
    }
    let inside = git(&repo)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .await
        .is_ok_and(|o| o.status.success());
    if !inside {
        return Err(TaskError::Invalid(format!(
            "{} is not in a git repository",
            repo.display()
        )));
    }
    Ok(TaskRequest {
        repo,
        branch: branch.to_owned(),
        base: base.map(str::to_owned),
        record: None,
        agent: None,
    })
}

/// A name git accepts for a branch, and no option in disguise.
async fn check_ref(repo: &Path, name: &str, what: &str) -> Result<(), TaskError> {
    let invalid = || TaskError::Invalid(format!("{name:?} is not a valid {what} name"));
    if name.is_empty() || name.len() > MAX_REF || name.starts_with('-') {
        return Err(invalid());
    }
    let ok = git(repo)
        .args(["check-ref-format", "--branch", name])
        .output()
        .await
        .is_ok_and(|o| o.status.success());
    if ok { Ok(()) } else { Err(invalid()) }
}

/// tmux-agents' rule, so both launchers give a branch the same session:
/// no '.', ':', '/' or space.
pub fn session_name(branch: &str) -> String {
    branch
        .chars()
        .map(|c| {
            if matches!(c, '.' | ':' | '/' | ' ') {
                '-'
            } else {
                c
            }
        })
        .collect()
}

/// git in `dir`, never prompting: the bridge has no terminal to answer on.
pub(crate) fn git(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    if std::env::var_os("GIT_SSH_COMMAND").is_none() {
        cmd.env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes");
    }
    cmd
}

/// Runs `cmd` to the end, or kills it after `timeout`.
pub(crate) async fn run(
    cmd: &mut Command,
    timeout: Duration,
    what: &str,
) -> Result<Output, TaskError> {
    cmd.stdin(std::process::Stdio::null()).kill_on_drop(true);
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => Err(TaskError::Failed(format!("cannot run {what}: {e}"))),
        Err(_) => Err(TaskError::Failed(format!(
            "{what} did not finish in {} seconds",
            timeout.as_secs()
        ))),
    }
}

/// The last non-empty line of a command's output, shortened for a message.
pub(crate) fn last_line(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    let line = text.lines().map(str::trim).rfind(|l| !l.is_empty())?;
    Some(line.chars().take(MAX_MESSAGE).collect())
}

#[cfg(test)]
mod tests;
