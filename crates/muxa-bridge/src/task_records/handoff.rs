//! How the goal reaches the agent (ADR 0015): `.muxa/TASK.md` in the task's
//! directory, then one fixed prompt typed once the agent is idle. No user
//! text goes on a command line or into a log.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muxa_tmux::{AgentState, Client, PaneId, Role, SessionId};

use super::{Config, blocking};
use crate::agents::Registry;
use crate::records::{Record, StoreError};

pub const PROMPT: &str = "Read .muxa/TASK.md and start.";
/// How long a new agent gets to report `idle`.
const PROMPT_WAIT: Duration = Duration::from_secs(30);
const PROMPT_POLL: Duration = Duration::from_secs(1);
/// Between the text and Enter, so a TUI does not take both as one paste.
const KEY_GAP: Duration = Duration::from_millis(200);
const LIST_TIMEOUT: Duration = Duration::from_secs(5);
const NOTE_ATTEMPTS: usize = 3;
const DIR: &str = ".muxa";
const FILE: &str = "TASK.md";
/// The pattern added to git's exclude file.
const EXCLUDE: &str = ".muxa/";
const NOT_SENT: &str = "The agent did not become idle within 30 seconds, so the start prompt \
     was not sent. In its pane, type: Read .muxa/TASK.md and start.";

/// The text of `TASK.md`.
pub fn render(r: &Record) -> String {
    let mut out = format!(
        "# {}\n\n## Goal\n\n{}\n\n## Context\n\n",
        r.title,
        r.goal.trim_end()
    );
    out.push_str(&format!("- Task: {}\n- Workspace: {}\n", r.id, r.workspace));
    if let Some(project) = &r.project {
        out.push_str(&format!("- Project: {project}\n"));
    }
    if let Some(repo) = &r.repo {
        out.push_str(&format!("- Repo: {}\n", repo.display()));
    }
    if let Some(branch) = &r.branch {
        out.push_str(&format!("- Branch: {branch}\n"));
    }
    out
}

/// Writes `<dir>/.muxa/TASK.md`. In a repo, `.muxa/` is excluded first, so
/// it never shows as a change. Blocking.
pub fn write(dir: &Path, record: &Record, in_repo: bool) -> Result<PathBuf, String> {
    if in_repo {
        exclude(dir)?;
    }
    let folder = dir.join(DIR);
    std::fs::create_dir_all(&folder)
        .map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
    let path = folder.join(FILE);
    std::fs::write(&path, render(record))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(path)
}

/// Adds `.muxa/` to the repo's `info/exclude`, shared by all its worktrees.
fn exclude(dir: &Path) -> Result<(), String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(format!("{} is not in a git repository", dir.display()));
    }
    let common = String::from_utf8_lossy(&out.stdout);
    let info = Path::new(common.trim_end_matches('\n')).join("info");
    let path = info.join("exclude");
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let Some(addition) = exclude_addition(&existing) else {
        return Ok(());
    };
    std::fs::create_dir_all(&info).map_err(|e| format!("cannot create {}: {e}", info.display()))?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| f.write_all(addition.as_bytes()))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// What to append to an exclude file, or None when it already has the line.
pub fn exclude_addition(existing: &str) -> Option<String> {
    let present = existing
        .lines()
        .map(str::trim)
        .any(|l| l == EXCLUDE || l == "/.muxa/" || l == ".muxa");
    if present {
        return None;
    }
    let lead = if existing.is_empty() || existing.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    Some(format!("{lead}{EXCLUDE}\n"))
}

/// Types [`PROMPT`] into the session's agent pane once its adapter reports
/// `idle`. Not on `waiting`: that includes a permission menu, where the
/// text would pick an option. After [`PROMPT_WAIT`], the record's note says
/// the prompt was not sent.
pub async fn send_prompt(
    client: Arc<Client>,
    agents: Arc<Registry>,
    config: Arc<Config>,
    task: String,
    session: SessionId,
) {
    let deadline = tokio::time::Instant::now() + PROMPT_WAIT;
    loop {
        if let Some((pane, Some(AgentState::Idle))) = agent_pane(&client, &agents, session).await {
            match type_prompt(&client, pane).await {
                Ok(()) => tracing::info!(task, %pane, "start prompt sent"),
                Err(e) => tracing::warn!(task, %pane, error = %e, "cannot send the start prompt"),
            }
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(PROMPT_POLL).await;
    }
    tracing::info!(task, %session, "agent not idle; start prompt not sent");
    note(&config, task, NOT_SENT).await;
}

async fn type_prompt(client: &Client, pane: PaneId) -> Result<(), muxa_tmux::Error> {
    muxa_tmux::send_text(client, pane, PROMPT).await?;
    tokio::time::sleep(KEY_GAP).await;
    muxa_tmux::send_keys(client, pane, &["Enter".to_owned()]).await
}

/// The session's agent pane and the state its tags or adapter give.
async fn agent_pane(
    client: &Client,
    agents: &Arc<Registry>,
    session: SessionId,
) -> Option<(PaneId, Option<AgentState>)> {
    let listed = tokio::time::timeout(LIST_TIMEOUT, muxa_tmux::list_sessions(client))
        .await
        .ok()?
        .ok()?;
    let mut mine: Vec<_> = listed.into_iter().filter(|s| s.id == session).collect();
    if mine.is_empty() {
        return None;
    }
    let agents = Arc::clone(agents);
    let mine = tokio::task::spawn_blocking(move || {
        crate::sessions::fill_agents(&mut mine, &agents);
        mine
    })
    .await
    .ok()?;
    mine.into_iter()
        .flat_map(|s| s.panes)
        .find(|p| p.role == Role::Agent && !p.dead)
        .map(|p| (p.id, p.agent.map(|a| a.state)))
}

/// Sets the record's note, retrying when another bridge wrote in between.
pub async fn note(config: &Config, task: String, text: &'static str) {
    let config = config.clone();
    let done = blocking(move || {
        let store = config.store()?;
        for _ in 0..NOTE_ATTEMPTS {
            let rev = store.get(&task).map_err(|e| super::store_error(&e))?.rev;
            match store.update(&task, rev, |r| r.note = Some(text.to_owned())) {
                Ok(_) => return Ok(()),
                Err(StoreError::Conflict { .. }) => continue,
                Err(e) => return Err(super::store_error(&e)),
            }
        }
        Err(super::store_error(&StoreError::Busy(task)))
    })
    .await;
    if let Err(e) = done {
        tracing::warn!(error = %e.message, "cannot set a task note");
    }
}
