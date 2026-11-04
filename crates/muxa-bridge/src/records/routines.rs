//! `[[routine]]` in a workspace's `.muxa/routines.toml` (ADR 0026 section 1,
//! ADR 0035): what runs, when, as which worker, and what it may publish.
//! Parsing only; `routines/` runs them.
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

use super::Workspace;
use super::cron::{self, Days};
use super::workers::Worker;

pub const DEFAULT_ITEMS: u32 = 10;
pub const MAX_ITEMS: u32 = 50;
/// The instructions leave room in the run's goal for muxa's output section. Bytes.
pub const MAX_INSTRUCTIONS: usize = 12 * 1024;
/// The folder under a workspace root that holds its Routines (ADR 0035).
pub const WORKSPACE_DIR: &str = ".muxa";
pub const ROUTINES_FILE: &str = "routines.toml";
/// Bytes. Far above any real file; an agent can write this file.
const MAX_ROUTINES_FILE: usize = 256 * 1024;
const MAX_NAME: usize = 64;
const MAX_PROJECT: usize = 200;

/// One local time on the days of a schedule, in an IANA time zone.
#[derive(Clone, PartialEq, Eq)]
pub struct Trigger {
    pub hour: i8,
    pub minute: i8,
    pub days: Days,
    /// The five cron fields, as written, or the ones `at` stands for.
    pub schedule: String,
    /// The IANA name, as written in the config.
    pub timezone: String,
    pub tz: jiff::tz::TimeZone,
}

impl fmt::Debug for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.schedule, self.timezone)
    }
}

impl Trigger {
    /// `HH:MM`.
    pub fn at(&self) -> String {
        format!("{:02}:{:02}", self.hour, self.minute)
    }
}

/// What the host allows a workspace's Routines (`[workspace.routines]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoutineGate {
    pub direct_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routine {
    pub workspace: String,
    /// Its id within the workspace.
    pub name: String,
    /// None: run-now only.
    pub trigger: Option<Trigger>,
    /// None: the workspace default agent.
    pub worker: Option<String>,
    pub instructions: PathBuf,
    /// The canonical `.muxa/` folder the instructions must stay in.
    pub dir: PathBuf,
    pub items: u32,
    /// The agent may write to external systems without a proposal.
    pub direct_writes: bool,
    pub project: Option<String>,
    pub enabled: bool,
}

/// A workspace whose routines file could not be used. It lists no Routines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutineProblem {
    pub workspace: String,
    /// Names the file and the Routine. A TOML parse error may quote the file.
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    routine: Vec<RawRoutine>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRoutine {
    name: String,
    at: Option<String>,
    schedule: Option<String>,
    timezone: Option<String>,
    worker: Option<String>,
    instructions: String,
    project: Option<String>,
    enabled: bool,
    output: Option<RawOutput>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOutput {
    items: Option<u32>,
    #[serde(default)]
    direct_writes: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum InstructionsError {
    #[error("cannot read routine instructions {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("routine instructions {path}: {reason}")]
    Invalid { path: PathBuf, reason: &'static str },
}

/// The Routines of `workspace`, from `<root>/.muxa/routines.toml`. A missing
/// folder or file means none. Any error is the whole file's: an agent may
/// have written it, so it must not fail the host's config (ADR 0035 section 2).
pub(super) fn load(
    workspace: &Workspace,
    gate: RoutineGate,
    workers: &[Worker],
) -> Result<Vec<Routine>, String> {
    let file = Path::new(WORKSPACE_DIR).join(ROUTINES_FILE);
    let at = |reason: &str| format!("{}: {reason}", file.display());
    let dir = match workspace.root.join(WORKSPACE_DIR).canonicalize() {
        Ok(dir) => dir,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(at(&format!("cannot resolve the folder: {e}"))),
    };
    if !dir.starts_with(&workspace.root) || !dir.is_dir() {
        return Err(at("the folder must be a folder inside the workspace root"));
    }
    let bytes = match read_regular(&dir.join(ROUTINES_FILE), MAX_ROUTINES_FILE) {
        Ok(bytes) => bytes,
        Err(ReadFail::Io(e)) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(ReadFail::Io(e)) => return Err(at(&format!("cannot read: {e}"))),
        Err(ReadFail::Invalid(reason)) => return Err(at(reason)),
    };
    let text = String::from_utf8(bytes).map_err(|_| at("is not UTF-8"))?;
    let raw: RawFile = toml::from_str(&text).map_err(|e| at(e.message()))?;
    let mut routines: Vec<Routine> = Vec::with_capacity(raw.routine.len());
    for r in raw.routine {
        let routine = r
            .check(workspace, gate, &dir, workers)
            .map_err(|r| at(&r))?;
        if routines.iter().any(|other| other.name == routine.name) {
            return Err(at(&format!("routine {:?} appears twice", routine.name)));
        }
        routines.push(routine);
    }
    Ok(routines)
}

impl RawRoutine {
    /// Errors name the Routine and the field, never a value of the instructions.
    fn check(
        self,
        workspace: &Workspace,
        gate: RoutineGate,
        dir: &Path,
        workers: &[Worker],
    ) -> Result<Routine, String> {
        line(&self.name, MAX_NAME).map_err(|r| format!("routine name {r}"))?;
        let at = |field: &str| format!("routine {:?}: {field}", self.name);
        let trigger = trigger(
            self.at.as_deref(),
            self.schedule.as_deref(),
            self.timezone.as_deref(),
        )
        .map_err(|r| at(&r))?;
        let worker = match self.worker.as_deref() {
            None | Some(muxa_protocol::DEFAULT_WORKER) => None,
            Some(name) => {
                let found = workers
                    .iter()
                    .find(|w| w.name == name)
                    .ok_or_else(|| at("worker is not configured"))?;
                if !found.allows(&workspace.name) {
                    return Err(at("worker is not allowed in its workspace"));
                }
                Some(name.to_owned())
            }
        };
        let instructions = dir.join(relative(&self.instructions).map_err(at)?);
        if let Some(project) = &self.project {
            line(project, MAX_PROJECT).map_err(|r| at(&format!("project {r}")))?;
        }
        let output = self.output.unwrap_or(RawOutput {
            items: None,
            direct_writes: false,
        });
        let items = output.items.unwrap_or(DEFAULT_ITEMS);
        if !(1..=MAX_ITEMS).contains(&items) {
            return Err(at(&format!("output items must be 1 to {MAX_ITEMS}")));
        }
        if output.direct_writes && !gate.direct_writes {
            return Err(at(
                "direct_writes needs direct_writes = true in [workspace.routines] of workspaces.toml",
            ));
        }
        Ok(Routine {
            workspace: workspace.name.clone(),
            name: self.name,
            trigger,
            worker,
            instructions,
            dir: dir.to_owned(),
            items,
            direct_writes: output.direct_writes,
            project: self.project,
            enabled: self.enabled,
        })
    }
}

/// A plain relative path: no `~`, no `..`, nothing absolute.
fn relative(raw: &str) -> Result<&Path, &'static str> {
    const RULE: &str = "instructions must be a path relative to .muxa/, without ..";
    if raw.is_empty() || raw.starts_with('~') || raw.chars().any(char::is_control) {
        return Err(RULE);
    }
    let path = Path::new(raw);
    if path.components().all(|c| matches!(c, Component::Normal(_))) {
        Ok(path)
    } else {
        Err(RULE)
    }
}

enum ReadFail {
    Io(io::Error),
    Invalid(&'static str),
}

/// At most `max` bytes of a regular file. NONBLOCK rejects a FIFO without
/// waiting for a writer.
fn read_regular(path: &Path, max: usize) -> Result<Vec<u8>, ReadFail> {
    let file = File::from(
        rustix::fs::open(
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|e| ReadFail::Io(e.into()))?,
    );
    if !file.metadata().map_err(ReadFail::Io)?.is_file() {
        return Err(ReadFail::Invalid("must be a regular file"));
    }
    let mut bytes = Vec::new();
    file.take((max + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(ReadFail::Io)?;
    if bytes.len() > max {
        return Err(ReadFail::Invalid("is too large"));
    }
    Ok(bytes)
}

impl Routine {
    /// The instructions as they are now: read on every run, so an edit needs
    /// no restart. A symlink may not lead out of `.muxa/`.
    pub fn read_instructions(&self) -> Result<String, InstructionsError> {
        let path = &self.instructions;
        let read = |source| InstructionsError::Read {
            path: path.clone(),
            source,
        };
        let invalid = |reason| InstructionsError::Invalid {
            path: path.clone(),
            reason,
        };
        let real = path.canonicalize().map_err(read)?;
        if !real.starts_with(&self.dir) {
            return Err(invalid("leads outside .muxa/"));
        }
        let bytes = match read_regular(&real, MAX_INSTRUCTIONS) {
            Ok(bytes) => bytes,
            Err(ReadFail::Io(e)) => return Err(read(e)),
            Err(ReadFail::Invalid("is too large")) => return Err(invalid("is larger than 12 KiB")),
            Err(ReadFail::Invalid(reason)) => return Err(invalid(reason)),
        };
        if bytes.contains(&0) {
            return Err(invalid("has a NUL byte"));
        }
        String::from_utf8(bytes).map_err(|_| invalid("is not UTF-8"))
    }
}

/// `at` and `schedule` each need `timezone`; at most one of them.
fn trigger(
    at: Option<&str>,
    schedule: Option<&str>,
    zone: Option<&str>,
) -> Result<Option<Trigger>, String> {
    let (text, zone) = match (at, schedule, zone) {
        (None, None, None) => return Ok(None),
        (Some(_), Some(_), _) => return Err("at and schedule cannot both be set".into()),
        (None, None, Some(_)) => return Err("timezone needs at or schedule".into()),
        (_, _, None) => return Err("at and schedule need a timezone".into()),
        (Some(time), None, Some(zone)) => {
            let (hour, minute) = clock(time)?;
            (cron::daily(hour, minute), zone)
        }
        (None, Some(text), Some(zone)) => {
            (text.split_whitespace().collect::<Vec<_>>().join(" "), zone)
        }
    };
    let parsed = cron::parse(&text)?;
    if zone.is_empty() || zone.chars().any(char::is_control) || zone.len() > 64 {
        return Err("timezone must be an IANA name such as Europe/Berlin".into());
    }
    let tz = jiff::tz::TimeZone::get(zone)
        .map_err(|_| format!("timezone {zone:?} is not a known IANA time zone"))?;
    Ok(Some(Trigger {
        hour: parsed.hour,
        minute: parsed.minute,
        days: parsed.days,
        schedule: text,
        timezone: zone.to_owned(),
        tz,
    }))
}

/// `HH:MM`.
fn clock(time: &str) -> Result<(i8, i8), String> {
    let bad_time = || "at must be HH:MM, 00:00 to 23:59".to_owned();
    let (h, m) = time.split_once(':').ok_or_else(bad_time)?;
    let two = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    if !two(h) || !two(m) {
        return Err(bad_time());
    }
    let (hour, minute): (i8, i8) = (
        h.parse().map_err(|_| bad_time())?,
        m.parse().map_err(|_| bad_time())?,
    );
    if hour > 23 || minute > 59 {
        return Err(bad_time());
    }
    Ok((hour, minute))
}

fn line(value: &str, max: usize) -> Result<(), &'static str> {
    if value.trim().is_empty() {
        return Err("is empty");
    }
    if value.chars().count() > max {
        return Err("is too long");
    }
    if value.chars().any(char::is_control) {
        return Err("has a control character");
    }
    Ok(())
}

#[cfg(test)]
mod tests;
