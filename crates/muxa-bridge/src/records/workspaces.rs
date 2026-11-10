//! Workspaces from the host's `workspaces.toml`, and the path rules that keep
//! a task's directory inside its workspace root. Clients never write this file.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use muxa_protocol::{OverallRequests, Share};
use serde::Deserialize;

use super::agent_env::{AgentSettings, RawAgent};
use super::assistant::{self, AssistantHost, Delegate, RawDelegate, RawHost, Role};
use super::is_valid_id;
use super::loops::{Checks, command_shape};
use super::routines::{self, Routine, RoutineGate, RoutineProblem};
use super::workers::{RawWorker, Worker};

/// In characters.
const MAX_NAME: usize = 64;
/// In characters, before `-<id>`.
const MAX_SLUG: usize = 40;
const SLUG_FALLBACK: &str = "task";
pub(super) const TASKS_DIR: &str = "tasks";
pub(super) const ROUTINES_DIR: &str = "routines";
pub(super) const ASSISTANT_DIR: &str = "assistant";

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path} is not valid TOML for workspaces: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("{path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("{0} is not an absolute path")]
    NotAbsolute(PathBuf),
    #[error("{path} is outside the workspace root {root}")]
    Outside { path: PathBuf, root: PathBuf },
    #[error("cannot resolve {path}: {source}")]
    Resolve { path: PathBuf, source: io::Error },
    #[error("not a task id; ids look like t-1a2b3c")]
    InvalidId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    pub name: String,
    /// Canonical.
    pub root: PathBuf,
    /// Canonical. They may lie outside `root` (ADR 0015); the repos under
    /// `root` need no entry (ADR 0031, `repos.rs`).
    pub repos: Vec<PathBuf>,
    /// How its agents start; None means as `--task-command` (ADR 0016).
    pub agent: Option<AgentSettings>,
    /// The checks a loop may run; None means no loops here (ADR 0019).
    pub loop_checks: Option<Checks>,
    /// Seconds an idle Task waits before the daemon parks it; None is off (ADR 0034).
    pub park_after: Option<u64>,
    /// None: this workspace has no Routines (ADR 0036).
    pub routines: Option<RoutineGate>,
    /// How its assistant runs start: a delegate's or the overall
    /// assistant's. None: it runs none (ADR 0048, 0049).
    pub assistant: Option<Delegate>,
    /// What it is to the overall assistant (ADR 0049 §2–§4).
    pub role: Role,
}

/// The configured workspaces, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workspaces {
    list: Vec<Workspace>,
    workers: Vec<Worker>,
    routines: Vec<Routine>,
    problems: Vec<RoutineProblem>,
    assistant: AssistantHost,
    /// `[codex] owned_threads` (ADR 0028); off without it.
    codex_threads: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    workspace: Vec<RawWorkspace>,
    #[serde(default)]
    worker: Vec<RawWorker>,
    /// Refused: Routines moved to each workspace (ADR 0036).
    #[serde(default)]
    routine: Vec<toml::Value>,
    assistant: Option<RawHost>,
    codex: Option<RawCodex>,
}

/// `[codex]`: the host's switch for Codex threads that muxa owns (ADR 0028).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCodex {
    #[serde(default)]
    owned_threads: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawWorkspace {
    name: String,
    root: String,
    #[serde(default)]
    repos: Vec<String>,
    agent: Option<RawAgent>,
    #[serde(rename = "loop")]
    loop_table: Option<RawLoop>,
    park_after: Option<String>,
    routines: Option<RawGate>,
    assistant: Option<RawDelegate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGate {
    enabled: bool,
    #[serde(default)]
    direct_writes: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLoop {
    checks: RawChecks,
}

/// `"any"`, or a list of exact commands.
#[derive(Deserialize)]
#[serde(untagged)]
enum RawChecks {
    Word(String),
    List(Vec<String>),
}

const ANY: &str = "any";

impl RawLoop {
    /// The error never quotes a command.
    fn check(self) -> Result<Checks, String> {
        match self.checks {
            RawChecks::Word(w) if w == ANY => Ok(Checks::Any),
            RawChecks::Word(_) => Err(format!("loop checks must be {ANY:?} or a list of commands")),
            RawChecks::List(list) => {
                for (i, c) in list.iter().enumerate() {
                    command_shape(c).map_err(|r| format!("loop checks[{i}]: {r}"))?;
                }
                Ok(Checks::List(list))
            }
        }
    }
}

impl Workspaces {
    /// `$XDG_CONFIG_HOME/muxa/workspaces.toml`, else `~/.config/muxa/workspaces.toml`.
    pub fn default_path(xdg_config_home: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
        let base = xdg_config_home
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".config")))?;
        Some(base.join("muxa/workspaces.toml"))
    }

    /// Reads `path`. A missing file means no workspaces. `home` expands a
    /// leading `~/` in roots and repos.
    pub fn load(path: &Path, home: Option<&Path>) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text, path, home),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// `origin` names the file in error messages.
    pub fn parse(text: &str, origin: &Path, home: Option<&Path>) -> Result<Self, ConfigError> {
        let raw: RawConfig = toml::from_str(text).map_err(|e| ConfigError::Parse {
            path: origin.to_owned(),
            message: e.message().to_owned(),
        })?;
        let invalid = |reason: String| ConfigError::Invalid {
            path: origin.to_owned(),
            reason,
        };
        if !raw.routine.is_empty() {
            return Err(invalid(format!(
                "[[routine]] moved to <workspace root>/{}/{} (ADR 0036); \
                 allow it with [workspace.routines] enabled = true",
                routines::WORKSPACE_DIR,
                routines::ROUTINES_FILE
            )));
        }
        let mut names = HashSet::new();
        let mut list = Vec::with_capacity(raw.workspace.len());
        for w in raw.workspace {
            check_name(&w.name).map_err(|r| invalid(format!("workspace name {r}")))?;
            if !names.insert(w.name.clone()) {
                return Err(invalid(format!("workspace {:?} appears twice", w.name)));
            }
            let root = resolve(&w.root, home)
                .map_err(|r| invalid(format!("workspace {:?}: root {r}", w.name)))?;
            let repos = w
                .repos
                .iter()
                .map(|r| resolve(r, home))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|r| invalid(format!("workspace {:?}: repo {r}", w.name)))?;
            let agent = match w.agent {
                Some(a) => a
                    .check(home)
                    .map_err(|r| invalid(format!("workspace {:?}: {r}", w.name)))?,
                None => None,
            };
            let loop_checks = w
                .loop_table
                .map(RawLoop::check)
                .transpose()
                .map_err(|r| invalid(format!("workspace {:?}: {r}", w.name)))?;
            let park_after = w
                .park_after
                .as_deref()
                .map(parse_duration)
                .transpose()
                .map_err(|r| invalid(format!("workspace {:?}: park_after {r}", w.name)))?;
            let routines = w.routines.filter(|g| g.enabled).map(|g| RoutineGate {
                direct_writes: g.direct_writes,
            });
            let (delegate, role) = w
                .assistant
                .map(RawDelegate::check)
                .transpose()
                .map_err(|r| invalid(format!("workspace {:?}: assistant {r}", w.name)))?
                .unwrap_or_default();
            list.push(Workspace {
                name: w.name,
                root,
                repos,
                agent,
                loop_checks,
                park_after,
                routines,
                assistant: delegate,
                role,
            });
        }
        let mut overall = list.iter().filter(|w| w.role == Role::Overall);
        if let (Some(first), Some(second)) = (overall.next(), overall.next()) {
            return Err(invalid(format!(
                "workspaces {:?} and {:?} both set [workspace.assistant] overall = true; \
                 at most one workspace may",
                first.name, second.name
            )));
        }
        let codex_threads = raw.codex.is_some_and(|c| c.owned_threads);
        let mut worker_names = HashSet::new();
        let mut workers = Vec::with_capacity(raw.worker.len());
        for raw_worker in raw.worker {
            let worker = raw_worker.check(home, &list).map_err(invalid)?;
            if !worker_names.insert(worker.name.clone()) {
                return Err(invalid("worker name appears twice".into()));
            }
            workers.push(worker);
        }
        for w in &list {
            if let Some(d) = &w.assistant {
                assistant::check_worker(d, &w.name, &workers, codex_threads)
                    .map_err(|r| invalid(format!("workspace {:?}: assistant {r}", w.name)))?;
            }
        }
        let host = raw
            .assistant
            .map(RawHost::check)
            .transpose()
            .map_err(invalid)?
            .unwrap_or_default();
        let mut all_routines = Vec::new();
        let mut problems = Vec::new();
        for w in &list {
            let Some(gate) = w.routines else { continue };
            match routines::load(w, gate, &workers) {
                Ok(found) => all_routines.extend(found),
                Err(reason) => problems.push(RoutineProblem {
                    workspace: w.name.clone(),
                    reason,
                }),
            }
        }
        Ok(Self {
            list,
            workers,
            routines: all_routines,
            problems,
            assistant: host,
            codex_threads,
        })
    }

    /// Whether a background Codex run gets a thread muxa creates and sends
    /// its turns to (ADR 0028). Off by default.
    pub fn codex_threads(&self) -> bool {
        self.codex_threads
    }

    /// The host's `[assistant]`; off without the table.
    pub fn assistant(&self) -> AssistantHost {
        self.assistant
    }

    /// The workspace's delegate, when the host and the workspace both allow requests.
    pub fn delegate(&self, workspace: &str) -> Option<&Delegate> {
        if !self.assistant.enabled {
            return None;
        }
        let w = self.get(workspace)?;
        if w.role == Role::Overall {
            return None;
        }
        w.assistant.as_ref()
    }

    /// The overall assistant's workspace, when the host allows assistant runs.
    pub fn overall(&self) -> Option<&Workspace> {
        if !self.assistant.enabled {
            return None;
        }
        self.list.iter().find(|w| w.role == Role::Overall)
    }

    /// What `workspace` shares with the overall assistant; `none` for an
    /// unknown workspace and for the overall one.
    pub fn share(&self, workspace: &str) -> Share {
        match self.get(workspace).map(|w| w.role) {
            Some(Role::Member { share, .. }) => share,
            _ => Share::None,
        }
    }

    /// Whether the overall assistant may ask `workspace`: `none` unless it
    /// takes requests now.
    pub fn overall_requests(&self, workspace: &str) -> OverallRequests {
        if self.delegate(workspace).is_none() {
            return OverallRequests::None;
        }
        match self.get(workspace).map(|w| w.role) {
            Some(Role::Member {
                overall_requests, ..
            }) => overall_requests,
            _ => OverallRequests::None,
        }
    }

    pub fn routines(&self) -> &[Routine] {
        &self.routines
    }

    /// Workspaces whose routines file could not be used.
    pub fn routine_problems(&self) -> &[RoutineProblem] {
        &self.problems
    }

    pub fn routine(&self, workspace: &str, name: &str) -> Option<&Routine> {
        self.routines
            .iter()
            .find(|r| r.workspace == workspace && r.name == name)
    }

    // Metadata publication remains gated with the workers runtime.
    #[allow(dead_code)]
    pub fn workers(&self) -> &[Worker] {
        &self.workers
    }

    #[allow(dead_code)]
    pub fn worker(&self, name: &str) -> Option<&Worker> {
        self.workers.iter().find(|worker| worker.name == name)
    }

    pub fn all(&self) -> &[Workspace] {
        &self.list
    }

    pub fn get(&self, name: &str) -> Option<&Workspace> {
        self.list.iter().find(|w| w.name == name)
    }

    /// Every workspace that has `main`, a main checkout, among its repos:
    /// listed, or under its root.
    pub fn listing(&self, main: &Path) -> Vec<&Workspace> {
        let main = main.canonicalize().unwrap_or_else(|_| main.to_owned());
        self.list.iter().filter(|w| w.has_repo(&main)).collect()
    }

    /// Another workspace that has `main` when `workspace` does not. An agent
    /// there would run with the wrong accounts (ADR 0016).
    pub fn owner_elsewhere(&self, main: &Path, workspace: &str) -> Option<&Workspace> {
        let listing = self.listing(main);
        if listing.iter().any(|w| w.name == workspace) {
            return None;
        }
        listing.first().copied()
    }
}

impl Workspace {
    /// `<root>/tasks/<slug>-<id>`, checked to stay inside the root. It is not created.
    pub fn task_dir(&self, title: &str, id: &str) -> Result<PathBuf, PathError> {
        if !is_valid_id(id) {
            return Err(PathError::InvalidId);
        }
        let dir = self
            .root
            .join(TASKS_DIR)
            .join(format!("{}-{id}", slug(title)));
        self.inside(&dir)
    }

    /// `<root>/routines/<slug>-<id>` for a Routine run (ADR 0026), checked
    /// to stay inside the root. It is not created.
    pub fn routine_dir(&self, routine: &str, id: &str) -> Result<PathBuf, PathError> {
        if !is_valid_id(id) {
            return Err(PathError::InvalidId);
        }
        let dir = self
            .root
            .join(ROUTINES_DIR)
            .join(format!("{}-{id}", slug(routine)));
        self.inside(&dir)
    }

    /// `<root>/assistant/<slug>-<id>` for an assistant run (ADR 0048 §4),
    /// checked to stay inside the root. It is not created.
    pub fn assistant_dir(&self, title: &str, id: &str) -> Result<PathBuf, PathError> {
        if !is_valid_id(id) {
            return Err(PathError::InvalidId);
        }
        let dir = self
            .root
            .join(ASSISTANT_DIR)
            .join(format!("{}-{id}", slug(title)));
        self.inside(&dir)
    }

    /// `path` with its existing part canonicalized, if that stays inside the
    /// root. The missing tail may only hold plain names, and no dangling
    /// symlink, so creating it later cannot leave the root.
    pub fn inside(&self, path: &Path) -> Result<PathBuf, PathError> {
        if !path.is_absolute() {
            return Err(PathError::NotAbsolute(path.to_owned()));
        }
        let outside = || PathError::Outside {
            path: path.to_owned(),
            root: self.root.clone(),
        };
        let mut existing = path.to_path_buf();
        let mut tail = Vec::new();
        let base = loop {
            match existing.canonicalize() {
                Ok(c) => break c,
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    if existing.symlink_metadata().is_ok() {
                        return Err(outside());
                    }
                    let name = existing.file_name().ok_or_else(outside)?.to_owned();
                    tail.push(name);
                    if !existing.pop() {
                        return Err(outside());
                    }
                }
                Err(source) => {
                    return Err(PathError::Resolve {
                        path: path.to_owned(),
                        source,
                    });
                }
            }
        };
        let resolved = tail.iter().rev().fold(base, |p, name| p.join(name));
        if resolved.starts_with(&self.root) {
            Ok(resolved)
        } else {
            Err(outside())
        }
    }
}

/// A title as a directory name: `[a-z0-9-]`, dashes collapsed and trimmed,
/// at most 40 characters, `task` when nothing is left.
/// `<n>m`, `<n>h` or `<n>d`, at least one minute, at most 365 days; in seconds.
pub fn parse_duration(text: &str) -> Result<u64, String> {
    let bad = || format!("{text:?} is not a duration such as \"30m\", \"12h\" or \"2d\"");
    let (number, unit) = text.split_at(text.len().saturating_sub(1));
    let scale = match unit {
        "m" => 60,
        "h" => 3600,
        "d" => 86_400,
        _ => return Err(bad()),
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(bad());
    }
    let seconds = number
        .parse::<u64>()
        .ok()
        .and_then(|n| n.checked_mul(scale))
        .ok_or_else(bad)?;
    if !(60..=365 * 86_400).contains(&seconds) {
        return Err(format!("{text:?} must be from 1m to 365d"));
    }
    Ok(seconds)
}

pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(MAX_SLUG);
    for c in title.chars() {
        if out.len() == MAX_SLUG {
            break;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        SLUG_FALLBACK.to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn check_name(name: &str) -> Result<(), &'static str> {
    if name.trim().is_empty() {
        return Err("is empty");
    }
    if name.chars().count() > MAX_NAME {
        return Err("is longer than 64 characters");
    }
    if name.chars().any(char::is_control) {
        return Err("has a control character");
    }
    Ok(())
}

/// Expands `~/`, then canonicalizes an absolute, existing path.
fn resolve(raw: &str, home: Option<&Path>) -> Result<PathBuf, String> {
    let path = match raw.strip_prefix("~/").or((raw == "~").then_some("")) {
        Some(rest) => home
            .ok_or_else(|| format!("{raw:?} starts with ~ but HOME is not set"))?
            .join(rest),
        None => PathBuf::from(raw),
    };
    if !path.is_absolute() {
        return Err(format!("{raw:?} is not an absolute path"));
    }
    path.canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod park_tests {
    use super::*;

    #[test]
    fn park_after_is_minutes_hours_or_days_and_off_by_default() {
        assert_eq!(parse_duration("30m"), Ok(1800));
        assert_eq!(parse_duration("12h"), Ok(43_200));
        assert_eq!(parse_duration("2d"), Ok(172_800));
        for bad in [
            "",
            "h",
            "12",
            "1.5h",
            "-1h",
            "0m",
            "366d",
            "12 h",
            "99999999999999999999d",
        ] {
            assert!(parse_duration(bad).is_err(), "{bad:?}");
        }
        let root = std::env::temp_dir();
        let root = root.to_string_lossy();
        let text = |extra: &str| format!("[[workspace]]\nname = \"w\"\nroot = \"{root}\"\n{extra}");
        let parse = |t: &str| Workspaces::parse(t, Path::new("w.toml"), None);
        assert_eq!(parse(&text("")).unwrap().all()[0].park_after, None);
        let set = parse(&text("park_after = \"12h\"\n")).unwrap();
        assert_eq!(set.all()[0].park_after, Some(43_200));
        let err = parse(&text("park_after = \"soon\"\n"))
            .unwrap_err()
            .to_string();
        assert!(err.contains("park_after"), "{err}");
    }
}
