//! A v2 Task's places (ADR 0068): a folder, branch worktrees and read-only
//! repos. The working place is the folder, else the first branch; the agent
//! starts there. `.muxa/TASK.md` there lists every place, and Claude Code
//! gets the others as additional directories, with edits denied in the
//! read-only ones. Read only is a rule for the agent, not a boundary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use muxa_protocol::v2::{Place, PlaceKind, PlaceSpec};
use muxa_server::TaskError;

use super::adopt::{self, Adopted};
use super::git::{self, BranchRequest, GitError};
use crate::records::{Workspace, is_valid_id};

/// Claude Code's settings for the other places, beside `TASK.md`.
const CLAUDE_SETTINGS: &str = "claude-settings.json";
const TASK_FILE: &str = "TASK.md";

/// A request's places, checked: each repo resolved to its main checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    Folder,
    Branch(BranchRequest),
    Read(PathBuf),
    /// A place that is there already; it is the working place.
    Existing(Adopted),
    /// A repo's own checkout as the working place (ADR 0077).
    Checkout(Adopted),
}

/// The places as made, the working place first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub places: Vec<Place>,
    /// The main checkout and branch of a worktree working place.
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
    /// The working place is in a git repo: `.muxa/` goes into its exclude.
    pub in_repo: bool,
}

impl Made {
    pub fn working(&self) -> &Path {
        Path::new(&self.places[0].path)
    }
}

/// Resolves and checks every place. No folder or branch means a folder.
pub async fn check(ws: &Workspace, specs: &[PlaceSpec]) -> Result<Vec<Checked>, TaskError> {
    let mut out = Vec::with_capacity(specs.len() + 1);
    let mut repos = HashSet::new();
    for spec in specs {
        let checked = match spec {
            PlaceSpec::Folder => Checked::Folder,
            PlaceSpec::Branch { repo, branch, base } => {
                let path = repo_path(ws, repo)?;
                let req = git::validate(&path.to_string_lossy(), branch, base.as_deref())
                    .await
                    .map_err(invalid)?;
                Checked::Branch(req)
            }
            PlaceSpec::Read { repo } => {
                let path = repo_path(ws, repo)?;
                Checked::Read(git::main_checkout(&path).await.map_err(invalid)?)
            }
            PlaceSpec::Existing { path } => Checked::Existing(adopt::resolve(ws, path).await?),
            PlaceSpec::Checkout { repo } => Checked::Checkout(checkout(ws, repo).await?),
            PlaceSpec::AllRead => {
                for main in all_repos(ws).await? {
                    if repos.insert(main.clone()) {
                        out.push(Checked::Read(main));
                    }
                }
                continue;
            }
        };
        if let Some(main) = main_of(&checked).await? {
            if !repos.insert(main.clone()) {
                return Err(TaskError::Invalid(format!(
                    "{} is named twice; a Task has one place per repo",
                    main.display()
                )));
            }
        }
        out.push(checked);
    }
    let existing = out
        .iter()
        .filter(|c| matches!(c, Checked::Existing(_) | Checked::Checkout(_)))
        .count();
    if existing > 0 {
        let others = out
            .iter()
            .any(|c| matches!(c, Checked::Folder | Checked::Branch(_)));
        if existing > 1 || others {
            return Err(TaskError::Invalid(
                "an existing place or a checkout is the working place: give one, without a folder or a branch"
                    .into(),
            ));
        }
        return Ok(out);
    }
    match out.iter().filter(|c| **c == Checked::Folder).count() {
        0 if !out.iter().any(|c| matches!(c, Checked::Branch(_))) => out.insert(0, Checked::Folder),
        0 | 1 => {}
        _ => return Err(TaskError::Invalid("a Task has at most one folder".into())),
    }
    Ok(out)
}

async fn main_of(c: &Checked) -> Result<Option<PathBuf>, TaskError> {
    match c {
        Checked::Folder => Ok(None),
        Checked::Existing(a) => Ok(a.main.clone()),
        Checked::Checkout(a) => Ok(Some(a.path.clone())),
        Checked::Read(main) => Ok(Some(main.clone())),
        Checked::Branch(req) => git::main_checkout(&req.repo)
            .await
            .map(Some)
            .map_err(invalid),
    }
}

/// The main checkout of `repo`, canonical, as an adopted place: never a
/// linked worktree, whatever path names it.
async fn checkout(ws: &Workspace, repo: &str) -> Result<Adopted, TaskError> {
    let path = repo_path(ws, repo)?;
    let main = git::main_checkout(&path).await.map_err(invalid)?;
    let main = main.canonicalize().unwrap_or(main);
    Ok(Adopted {
        path: main,
        main: None,
        branch: None,
        in_repo: true,
    })
}

/// The main checkout of every repo of `ws`. A folder the scan finds that is
/// not a git repo is skipped.
async fn all_repos(ws: &Workspace) -> Result<Vec<PathBuf>, TaskError> {
    let ws_owned = ws.clone();
    let listed = tokio::task::spawn_blocking(move || ws_owned.repo_list())
        .await
        .map_err(|e| TaskError::Failed(e.to_string()))?;
    let mut out = Vec::with_capacity(listed.len());
    for repo in listed {
        match git::main_checkout(&repo).await {
            Ok(main) => out.push(main),
            Err(e) => {
                tracing::warn!(repo = %repo.display(), error = %e, "not a repo; all-read skips it")
            }
        }
    }
    Ok(out)
}

fn repo_path(ws: &Workspace, raw: &str) -> Result<PathBuf, TaskError> {
    ws.repo_path(raw)
        .map_err(|e| TaskError::Invalid(format!("Workspace {}: {e}", ws.name)))
}

/// The existing working place of a checked request, if it has one.
pub fn adopted(checked: &[Checked]) -> Option<&Adopted> {
    checked.iter().find_map(|c| match c {
        Checked::Existing(a) | Checked::Checkout(a) => Some(a),
        _ => None,
    })
}

/// Makes the folder and the worktrees, or finds existing worktrees.
pub async fn make(ws: &Workspace, title: &str, checked: Vec<Checked>) -> Result<Made, TaskError> {
    let working = checked
        .iter()
        .position(|c| matches!(c, Checked::Existing(_) | Checked::Checkout(_)))
        .or_else(|| checked.iter().position(|c| *c == Checked::Folder))
        .or_else(|| checked.iter().position(|c| matches!(c, Checked::Branch(_))))
        .ok_or_else(|| TaskError::Invalid("a Task needs a folder or a branch".into()))?;
    let mut ordered = checked;
    let first = ordered.remove(working);
    ordered.insert(0, first);
    let mut made = Made {
        places: Vec::with_capacity(ordered.len()),
        repo: None,
        branch: None,
        in_repo: false,
    };
    for (i, c) in ordered.into_iter().enumerate() {
        let place = match c {
            Checked::Folder => Place {
                kind: PlaceKind::Folder,
                path: folder(ws, title)?.to_string_lossy().into_owned(),
                branch: None,
            },
            Checked::Branch(req) => {
                let tree = git::worktree(&req).await.map_err(invalid)?;
                if i == 0 {
                    made.repo = Some(tree.main);
                    made.branch = Some(req.branch.clone());
                    made.in_repo = true;
                }
                Place {
                    kind: PlaceKind::Branch,
                    path: tree.path.to_string_lossy().into_owned(),
                    branch: Some(req.branch),
                }
            }
            Checked::Read(main) => Place {
                kind: PlaceKind::Read,
                path: main.to_string_lossy().into_owned(),
                branch: None,
            },
            Checked::Checkout(a) => {
                made.in_repo = true;
                made.repo = Some(a.path.clone());
                Place {
                    kind: PlaceKind::Checkout,
                    branch: git::current_branch(&a.path).await,
                    path: a.path.to_string_lossy().into_owned(),
                }
            }
            Checked::Existing(a) => {
                made.in_repo = a.in_repo;
                made.repo = a.main;
                made.branch = a.branch.clone();
                Place {
                    kind: if a.branch.is_some() {
                        PlaceKind::Branch
                    } else {
                        PlaceKind::Folder
                    },
                    path: a.path.to_string_lossy().into_owned(),
                    branch: a.branch,
                }
            }
        };
        made.places.push(place);
    }
    Ok(made)
}

/// `<root>/tasks/<slug>-<id>`, made now. A new id each time: a folder
/// Task is new each time it starts.
fn folder(ws: &Workspace, title: &str) -> Result<PathBuf, TaskError> {
    let id = new_id().ok_or_else(|| TaskError::Failed("cannot make a Task id".into()))?;
    let dir = ws
        .task_dir(title, &id)
        .map_err(|e| TaskError::Invalid(format!("Workspace {}: {e}", ws.name)))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| TaskError::Failed(format!("cannot create {}: {e}", dir.display())))?;
    Ok(dir)
}

fn new_id() -> Option<String> {
    let id = format!("t-{:06x}", getrandom::u32().ok()? & 0x00ff_ffff);
    is_valid_id(&id).then_some(id)
}

/// Writes `.muxa/TASK.md` in the working place unless one is there: a
/// restarted Task keeps its notes. Blocking.
pub fn brief(made: &Made, title: &str, goal: Option<&str>) -> Result<(), String> {
    let working = made.working();
    if made.in_repo {
        git::exclude(working)?;
    }
    if working.join(".muxa").join(TASK_FILE).exists() {
        return Ok(());
    }
    git::write_file(working, TASK_FILE, &task_text(made, title, goal)).map(drop)
}

/// Pure.
pub fn task_text(made: &Made, title: &str, goal: Option<&str>) -> String {
    let mut out = format!("# {title}\n\n");
    if let Some(goal) = goal.map(str::trim).filter(|g| !g.is_empty()) {
        out.push_str(&format!("## Goal\n\n{goal}\n\n"));
    }
    out.push_str("## Places\n\n");
    for (i, p) in made.places.iter().enumerate() {
        let what = match (&p.kind, &p.branch) {
            (PlaceKind::Folder, _) => "a folder for this Task".to_owned(),
            (PlaceKind::Branch, Some(b)) => format!("a worktree on branch {b}"),
            (PlaceKind::Branch, None) => "a worktree".to_owned(),
            (PlaceKind::Read, _) => "read only: do not change anything here".to_owned(),
            (PlaceKind::Checkout, Some(b)) => {
                format!("the repo's own checkout on {b}: changes land there directly")
            }
            (PlaceKind::Checkout, None) => {
                "the repo's own checkout: changes land there directly".to_owned()
            }
        };
        let lead = if i == 0 {
            "Working place, you start here: "
        } else {
            ""
        };
        out.push_str(&format!("- {lead}{} ({what})\n", p.path));
    }
    out.push_str("\nKeep your notes on progress, decisions and next actions below this line.\n");
    out
}

/// Writes the settings for the other places and returns the flag that loads
/// them, or nothing when the Task has one place. Blocking.
pub fn claude_settings(made: &Made) -> Result<Option<String>, String> {
    let others = &made.places[1..];
    if others.is_empty() {
        return Ok(None);
    }
    let path = git::write_file(made.working(), CLAUDE_SETTINGS, &settings_text(others))?;
    Ok(Some(format!(
        "--settings {}",
        quote(&path.to_string_lossy())
    )))
}

/// Pure. Paths in permission rules start with `//` to be absolute; a rule
/// `Edit(…)` covers every built-in tool that edits a file.
pub fn settings_text(others: &[Place]) -> String {
    let dirs: Vec<&str> = others.iter().map(|p| p.path.as_str()).collect();
    let deny: Vec<String> = others
        .iter()
        .filter(|p| p.kind == PlaceKind::Read)
        .map(|p| format!("Edit(/{}/**)", p.path.trim_end_matches('/')))
        .collect();
    let value = serde_json::json!({
        "permissions": {"additionalDirectories": dirs, "deny": deny},
    });
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

/// One word for `/bin/sh`.
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn invalid(e: GitError) -> TaskError {
    match e {
        GitError::Invalid(m) => TaskError::Invalid(m),
        other => TaskError::Failed(other.to_string()),
    }
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
