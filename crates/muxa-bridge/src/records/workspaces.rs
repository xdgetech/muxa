//! Workspaces from the host's `workspaces.toml`, and the path rules that keep
//! a task's directory inside its workspace root. Clients never write this file.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::agent_env::{AgentSettings, RawAgent};
use super::is_valid_id;
use super::loops::{Checks, command_shape};
use super::routines::{RawRoutine, Routine};
use super::workers::{RawWorker, Worker};

/// In characters.
const MAX_NAME: usize = 64;
/// In characters, before `-<id>`.
const MAX_SLUG: usize = 40;
const SLUG_FALLBACK: &str = "task";
const TASKS_DIR: &str = "tasks";
const ROUTINES_DIR: &str = "routines";

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
    /// Canonical. They may lie outside `root` (ADR 0015).
    pub repos: Vec<PathBuf>,
    /// How its agents start; None means as `--task-command` (ADR 0016).
    pub agent: Option<AgentSettings>,
    /// The checks a loop may run; None means no loops here (ADR 0019).
    pub loop_checks: Option<Checks>,
}

/// The configured workspaces, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workspaces {
    list: Vec<Workspace>,
    workers: Vec<Worker>,
    routines: Vec<Routine>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    #[serde(default)]
    workspace: Vec<RawWorkspace>,
    #[serde(default)]
    worker: Vec<RawWorker>,
    #[serde(default)]
    routine: Vec<RawRoutine>,
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
            list.push(Workspace {
                name: w.name,
                root,
                repos,
                agent,
                loop_checks,
            });
        }
        let mut worker_names = HashSet::new();
        let mut workers = Vec::with_capacity(raw.worker.len());
        for raw_worker in raw.worker {
            let worker = raw_worker.check(home, &list).map_err(invalid)?;
            if !worker_names.insert(worker.name.clone()) {
                return Err(invalid("worker name appears twice".into()));
            }
            workers.push(worker);
        }
        let mut routines: Vec<Routine> = Vec::with_capacity(raw.routine.len());
        for raw_routine in raw.routine {
            let routine = raw_routine.check(home, &list, &workers).map_err(invalid)?;
            if routines
                .iter()
                .any(|r| r.workspace == routine.workspace && r.name == routine.name)
            {
                return Err(invalid(format!(
                    "routine {:?} appears twice in workspace {:?}",
                    routine.name, routine.workspace
                )));
            }
            routines.push(routine);
        }
        Ok(Self {
            list,
            workers,
            routines,
        })
    }

    pub fn routines(&self) -> &[Routine] {
        &self.routines
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

    /// Every workspace whose `repos` lists `main`, a main checkout.
    pub fn listing(&self, main: &Path) -> Vec<&Workspace> {
        let main = main.canonicalize().unwrap_or_else(|_| main.to_owned());
        self.list
            .iter()
            .filter(|w| w.repos.contains(&main))
            .collect()
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
