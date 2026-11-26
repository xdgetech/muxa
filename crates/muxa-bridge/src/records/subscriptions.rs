//! `[[subscription]]` in `workspaces.toml` (ADR 0103): a named agent login on
//! the host. muxa keeps its name and folder; the login itself stays with the
//! agent. Workspaces, Bots and the Capture assistant name them in order.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::workers::WorkerAgent;
use crate::agents::{claude, codex};
use crate::thresholds::Snapshot;

/// Characters of a plan label.
const MAX_PLAN: usize = 64;
const MAX_NAME: usize = 64;
/// The most subscriptions one list may name.
pub const MAX_LIST: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    pub name: String,
    pub agent: WorkerAgent,
    /// Absolute, `~/` expanded. Need not exist: the agent makes it at login.
    pub home: PathBuf,
    /// A label only.
    pub plan: Option<String>,
    /// The agent's own default folder: it gets no variable (ADR 0103 §1).
    pub default_login: bool,
}

impl Subscription {
    /// The variable that points the agent at this login; None for the
    /// default login, which the agent finds by itself.
    pub fn env(&self) -> Option<(String, String)> {
        if self.default_login {
            return None;
        }
        let key = match self.agent {
            WorkerAgent::ClaudeCode => claude::DIR_ENV,
            WorkerAgent::Codex => codex::DIR_ENV,
        };
        Some((key.to_owned(), self.home.to_string_lossy().into_owned()))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawSubscription {
    name: String,
    agent: String,
    home: String,
    plan: Option<String>,
}

/// Checks every `[[subscription]]`. Errors name the subscription, never a path.
pub(super) fn check_all(
    raw: Vec<RawSubscription>,
    home: Option<&Path>,
) -> Result<Vec<Subscription>, String> {
    let mut names = HashSet::new();
    let mut homes = HashSet::new();
    let mut out = Vec::with_capacity(raw.len());
    for s in raw {
        name(&s.name)?;
        let at = |field: &str| format!("subscription {:?}: {field}", s.name);
        if !names.insert(s.name.clone()) {
            return Err(at("appears twice"));
        }
        let agent = match s.agent.as_str() {
            "claude-code" => WorkerAgent::ClaudeCode,
            "codex" => WorkerAgent::Codex,
            _ => return Err(at("agent must be \"claude-code\" or \"codex\"")),
        };
        let path = super::workspaces::expand(&s.home, home)
            .map_err(|_| at("home must be absolute or start with ~/"))?;
        if path
            .to_str()
            .is_none_or(|p| p.chars().any(char::is_control))
        {
            return Err(at("home must be UTF-8 without control characters"));
        }
        if !homes.insert(path.clone()) {
            return Err(at("home is the folder of another subscription"));
        }
        if let Some(plan) = &s.plan
            && (plan.chars().count() > MAX_PLAN || plan.chars().any(char::is_control))
        {
            return Err(at(&format!(
                "plan must be one line of at most {MAX_PLAN} characters"
            )));
        }
        let default_dir = match agent {
            WorkerAgent::ClaudeCode => ".claude",
            WorkerAgent::Codex => ".codex",
        };
        let default_login = home.is_some_and(|h| h.join(default_dir) == path);
        out.push(Subscription {
            name: s.name,
            agent,
            home: path,
            plan: s.plan,
            default_login,
        });
    }
    Ok(out)
}

/// `[a-z0-9][a-z0-9_-]*`, at most 64 bytes, like a Bot's name.
pub fn name(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= MAX_NAME
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && value.as_bytes()[0].is_ascii_alphanumeric();
    if ok {
        Ok(())
    } else {
        Err(format!(
            "subscription name must be 1 to {MAX_NAME} lowercase letters, digits, - or _, starting with a letter or digit"
        ))
    }
}

/// A Workspace's list: every name must be a subscription, once.
pub(super) fn check_list(names: &[String], all: &[Subscription]) -> Result<(), String> {
    if names.len() > MAX_LIST {
        return Err(format!("subscriptions has at most {MAX_LIST} names"));
    }
    let mut seen = HashSet::new();
    for n in names {
        if !all.iter().any(|s| &s.name == n) {
            return Err(format!("subscriptions: {n:?} is not a [[subscription]]"));
        }
        if !seen.insert(n) {
            return Err(format!("subscriptions: {n:?} appears twice"));
        }
    }
    Ok(())
}

/// The names of `wanted` that `allowed` has, in `wanted`'s order. A list in a
/// file agents write never blocks a load (ADR 0103 §4): the rest are dropped
/// and named in the log.
pub(super) fn keep_known(wanted: Vec<String>, allowed: &[&str], owner: &str) -> Vec<String> {
    let (kept, dropped): (Vec<String>, Vec<String>) = wanted
        .into_iter()
        .partition(|n| allowed.contains(&n.as_str()));
    if !dropped.is_empty() {
        tracing::warn!(%owner, names = ?dropped, "subscriptions not allowed here are skipped");
    }
    let mut seen = HashSet::new();
    kept.into_iter()
        .filter(|n| seen.insert(n.clone()))
        .collect()
}

/// The kind of login `command` needs: Claude Code or Codex by its program;
/// None for any other program, which gets no subscription.
pub fn agent_of(command: &super::workers::LaunchCommand) -> Option<WorkerAgent> {
    match crate::launch::program(command)? {
        "claude" => Some(WorkerAgent::ClaudeCode),
        "codex" => Some(WorkerAgent::Codex),
        _ => None,
    }
}

/// The subscription an agent started with `command` uses: `keep` when it
/// still exists for that agent (a restart keeps its conversation's login,
/// ADR 0103 §5), else [`pick`] from `names`.
pub fn choose<'a>(
    all: &'a [Subscription],
    names: &[String],
    command: &super::workers::LaunchCommand,
    keep: Option<&str>,
    limits: &Snapshot,
) -> Option<&'a Subscription> {
    let agent = agent_of(command)?;
    keep.and_then(|k| all.iter().find(|s| s.name == k && s.agent == agent))
        .or_else(|| pick(all, names, agent, limits))
}

/// What an agent gets of its subscription: the name for its
/// `muxa.subscription` label and the variable for its environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub name: String,
    pub env: Option<(String, String)>,
    /// Its folder, where the agent keeps its conversations.
    pub home: PathBuf,
}

/// [`choose`], as an agent's label and variable.
pub fn login(
    all: &[Subscription],
    names: &[String],
    command: &super::workers::LaunchCommand,
    keep: Option<&str>,
    limits: &Snapshot,
) -> Option<Login> {
    choose(all, names, command, keep, limits).map(|s| Login {
        name: s.name.clone(),
        env: s.env(),
        home: s.home.clone(),
    })
}

/// The Claude subscription among `names` whose folder holds a conversation
/// for `dir`: an adopted place resumes there (ADR 0103 §5).
pub fn holding(all: &[Subscription], names: &[String], dir: &Path) -> Option<String> {
    names
        .iter()
        .filter_map(|n| all.iter().find(|s| &s.name == n))
        .filter(|s| s.agent == WorkerAgent::ClaudeCode)
        .find(|s| claude::has_conversation(std::slice::from_ref(&s.home), dir))
        .map(|s| s.name.clone())
}

/// `names` as a Workspace lets its agents use them (ADR 0103 §4): all of
/// them in order, or with switching off only the first of each kind, which
/// the user moves to the top to switch by hand.
pub fn order(all: &[Subscription], names: &[String], switch: bool) -> Vec<String> {
    if switch {
        return names.to_vec();
    }
    let mut kinds: Vec<WorkerAgent> = Vec::new();
    names
        .iter()
        .filter(|n| {
            let Some(s) = all.iter().find(|s| &s.name == *n) else {
                return false;
            };
            let first = !kinds.contains(&s.agent);
            kinds.push(s.agent);
            first
        })
        .cloned()
        .collect()
}

/// The subscriptions in `names` for `agent`, in order.
fn of_kind<'a, 'n>(
    all: &'a [Subscription],
    names: &'n [String],
    agent: WorkerAgent,
) -> impl Iterator<Item = &'a Subscription> + use<'a, 'n> {
    names
        .iter()
        .filter_map(|n| all.iter().find(|s| &s.name == n))
        .filter(move |s| s.agent == agent)
}

/// The first subscription in `names` for `agent` below 95% (ADR 0103 §9).
/// When every one is full, still the first: the user's own Task starts, and
/// unattended work asks [`wait_until`] first.
pub fn pick<'a>(
    all: &'a [Subscription],
    names: &[String],
    agent: WorkerAgent,
    limits: &Snapshot,
) -> Option<&'a Subscription> {
    of_kind(all, names, agent)
        .find(|s| limits.full_until(&s.name).is_none())
        .or_else(|| of_kind(all, names, agent).next())
}

/// When unattended work for `agent` may start again: the earliest reset,
/// while every subscription of that kind in `names` is full. None when one
/// has room, when none has current data, or when `names` has none.
pub fn wait_until(
    all: &[Subscription],
    names: &[String],
    agent: WorkerAgent,
    limits: &Snapshot,
) -> Option<u64> {
    let mut earliest: Option<u64> = None;
    for s in of_kind(all, names, agent) {
        let until = limits.full_until(&s.name)?;
        earliest = Some(earliest.map_or(until, |e| e.min(until)));
    }
    earliest
}

#[cfg(test)]
#[path = "subscriptions_tests.rs"]
mod tests;
