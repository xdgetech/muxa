//! Fills in what tmux-agents' tags would say for sessions that have none: the
//! repo from git, and the agent pane from the agent adapters. Tags always win.
//! Blocking (git, `ps`); run it on the blocking pool.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

use muxa_tmux::{Pane, Role, Session, WindowId};

use crate::agents::{PaneInfo, Processes, Registry};

pub fn enrich(sessions: &mut [Session], agents: &Registry) {
    fill_repos(sessions);
    if sessions.iter().any(|s| !untagged_windows(s).is_empty()) {
        match Processes::list() {
            Ok(procs) => {
                for s in sessions.iter_mut() {
                    claim_agents(s, agents, &procs);
                }
            }
            Err(e) => tracing::warn!(error = %e, "cannot find agent panes"),
        }
    }
}

/// A session without `@agent_repo_dir` gets the repo of its first visible pane.
fn fill_repos(sessions: &mut [Session]) {
    let mut found: HashMap<String, Option<(String, String)>> = HashMap::new();
    for s in sessions.iter_mut().filter(|s| s.repo_dir.is_none()) {
        let Some(path) = s
            .panes
            .iter()
            .find(|p| p.role != Role::Hidden)
            .map(|p| p.path.clone())
        else {
            continue;
        };
        let repo = found
            .entry(path)
            .or_insert_with_key(|p| main_checkout(Path::new(p)))
            .clone();
        if let Some((name, dir)) = repo {
            s.repo.get_or_insert(name);
            s.repo_dir = Some(dir);
        }
    }
}

/// The name and absolute path of the main checkout that holds `dir`, also
/// from inside one of its worktrees.
fn main_checkout(dir: &Path) -> Option<(String, String)> {
    if dir.as_os_str().is_empty() {
        return None;
    }
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["rev-parse", "--path-format=absolute", "--git-common-dir"])
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let common = String::from_utf8(out.stdout).ok()?;
    let main = Path::new(common.trim_end_matches('\n')).parent()?;
    let name = main.file_name()?.to_string_lossy().into_owned();
    Some((name, main.to_string_lossy().into_owned()))
}

/// Windows with no agent pane and a pane whose role no tag decided.
fn untagged_windows(s: &Session) -> Vec<WindowId> {
    let mut windows: BTreeMap<WindowId, (bool, bool)> = BTreeMap::new();
    for p in &s.panes {
        let w = windows.entry(p.window).or_default();
        w.0 |= p.role == Role::Agent;
        w.1 |= p.role == Role::Unknown;
    }
    windows
        .into_iter()
        .filter(|(_, (agent, unknown))| !agent && *unknown)
        .map(|(w, _)| w)
        .collect()
}

/// In each untagged window, a pane an adapter recognises is the agent, and the
/// window's other untagged panes are its shells.
fn claim_agents(s: &mut Session, agents: &Registry, procs: &Processes) {
    for window in untagged_windows(s) {
        let claimed = s.panes.iter().position(|p| {
            p.window == window && p.role == Role::Unknown && agents.claims(&info(&s.name, p), procs)
        });
        let Some(agent) = claimed else { continue };
        for (i, p) in s.panes.iter_mut().enumerate() {
            if p.window != window || p.role != Role::Unknown {
                continue;
            }
            p.role = if i == agent { Role::Agent } else { Role::Shell };
        }
    }
}

fn info(session: &str, p: &Pane) -> PaneInfo {
    PaneInfo {
        pane: p.id.to_string(),
        session: session.to_owned(),
        pid: p.pid,
        path: p.path.clone(),
        shell_foreground: p.shell_foreground,
    }
}

#[cfg(test)]
mod tests;
