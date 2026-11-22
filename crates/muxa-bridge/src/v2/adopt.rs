//! Adopting a place that already exists (ADR 0076): a folder or git worktree
//! in a Workspace, as an older muxa left it. Nothing is made or removed here.
//! A worktree keeps its branch, so closing the Task follows ADR 0043.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use muxa_server::TaskError;

use super::git::{self, GitError};
use crate::agents::{self, Processes};
use crate::records::Workspace;

/// A checked existing place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adopted {
    /// Canonical.
    pub path: PathBuf,
    /// The main checkout, when `path` is a linked worktree on a branch.
    pub main: Option<PathBuf>,
    pub branch: Option<String>,
    /// `path` is in a git repo: `.muxa/` goes into its exclude.
    pub in_repo: bool,
}

/// Finds a coding agent that runs in a folder: its program and pid.
pub type AgentLookup = dyn Fn(&Path) -> Result<Option<(String, u32)>, String> + Send + Sync;

/// The host's lookup: `ps`, then each agent's working directory.
pub fn host_lookup() -> Arc<AgentLookup> {
    Arc::new(|dir: &Path| {
        let procs = Processes::list().map_err(|e| e.to_string())?;
        agents::agent_in(dir, &procs, &agents::process_cwds).map_err(|e| e.to_string())
    })
}

/// Resolves `raw` with its symlinks. It must be a folder inside the
/// Workspace's root, or a linked worktree of one of its repos.
pub async fn resolve(ws: &Workspace, raw: &str) -> Result<Adopted, TaskError> {
    let given = Path::new(raw);
    if !given.is_absolute() {
        return Err(TaskError::Invalid(format!(
            "{raw} is not an absolute path; give the folder's full path"
        )));
    }
    let path = given
        .canonicalize()
        .map_err(|_| TaskError::Invalid(format!("{raw} does not exist")))?;
    if !path.is_dir() {
        return Err(TaskError::Invalid(format!("{raw} is not a folder")));
    }
    let main = match git::main_checkout(&path).await {
        Ok(main) => Some(main.canonicalize().unwrap_or(main)),
        Err(GitError::Invalid(_)) => None,
        Err(e) => return Err(TaskError::Failed(e.to_string())),
    };
    let top = match &main {
        Some(_) => git::toplevel(&path)
            .await
            .map_err(|e| TaskError::Failed(e.to_string()))?
            .map(|t| t.canonicalize().unwrap_or(t)),
        None => None,
    };
    // A linked worktree's own top folder, not the main checkout.
    let worktree = top.as_ref() == Some(&path) && main.as_ref() != Some(&path);
    let listed = worktree && main.as_ref().is_some_and(|m| ws.has_repo(m));
    if !path.starts_with(&ws.root) && !listed {
        return Err(TaskError::Invalid(format!(
            "{} is outside Workspace {}: give a folder in {} or a worktree of one of its repos",
            path.display(),
            ws.name,
            ws.root.display()
        )));
    }
    let branch = if worktree {
        git::current_branch(&path).await
    } else {
        None
    };
    Ok(Adopted {
        in_repo: main.is_some(),
        main: branch.as_ref().and(main),
        branch,
        path,
    })
}

/// Refuses when a coding agent already works in `dir`: two agents never
/// share a folder.
pub async fn refuse_if_busy(lookup: Arc<AgentLookup>, dir: &Path) -> Result<(), TaskError> {
    let owned = dir.to_owned();
    let found = tokio::task::spawn_blocking(move || lookup(&owned))
        .await
        .map_err(|e| TaskError::Failed(e.to_string()))?
        .map_err(|e| TaskError::Failed(format!("cannot check for a running agent: {e}")))?;
    match found {
        None => Ok(()),
        Some((program, pid)) => Err(TaskError::Busy(format!(
            "{program} (pid {pid}) already works in {}. Stop it there first, then resume the Task.",
            dir.display()
        ))),
    }
}

/// The Claude Code config dirs that hold `ws`'s conversations: its
/// `CLAUDE_CONFIG_DIR`, else the host's.
pub fn claude_dirs(ws: &Workspace, host: &[PathBuf]) -> Vec<PathBuf> {
    match ws
        .agent
        .as_ref()
        .and_then(|a| a.env(agents::claude::DIR_ENV))
    {
        Some(dir) => vec![PathBuf::from(dir)],
        None => host.to_vec(),
    }
}

#[cfg(test)]
#[path = "adopt_tests.rs"]
pub(crate) mod tests;
