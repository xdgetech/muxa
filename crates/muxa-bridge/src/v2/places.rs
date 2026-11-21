//! A v2 Task's places (ADR 0068): a folder, branch worktrees and read-only
//! repos. The working place is the folder, else the first branch; the agent
//! starts there. `.muxa/TASK.md` there lists every place, and Claude Code
//! gets the others as additional directories, with edits denied in the
//! read-only ones. Read only is a rule for the agent, not a boundary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use muxa_protocol::v2::{Place, PlaceKind, PlaceSpec};
use muxa_server::TaskError;

use crate::records::{Workspace, is_valid_id};
use crate::task_records::brief;
use crate::tasks::{self, builtin};

/// Claude Code's settings for the other places, beside `TASK.md`.
const CLAUDE_SETTINGS: &str = "claude-settings.json";
const TASK_FILE: &str = "TASK.md";

/// A request's places, checked: each repo resolved to its main checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checked {
    Folder,
    Branch(tasks::TaskRequest),
    Read(PathBuf),
}

/// The places as made, the working place first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub places: Vec<Place>,
    /// The main checkout and branch of a worktree working place.
    pub repo: Option<PathBuf>,
    pub branch: Option<String>,
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
                let req = tasks::validate(&path.to_string_lossy(), branch, base.as_deref())
                    .await
                    .map_err(invalid)?;
                Checked::Branch(req)
            }
            PlaceSpec::Read { repo } => {
                let path = repo_path(ws, repo)?;
                Checked::Read(builtin::main_checkout(&path).await.map_err(invalid)?)
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
        Checked::Read(main) => Ok(Some(main.clone())),
        Checked::Branch(req) => builtin::main_checkout(&req.repo)
            .await
            .map(Some)
            .map_err(invalid),
    }
}

fn repo_path(ws: &Workspace, raw: &str) -> Result<PathBuf, TaskError> {
    ws.repo_path(raw)
        .map_err(|e| TaskError::Invalid(format!("Workspace {}: {e}", ws.name)))
}

/// Makes the folder and the worktrees, or finds existing worktrees.
pub async fn make(ws: &Workspace, title: &str, checked: Vec<Checked>) -> Result<Made, TaskError> {
    let working = checked
        .iter()
        .position(|c| *c == Checked::Folder)
        .or_else(|| checked.iter().position(|c| matches!(c, Checked::Branch(_))))
        .ok_or_else(|| TaskError::Invalid("a Task needs a folder or a branch".into()))?;
    let mut ordered = checked;
    let first = ordered.remove(working);
    ordered.insert(0, first);
    let mut made = Made {
        places: Vec::with_capacity(ordered.len()),
        repo: None,
        branch: None,
    };
    for (i, c) in ordered.into_iter().enumerate() {
        let place = match c {
            Checked::Folder => Place {
                kind: PlaceKind::Folder,
                path: folder(ws, title)?.to_string_lossy().into_owned(),
                branch: None,
            },
            Checked::Branch(req) => {
                let tree = builtin::worktree(&req).await.map_err(invalid)?;
                if i == 0 {
                    made.repo = Some(tree.main);
                    made.branch = Some(req.branch.clone());
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
    if made.repo.is_some() {
        brief::exclude(working)?;
    }
    if working.join(".muxa").join(TASK_FILE).exists() {
        return Ok(());
    }
    brief::write_file(working, TASK_FILE, &task_text(made, title, goal)).map(drop)
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
    let path = brief::write_file(made.working(), CLAUDE_SETTINGS, &settings_text(others))?;
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

fn invalid(e: tasks::TaskError) -> TaskError {
    match e {
        tasks::TaskError::Invalid(m) => TaskError::Invalid(m),
        other => TaskError::Failed(other.to_string()),
    }
}

#[cfg(test)]
#[path = "places_tests.rs"]
mod tests;
