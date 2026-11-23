//! The editable view of the settings files, as written in them (ADR 0074).
//! It reads the files leniently, so a file the loader refuses still shows,
//! with the reason. Env values and secrets never enter the view.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use muxa_protocol::settings::{
    BotSpec, ConnectionSpec, EnvKey, McpSpec, RoutineSpec, SettingsAgent, SettingsBot,
    SettingsConnection, SettingsLoop, SettingsRoutine, SettingsWorkspace, WorkerSpec,
};
use muxa_protocol::v2::LoopChecks;
use serde::Deserialize;

use super::files::{self, Revision};
use super::secrets::Secrets;
use crate::records::bots::{BOTS_DIR, KEEP_MEMORIES, MAX_MEMORY, MAX_SOUL, MEMORY_FILE, SOUL_FILE};
use crate::records::routines::{MAX_INSTRUCTIONS, ROUTINES_FILE, WORKSPACE_DIR};
use crate::records::workspaces::expand;

#[derive(Deserialize, Default)]
pub struct FileConfig {
    #[serde(default)]
    pub workspace: Vec<FileWorkspace>,
    #[serde(default)]
    pub worker: Vec<WorkerSpec>,
}

#[derive(Deserialize)]
pub struct FileWorkspace {
    pub name: String,
    pub root: String,
    #[serde(default)]
    repos: Vec<String>,
    #[serde(default)]
    hidden_repos: Vec<String>,
    park_after: Option<String>,
    agent: Option<FileAgent>,
    #[serde(rename = "loop")]
    task_loop: Option<FileLoop>,
    routines: Option<FileGate>,
    #[serde(default)]
    connection: Vec<ConnectionSpec>,
    #[serde(default)]
    mcp: Vec<McpSpec>,
}

#[derive(Deserialize)]
struct FileAgent {
    command: Option<String>,
    /// Values are read only to be dropped.
    #[serde(default)]
    env: BTreeMap<String, toml::Value>,
}

#[derive(Deserialize)]
struct FileLoop {
    checks: LoopChecks,
}

#[derive(Deserialize)]
struct FileGate {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    direct_writes: bool,
}

#[derive(Deserialize, Default)]
struct FileRoutines {
    #[serde(default)]
    routine: Vec<FileRoutine>,
    #[serde(default)]
    bot: Vec<BotSpec>,
}

#[derive(Deserialize)]
struct FileRoutine {
    name: String,
    at: Option<String>,
    schedule: Option<String>,
    timezone: Option<String>,
    worker: Option<String>,
    instructions: String,
    project: Option<String>,
    #[serde(default)]
    enabled: bool,
    output: Option<FileOutput>,
    bot: Option<String>,
    #[serde(default)]
    read_only: bool,
}

#[derive(Deserialize)]
struct FileOutput {
    items: Option<u32>,
    #[serde(default)]
    direct_writes: bool,
}

impl FileWorkspace {
    /// `<root>/.muxa`, `~/` expanded, not resolved.
    pub fn muxa_dir(&self, home: Option<&Path>) -> Option<PathBuf> {
        expand(&self.root, home).ok().map(|r| r.join(WORKSPACE_DIR))
    }
}

/// `workspaces.toml` as written; an empty config when it cannot be read.
pub fn parse_config(text: &str) -> Result<FileConfig, String> {
    toml::from_str(text).map_err(|e| format!("workspaces.toml: {}", e.message()))
}

/// The routines file of a Workspace, read for the view and the revision.
pub struct RoutinesFile {
    pub path: PathBuf,
    pub bytes: Option<Vec<u8>>,
}

pub fn read_routines(dir: &Path) -> Result<RoutinesFile, files::FileError> {
    let path = dir.join(ROUTINES_FILE);
    let bytes = files::read(&path)?;
    Ok(RoutinesFile { path, bytes })
}

/// The instruction file's text, when it stays inside `dir` and fits.
fn instructions_text(dir: &Path, rel: &str) -> Option<String> {
    text_in(dir, rel, MAX_INSTRUCTIONS)
}

/// A file's text under `dir`, when it stays inside and fits.
fn text_in(dir: &Path, rel: &str, max: usize) -> Option<String> {
    let real_dir = dir.canonicalize().ok()?;
    let real = dir.join(rel).canonicalize().ok()?;
    if !real.starts_with(&real_dir) || !real.is_file() {
        return None;
    }
    let bytes = files::read(&real).ok()??;
    if bytes.len() > max || bytes.contains(&0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

/// A Bot file's text: empty when it does not exist yet.
fn bot_text(dir: &Path, rel: &str, max: usize) -> Option<String> {
    if dir.join(rel).symlink_metadata().is_err() {
        return Some(String::new());
    }
    text_in(dir, rel, max)
}

/// The Bots of a routines file. The Soul counts in the revision; the
/// Memory does not, because runs write it.
fn bots(raw: Vec<BotSpec>, dir: &Path, revision: &mut Revision) -> Vec<SettingsBot> {
    raw.into_iter()
        .map(|spec| {
            let folder = format!("{BOTS_DIR}/{}", spec.name);
            let soul = format!("{folder}/{SOUL_FILE}");
            let path = dir.join(&soul);
            revision.add(&path, files::read(&path).ok().flatten().as_deref());
            let memory_versions = (1..=KEEP_MEMORIES)
                .take_while(|n| {
                    dir.join(format!("{folder}/history/memory-{n}.md"))
                        .is_file()
                })
                .count();
            SettingsBot {
                soul_text: bot_text(dir, &soul, MAX_SOUL),
                memory_text: bot_text(dir, &format!("{folder}/{MEMORY_FILE}"), MAX_MEMORY),
                memory_versions: u32::try_from(memory_versions).unwrap_or(0),
                spec,
            }
        })
        .collect()
}

/// The Routines in a routines file's text, and why they cannot be shown.
fn routines(
    file: &RoutinesFile,
    dir: &Path,
    revision: &mut Revision,
) -> (Vec<SettingsRoutine>, Vec<SettingsBot>, Option<String>) {
    let Some(bytes) = &file.bytes else {
        return (Vec::new(), Vec::new(), None);
    };
    let parsed = std::str::from_utf8(bytes)
        .map_err(|_| "is not UTF-8".to_owned())
        .and_then(|t| toml::from_str::<FileRoutines>(t).map_err(|e| e.message().to_owned()));
    let raw = match parsed {
        Ok(raw) => raw,
        Err(why) => {
            return (
                Vec::new(),
                Vec::new(),
                Some(format!(".muxa/routines.toml: {why}")),
            );
        }
    };
    let bot_list = bots(raw.bot, dir, revision);
    let list = raw
        .routine
        .into_iter()
        .map(|r| {
            let path = dir.join(&r.instructions);
            revision.add(&path, files::read(&path).ok().flatten().as_deref());
            let text = instructions_text(dir, &r.instructions);
            let output = r.output.unwrap_or(FileOutput {
                items: None,
                direct_writes: false,
            });
            SettingsRoutine {
                spec: RoutineSpec {
                    name: r.name,
                    at: r.at,
                    schedule: r.schedule,
                    timezone: r.timezone,
                    worker: r.worker,
                    instructions: r.instructions,
                    project: r.project,
                    enabled: r.enabled,
                    items: output.items,
                    direct_writes: output.direct_writes,
                    bot: r.bot,
                    read_only: r.read_only,
                },
                instructions_text: text,
            }
        })
        .collect();
    (list, bot_list, None)
}

/// One Workspace of the view. `routines_problem` comes from the loader.
pub fn workspace(
    w: FileWorkspace,
    home: Option<&Path>,
    secrets: &dyn Secrets,
    revision: &mut Revision,
) -> SettingsWorkspace {
    let mut problem = None;
    let mut list = Vec::new();
    let mut bot_list = Vec::new();
    if let Some(dir) = w.muxa_dir(home) {
        match read_routines(&dir) {
            Ok(file) => {
                revision.add(&file.path, file.bytes.as_deref());
                (list, bot_list, problem) = routines(&file, &dir, revision);
            }
            Err(e) => problem = Some(e.to_string()),
        }
    }
    let agent = w
        .agent
        .map_or_else(SettingsAgent::default, |a| SettingsAgent {
            command: a.command,
            env: a
                .env
                .into_keys()
                .map(|key| EnvKey { key, set: true })
                .collect(),
        });
    let connections = w
        .connection
        .into_iter()
        .map(|spec| SettingsConnection {
            secret: secrets.state(&crate::connections::account(&w.name, &spec.name)),
            spec,
        })
        .collect();
    let gate = w.routines.unwrap_or(FileGate {
        enabled: false,
        direct_writes: false,
    });
    let found_repos = found_repos(&w.root, home);
    SettingsWorkspace {
        found_repos,
        name: w.name,
        root: w.root,
        hidden_repos: w.hidden_repos,
        repos: w.repos,
        park_after: w.park_after,
        agent,
        task_loop: w.task_loop.map(|l| SettingsLoop { checks: l.checks }),
        routines_enabled: gate.enabled,
        direct_writes: gate.direct_writes,
        connections,
        mcp: w.mcp,
        routines: list,
        bots: bot_list,
        routines_problem: problem,
    }
}

/// The repos the scan finds under `root`, as named under it. Blocking.
fn found_repos(root: &str, home: Option<&Path>) -> Vec<String> {
    let Ok(root) = expand(root, home) else {
        return Vec::new();
    };
    let root = root.canonicalize().unwrap_or(root);
    crate::records::repos::discover(&root)
        .iter()
        .filter_map(|r| r.strip_prefix(&root).ok())
        .map(|r| r.to_string_lossy().into_owned())
        .collect()
}
