//! The sweep of ADR 0043 §6: a listed worktree that no open Task, no tmux pane
//! and no process holds goes when nothing can be lost. Others wait for the
//! user, and are logged. The daemon runs it at start and then now and then.

use std::path::{Path, PathBuf};

use crate::records::{Record, Status};
use crate::tasks::{builtin, same_dir, worktrees};

use super::Context;

/// A worktree that nothing holds but that could lose work, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub worktree: PathBuf,
    pub why: &'static str,
}

/// One sweep. Without tmux it does nothing: a session may hold any worktree.
pub(crate) async fn sweep(cx: &Context) -> Vec<Pending> {
    let config = std::sync::Arc::clone(&cx.records);
    let (listed, records) = tokio::task::spawn_blocking(move || {
        seed_once(&config.records_if_any());
        (worktrees::listed(), config.records_if_any())
    })
    .await
    .unwrap_or_default();
    let listed: Vec<PathBuf> = listed.into_iter().filter(|w| still_there(w)).collect();
    if listed.is_empty() {
        return Vec::new();
    }
    let env = match cx.env().await {
        Ok(env) => env,
        Err(e) => {
            tracing::debug!(error = %e.message, "no tmux for the worktree sweep");
            return Vec::new();
        }
    };
    let sessions = match muxa_tmux::list_sessions(&env.client).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot list sessions for the worktree sweep");
            return Vec::new();
        }
    };
    let Some(cwds) = process_cwds().await else {
        return Vec::new();
    };
    let mut pending = Vec::new();
    for worktree in listed {
        if held(&worktree, &sessions, &records, &cwds) {
            continue;
        }
        if let Some(why) = sweep_one(&worktree).await {
            pending.push(Pending { worktree, why });
        }
    }
    pending
}

/// A worktree removed by hand is taken off the list.
fn still_there(worktree: &Path) -> bool {
    if worktree.is_dir() {
        return true;
    }
    worktrees::unregister(worktree);
    false
}

/// Removes `worktree` when nothing can be lost; otherwise why not.
async fn sweep_one(worktree: &Path) -> Option<&'static str> {
    let main = match builtin::main_checkout(worktree).await {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(worktree = %worktree.display(), error = %e, "not a worktree any more");
            return None;
        }
    };
    match worktrees::risk(worktree, &main).await {
        Ok(None) => {}
        Ok(Some(why)) => {
            tracing::info!(worktree = %worktree.display(), why, "a worktree nothing holds waits for the user");
            return Some(why);
        }
        Err(e) => {
            tracing::warn!(worktree = %worktree.display(), error = %e, "cannot check a worktree");
            return None;
        }
    }
    let removal = worktrees::Removal::read(&main, worktree, false).await;
    match removal.run("no session").await {
        Ok(()) => {
            tracing::info!(worktree = %worktree.display(), "removed a worktree nothing holds")
        }
        Err(e) => {
            tracing::warn!(worktree = %worktree.display(), error = %e, "cannot remove a worktree")
        }
    }
    None
}

/// An open Task, a tmux session's worktree, or a pane or process working in it.
fn held(
    worktree: &Path,
    sessions: &[muxa_tmux::Session],
    records: &[Record],
    cwds: &[PathBuf],
) -> bool {
    let inside = |p: &Path| same_dir(p, worktree) || p.starts_with(worktree);
    records.iter().any(|r| {
        matches!(r.status, Status::Open | Status::Review)
            && r.dir.as_deref().is_some_and(|d| same_dir(d, worktree))
    }) || sessions.iter().any(|s| {
        s.worktree
            .as_deref()
            .is_some_and(|w| same_dir(Path::new(w), worktree))
            || s.panes.iter().any(|p| inside(Path::new(&p.path)))
    }) || cwds.iter().any(|c| inside(c))
}

/// Every process's working directory, as `stop-worktree.sh` reads them. An
/// editor or an agent outside tmux holds a worktree too. None when unknown.
async fn process_cwds() -> Option<Vec<PathBuf>> {
    let out = tokio::process::Command::new("lsof")
        .args(["-nP", "-a", "-d", "cwd", "-Fn"])
        .stdin(std::process::Stdio::null())
        .output()
        .await;
    match out {
        Ok(o) => Some(
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| l.strip_prefix('n'))
                .map(PathBuf::from)
                .collect(),
        ),
        Err(e) => {
            tracing::warn!(error = %e, "cannot read process directories; the sweep skips this round");
            None
        }
    }
}

/// ADR 0043 §8: once, list the worktrees muxa made before the list existed:
/// each Task record's `dir` that is a worktree. Blocking.
fn seed_once(records: &[Record]) {
    let Some(dir) = worktrees::dir() else { return };
    let marker = dir.join(".seeded");
    if marker.exists() {
        return;
    }
    for r in records {
        if let Some(d) = r.dir.as_deref().filter(|d| d.join(".git").is_file()) {
            worktrees::register(d);
        }
    }
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&marker, "")) {
        tracing::warn!(error = %e, "cannot mark the worktree list seeded");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_or_open_task_in_a_worktree_holds_it() {
        let wt = Path::new("/w/.worktrees/app-x");
        assert!(held(wt, &[], &[], &[wt.join("src")]));
        assert!(!held(
            wt,
            &[],
            &[],
            &[PathBuf::from("/w/.worktrees/app-xy")]
        ));
        assert!(!held(wt, &[], &[], &[]));
    }
}
