//! The place sweep of ADR 0102 §3: a recorded worktree that nothing holds
//! goes when clean and merged; one that could lose work stays, and the
//! Inbox asks. Never a place muxa has no record of.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muxa_backend::{SessionBackend, TerminalInfo};
use muxa_inbox::{Inbox, NewItem};
use muxa_protocol::v2::{Hold, ItemKind, PlaceKind, WorktreeMode};
use muxa_server::holds::{holds, places_of};
use muxa_server::tasks::WORKSPACE;

use super::Herdr;
use super::close;
use super::place_records::{self, PlaceRecord, Places};
use crate::config::Config;

const EVERY: Duration = Duration::from_secs(600);
/// Seconds a new place is left alone: a Task that is starting has its
/// worktree before its agent terminal, and a new branch is merged.
const GRACE: u64 = 600;
const SOURCE: &str = "host";

/// What one sweep did, for tests and the log.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Swept {
    pub removed: Vec<PathBuf>,
    pub kept: Vec<PathBuf>,
    pub emptied: Vec<PathBuf>,
}

/// Sweeps at start and every 10 minutes while the daemon runs.
pub fn spawn(herdr: Arc<Herdr>, config: Arc<Config>) {
    tokio::spawn(async move {
        loop {
            match herdr.backend().await {
                Ok(backend) => match sweep(&backend, &config, &herdr.inbox).await {
                    Ok(s) => tracing::info!(
                        removed = s.removed.len(),
                        kept = s.kept.len(),
                        emptied = s.emptied.len(),
                        "place sweep"
                    ),
                    Err(e) => tracing::warn!(error = %e, "place sweep skipped"),
                },
                Err(e) => tracing::info!(error = %e, "no herdr-core for the place sweep"),
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// One sweep. When the terminals cannot be listed it changes nothing: no
/// place can be shown to be free.
pub async fn sweep<B: SessionBackend>(
    backend: &B,
    config: &Arc<Config>,
    inbox: &Inbox,
) -> Result<Swept, String> {
    let Some(state_dir) = config.state_dir.clone() else {
        return Ok(Swept::default());
    };
    let holds = holds(backend, inbox).await?;
    let seeds = seeds(&holds.terminals);
    let dir = state_dir.clone();
    let places = blocking(move || {
        place_records::update(&dir, |list| {
            seed_once(list, seeds, crate::records::now());
            list.clone()
        })
        .map_err(|e| e.to_string())
    })
    .await?;
    let drop_after = {
        let c = Arc::clone(config);
        blocking(move || Ok(c.load_workspaces()?.host_build().drop_after)).await?
    };
    let now = crate::records::now();
    let mut swept = Swept::default();
    let mut changed: Vec<Change> = Vec::new();
    for p in places.places {
        match holds.of(&p.path) {
            Some(Hold::Live) if p.ended_since.is_some() => {
                changed.push(Change::Ended(p.path, None))
            }
            Some(Hold::Live) => {}
            Some(Hold::Ended) => {
                let since = p.ended_since.unwrap_or(now);
                if p.ended_since.is_none() {
                    changed.push(Change::Ended(p.path.clone(), Some(now)));
                }
                if now.saturating_sub(since) >= drop_after && !p.build.is_empty() {
                    let emptied = empty(p.build.clone()).await;
                    if emptied {
                        swept.emptied.push(p.path);
                    }
                }
            }
            None if now.saturating_sub(p.made) < GRACE => {}
            None => match free(&p, inbox).await {
                Freed::Removed => {
                    changed.push(Change::Gone(p.path.clone()));
                    swept.removed.push(p.path);
                }
                Freed::Gone => changed.push(Change::Gone(p.path)),
                Freed::Kept => swept.kept.push(p.path),
            },
        }
    }
    blocking(move || {
        place_records::update(&state_dir, |list| apply(list, changed)).map_err(|e| e.to_string())
    })
    .await?;
    Ok(swept)
}

enum Change {
    Ended(PathBuf, Option<u64>),
    Gone(PathBuf),
}

fn apply(list: &mut Places, changes: Vec<Change>) {
    for c in changes {
        match c {
            Change::Ended(path, since) => {
                if let Some(r) = list.places.iter_mut().find(|r| r.path == path) {
                    r.ended_since = since;
                }
            }
            Change::Gone(path) => list.drop_path(&path),
        }
    }
}

enum Freed {
    Removed,
    /// Removed by hand: only the record and the build folders go.
    Gone,
    Kept,
}

/// A place nothing holds. A folder stays (it holds the user's files) but
/// its build output goes; a worktree goes only when nothing can be lost.
async fn free(p: &PlaceRecord, inbox: &Inbox) -> Freed {
    if !p.path.exists() {
        remove_build(p.build.clone()).await;
        return Freed::Gone;
    }
    if p.kind != PlaceKind::Branch {
        if !p.build.is_empty() {
            empty(p.build.clone()).await;
        }
        return Freed::Kept;
    }
    let Some(branch) = &p.branch else {
        return Freed::Kept;
    };
    let tree = match close::tree(&p.path, branch).await {
        Ok(Some(t)) => t,
        Ok(None) => return Freed::Kept,
        Err(e) => {
            tracing::warn!(path = %p.path.display(), error = %e, "cannot check a Task worktree");
            return Freed::Kept;
        }
    };
    let losses = match close::losses_of(&tree).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!(path = %p.path.display(), error = %e, "cannot check a Task worktree");
            return Freed::Kept;
        }
    };
    if !losses.is_empty() {
        ask(inbox, p, &losses);
        return Freed::Kept;
    }
    match close::remove_tree(&tree, WorktreeMode::Remove).await {
        Ok(()) => {
            remove_build(p.build.clone()).await;
            tracing::info!(path = %p.path.display(), "a free Task worktree was removed");
            Freed::Removed
        }
        Err(e) => {
            tracing::warn!(path = %p.path.display(), error = %e, "cannot remove a free Task worktree");
            Freed::Kept
        }
    }
}

/// One Decide item per kept worktree, ever: Done means the user saw it.
fn ask(inbox: &Inbox, p: &PlaceRecord, losses: &[String]) {
    let path = p.path.display().to_string();
    let note = format!(
        "No Task holds this worktree any more, and removing it would lose work: {}.\n\n\
         See it with `muxa-cli disk`. Remove it anyway with `muxa-cli disk clean --yes --force {path}`.",
        losses.join("; ")
    );
    let item = NewItem {
        kind: ItemKind::Decide,
        title: format!("Kept worktree {}", name_of(&p.path)),
        note: Some(note),
        workspace: Some(p.workspace.clone()),
        due: None,
        source: SOURCE.into(),
        key: Some(format!("place-kept:{path}")),
        once: true,
        terminal: None,
        routine: None,
        place: None,
        extra: Default::default(),
    };
    if let Err(e) = inbox.change(|b, now| b.add(item, now).map(drop)) {
        tracing::warn!(error = %e, "cannot ask about a kept worktree");
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The worktrees and folders of Task agents found now, as records.
fn seeds(terminals: &[TerminalInfo]) -> Vec<PlaceRecord> {
    let mut out = Vec::new();
    for t in terminals {
        let Some(ws) = t.labels.get(WORKSPACE) else {
            continue;
        };
        for p in places_of(t) {
            if matches!(p.kind, PlaceKind::Branch | PlaceKind::Folder) {
                out.push(PlaceRecord {
                    workspace: ws.clone(),
                    kind: p.kind,
                    path: PathBuf::from(p.path),
                    branch: p.branch,
                    build: Vec::new(),
                    made: 0,
                    ended_since: None,
                });
            }
        }
    }
    out
}

/// The first sweep records the places of the Tasks muxa runs now: they were
/// made before muxa kept a record (ADR 0102 §2).
fn seed_once(list: &mut Places, seeds: Vec<PlaceRecord>, now: u64) {
    if list.seeded {
        return;
    }
    for s in seeds {
        if list.get(&s.path).is_none() {
            list.put(PlaceRecord { made: now, ..s });
        }
    }
    list.seeded = true;
}

/// Empties each build folder and keeps it, so the links in a place stay
/// good. True when all were emptied.
async fn empty(build: Vec<PathBuf>) -> bool {
    blocking(move || {
        super::build_dirs::remove(&build)?;
        for dir in &build {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        Ok(())
    })
    .await
    .inspect_err(|e| tracing::warn!(error = %e, "cannot empty build folders"))
    .is_ok()
}

async fn remove_build(build: Vec<PathBuf>) {
    if build.is_empty() {
        return;
    }
    if let Err(e) = blocking(move || super::build_dirs::remove(&build)).await {
        tracing::warn!(error = %e, "cannot remove build folders");
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
#[path = "sweep_tests.rs"]
mod tests;
