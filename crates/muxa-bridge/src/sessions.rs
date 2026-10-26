//! Fills in what tmux-agents' tags would say for sessions that have none: the
//! repo from git, and the agent pane and its state from the agent adapters.
//! Tags always win. Blocking (git, `ps`); run it on the blocking pool.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

use muxa_tmux::{Pane, Role, Session, WindowId};

use crate::agents::{Found, PaneInfo, Processes, Registry};
use crate::convert;

pub fn enrich(sessions: &mut [Session], agents: &Registry) {
    fill_repos(sessions);
    fill_agents(sessions, agents);
}

/// Agent panes and states that no tag gives. Lists processes only when some
/// pane needs an adapter.
pub fn fill_agents(sessions: &mut [Session], agents: &Registry) {
    if !sessions.iter().any(needs_adapter) {
        return;
    }
    match Processes::list() {
        Ok(procs) => {
            for s in sessions.iter_mut() {
                claim_agents(s, agents, &procs);
                fill_states(s, agents, &procs);
            }
        }
        Err(e) => tracing::warn!(error = %e, "cannot find agent panes"),
    }
}

fn needs_adapter(s: &Session) -> bool {
    !untagged_windows(s).is_empty() || s.panes.iter().any(stateless_agent)
}

fn stateless_agent(p: &Pane) -> bool {
    p.role == Role::Agent && p.agent.is_none()
}

/// An agent pane tagged without a state, e.g. by `@muxa_role`, gets the state its adapter sees.
fn fill_states(s: &mut Session, agents: &Registry, procs: &Processes) {
    let name = s.name.clone();
    for p in s.panes.iter_mut().filter(|p| stateless_agent(p)) {
        if let Some(found) = agents.find(&info(&name, p), procs) {
            set_status(p, &found);
        }
    }
}

fn set_status(p: &mut Pane, found: &Found) {
    p.agent = found.status.as_ref().map(convert::agent_status);
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
        let claimed = s.panes.iter().enumerate().find_map(|(i, p)| {
            let candidate = p.window == window && p.role == Role::Unknown;
            candidate
                .then(|| agents.find(&info(&s.name, p), procs))
                .flatten()
                .map(|found| (i, found))
        });
        let Some((agent, found)) = claimed else {
            continue;
        };
        for (i, p) in s.panes.iter_mut().enumerate() {
            if p.window != window || p.role != Role::Unknown {
                continue;
            }
            if i == agent {
                p.role = Role::Agent;
                set_status(p, &found);
            } else {
                p.role = Role::Shell;
            }
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
