//! Routine runs on the session backend (plan F1, ADR 0026, 0067): each run's
//! agent is a terminal labelled `muxa.kind = routine`, started in the run's
//! folder with its prompt as an argument. When the agent reports `done`
//! (OSC 7501), the output check runs; a pass publishes and ends the terminal,
//! a failure types the fixed prompt, up to [`MAX_ATTEMPTS`]. The schedule,
//! the Routine's state file and the publish are the v1 engine's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use jiff::civil::Date;
use muxa_backend::{
    Change, ChangeStream, Labels, Launch, SessionBackend, Size, TerminalId, TerminalInfo,
    TerminalStream,
};
use muxa_protocol::RoutineFailure;

use super::git as brief;
use super::mcp;
use crate::loops::CheckEnd;
use crate::records::routines::Routine;
use crate::records::workers::{LaunchCommand, WorkerChoice};
use crate::records::{Workspaces, is_valid_id, now};
use crate::routines::check::{self, RunAt};
use crate::routines::record::{MAX_ATTEMPTS, fail_prompt};
use crate::routines::runs::{self, StartError, When};
use crate::routines::store::{RoutineState, RoutineStore, StateError};

pub const KIND: &str = "routine";
const RUN: &str = "muxa.run";
const ROUTINE: &str = "muxa.routine";
const OCCURRENCE: &str = "muxa.occurrence";
const START_PROMPT: &str = "Read .muxa/TASK.md and start.";
/// A run that has not published by then fails and its agent ends.
const RUN_LIMIT: u64 = 2 * 60 * 60;
/// The size a run's terminal starts at; nobody views it unless asked.
const SIZE: Size = Size {
    cols: 120,
    rows: 40,
};

pub struct Runner {
    config: Arc<crate::config::Config>,
    /// `--task-command`: the agent when the Workspace names none.
    fallback: String,
}

/// What a run needs to start, made before the agent runs.
struct Ready {
    routine: Routine,
    occurrence: String,
    run: String,
    dir: PathBuf,
    argv: Vec<String>,
    env: BTreeMap<String, String>,
}

impl Runner {
    pub fn new(config: Arc<crate::config::Config>, fallback: String) -> Self {
        Runner { config, fallback }
    }

    /// Starts one run: claims it, makes its folder and runs its agent.
    pub async fn start<B: SessionBackend>(
        &self,
        backend: &B,
        workspace: &str,
        name: &str,
        when: When,
    ) -> Result<RoutineState, StartError> {
        let (config, fallback) = (Arc::clone(&self.config), self.fallback.clone());
        let (ws, nm) = (workspace.to_owned(), name.to_owned());
        let ready = tokio::task::spawn_blocking(move || ready(&config, &ws, &nm, when, &fallback))
            .await
            .map_err(|_| StartError::Failed("the routine start stopped".into()))??;
        let launch = Launch {
            argv: ready.argv.clone(),
            cwd: Some(ready.dir.clone()),
            name: Some(format!("{} {}", ready.routine.name, ready.occurrence)),
            labels: labels(&ready),
            env: ready.env.clone(),
        };
        let started = backend.run(&launch).await;
        let error = started.as_ref().err().map(ToString::to_string);
        let config = Arc::clone(&self.config);
        let run = started.is_ok().then(|| ready.run.clone());
        let state = tokio::task::spawn_blocking(move || {
            runs::finish_start(&config, &ready.routine, &ready.occurrence, run, error)
        })
        .await
        .map_err(|_| StartError::Failed("the routine start stopped".into()))??;
        Ok(state)
    }

    /// The agent ended a turn: check the output, and publish, prompt or fail.
    pub async fn turn_ended<B: SessionBackend>(&self, backend: &B, t: &TerminalInfo) {
        let Some(run) = run_of(t) else { return };
        if !self.is_open(&run).await {
            return;
        }
        let attempt = attempts(&run.dir) + 1;
        let (config, at) = (Arc::clone(&self.config), run.clone());
        let end = tokio::task::spawn_blocking(move || {
            let before = last_published(&config, &at);
            let end = check::run_at(&config, &at, attempt);
            if matches!(end, CheckEnd::Exited { code: 0, .. }) {
                if let Some(old) = before.filter(|old| *old != at.id) {
                    remove_run_dir(&config, &at, &old);
                }
            }
            end
        })
        .await
        .unwrap_or(CheckEnd::Exited {
            code: 1,
            seconds: 0,
        });
        let passed = matches!(end, CheckEnd::Exited { code: 0, .. });
        if passed {
            end_terminal(backend, &t.id).await;
        } else if attempt >= MAX_ATTEMPTS {
            let why = format!("The output check failed {attempt} times.");
            self.fail(&run, &why).await;
            end_terminal(backend, &t.id).await;
        } else if let Err(e) = type_line(backend, &t.id, &fail_prompt(attempt)).await {
            self.fail(&run, &format!("The fix prompt was not typed: {e}"))
                .await;
            end_terminal(backend, &t.id).await;
        }
    }

    /// The agent ended: a run that did not publish fails.
    pub async fn agent_ended(&self, t: &TerminalInfo) {
        if let Some(run) = run_of(t) {
            if self.is_open(&run).await {
                self.fail(&run, "The agent ended without a publish.").await;
            }
        }
    }

    /// The daemon's tick: starts due runs, and fails open runs whose agent
    /// is gone or that ran past [`RUN_LIMIT`].
    pub async fn tick<B: SessionBackend>(&self, backend: &B) {
        let live: Vec<TerminalInfo> = match backend.list().await {
            Ok(list) => list
                .into_iter()
                .filter(|t| t.live && t.labels.get("muxa.kind").map(String::as_str) == Some(KIND))
                .collect(),
            Err(e) => {
                tracing::info!(error = %e, "cannot list run terminals");
                return;
            }
        };
        let config = Arc::clone(&self.config);
        let at = now();
        let running: Vec<String> = live
            .iter()
            .filter_map(|t| t.labels.get(RUN).cloned())
            .collect();
        let (due, overdue) = tokio::task::spawn_blocking(move || {
            let mut overdue = Vec::new();
            let due = runs::scan_with(&config, at, |store, _, routine, state| {
                if let Some(id) = close_if_gone(store, routine, state, &running, at) {
                    overdue.push(id);
                }
            });
            (due, overdue)
        })
        .await
        .unwrap_or_default();
        for id in overdue {
            if let Some(t) = live.iter().find(|t| t.labels.get(RUN) == Some(&id)) {
                end_terminal(backend, &t.id).await;
            }
        }
        for (workspace, name, date) in due {
            self.start_scheduled(backend, &workspace, &name, date).await;
        }
    }

    async fn start_scheduled<B: SessionBackend>(
        &self,
        backend: &B,
        ws: &str,
        name: &str,
        date: Date,
    ) {
        match self.start(backend, ws, name, When::Scheduled(date)).await {
            Ok(_) | Err(StartError::Handled | StartError::Busy(_)) => {}
            Err(e) => tracing::info!(routine = %name, error = %e, "scheduled run not started"),
        }
    }

    /// Follows the run terminals until the subscription ends.
    pub async fn watch<B: SessionBackend>(&self, backend: &B) -> Result<(), String> {
        let filter = Labels::from([("muxa.kind".to_owned(), KIND.to_owned())]);
        let (initial, mut changes) = backend
            .subscribe(&filter)
            .await
            .map_err(|e| e.to_string())?;
        let mut known: BTreeMap<TerminalId, TerminalInfo> =
            initial.into_iter().map(|t| (t.id.clone(), t)).collect();
        while let Some(change) = changes.next().await {
            match change.map_err(|e| e.to_string())? {
                Change::Created(t) => {
                    known.insert(t.id.clone(), t);
                }
                Change::Status { id, status } => {
                    let Some(t) = known.get_mut(&id) else {
                        continue;
                    };
                    let done = status.as_ref().is_some_and(|s| s.state == "done");
                    t.status = status;
                    if done {
                        let t = t.clone();
                        self.turn_ended(backend, &t).await;
                    }
                }
                Change::Ended { id, .. } => {
                    if let Some(t) = known.get(&id) {
                        self.agent_ended(t).await;
                    }
                }
                Change::Removed { id } => {
                    known.remove(&id);
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn is_open(&self, run: &RunAt) -> bool {
        let (config, run) = (Arc::clone(&self.config), run.clone());
        tokio::task::spawn_blocking(move || {
            let store = RoutineStore::open(&runs::dir(&config).ok()?).ok()?;
            let state = store.get(&run.workspace, &run.name).ok()?;
            state.running?.task.filter(|t| *t == run.id)
        })
        .await
        .ok()
        .flatten()
        .is_some()
    }

    async fn fail(&self, run: &RunAt, why: &str) {
        let (config, run, why) = (Arc::clone(&self.config), run.clone(), why.to_owned());
        let saved = tokio::task::spawn_blocking(move || {
            let store = RoutineStore::open(&runs::dir(&config).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            store
                .update::<_, StateError>(&run.workspace, &run.name, |s| {
                    close(s, &run.occurrence, &run.id, &why, now());
                    Ok(())
                })
                .map_err(|e| e.to_string())?;
            tracing::info!(routine = %run.name, run = %run.id, "routine run failed");
            Ok::<_, String>(())
        })
        .await;
        if let Ok(Err(e)) | Err(e) = saved.map_err(|e| e.to_string()) {
            tracing::warn!(error = %e, "cannot keep a routine failure");
        }
    }
}

/// Blocking. The run id of the Routine's last publish.
fn last_published(config: &crate::config::Config, run: &RunAt) -> Option<String> {
    let store = RoutineStore::open(&runs::dir(config).ok()?).ok()?;
    let state = store.get(&run.workspace, &run.name).ok()?;
    state.last_success.map(|s| s.task)
}

/// Blocking. A publish replaces the previous one, whose folder goes; a failed
/// run's folder stays to be read (ADR 0037 §2).
fn remove_run_dir(config: &crate::config::Config, run: &RunAt, old: &str) {
    let Ok(workspaces) = runs::load(config) else {
        return;
    };
    let Some(dir) = workspaces
        .get(&run.workspace)
        .and_then(|w| w.routine_dir(&run.name, old).ok())
        .filter(|d| d.is_dir())
    else {
        return;
    };
    match std::fs::remove_dir_all(&dir) {
        Ok(()) => {
            tracing::info!(dir = %dir.display(), "removed the previous published run's folder")
        }
        Err(e) => {
            tracing::warn!(dir = %dir.display(), error = %e, "cannot remove a published run's folder")
        }
    }
}

/// Blocking. Claims the run, makes its folder and works out the agent.
fn ready(
    config: &crate::config::Config,
    workspace: &str,
    name: &str,
    when: When,
    fallback: &str,
) -> Result<Ready, StartError> {
    let workspaces = runs::load(config)?;
    let routine = workspaces.routine(workspace, name).cloned().ok_or_else(|| {
        StartError::Bad(format!(
            "no routine {name:?} in workspace {workspace:?}; list-routines shows the host's Routines"
        ))
    })?;
    if !routine.enabled {
        return Err(StartError::Bad(format!(
            "routine {name:?} is not enabled; set enabled = true in its routines.toml"
        )));
    }
    let occurrence = runs::occurrence(&routine, when);
    let store = RoutineStore::open(&runs::dir(config)?)?;
    let fail = |e: StartError| runs::fail_claimless(&store, &routine, &occurrence, when, e);
    if runs::runs_codex(&workspaces, &routine, fallback) {
        return Err(fail(StartError::Unsupported(CODEX.into())));
    }
    let instructions = routine
        .read_instructions()
        .map_err(|e| fail(StartError::Failed(e.to_string())))?;
    let goal = crate::routines::brief::goal(&routine, &instructions);
    runs::claim(&store, &routine, &occurrence, when)?;
    // After the claim: a failure ends the claimed run with its reason.
    made(
        config,
        &workspaces,
        routine.clone(),
        occurrence.clone(),
        &goal,
        fallback,
    )
    .map_err(|why| {
        let _ = runs::finish_start(config, &routine, &occurrence, None, Some(why.clone()));
        StartError::Failed(why)
    })
}

// A run acts on `done`, which only OSC 7501 reports. Whether Codex sends it
// by itself is unchecked (docs/findings/session-backends.md).
const CODEX: &str =
    "a Routine on herdr-core needs Claude Code; Codex Routines are not supported yet";

fn made(
    config: &crate::config::Config,
    workspaces: &Workspaces,
    routine: Routine,
    occurrence: String,
    goal: &str,
    fallback: &str,
) -> Result<Ready, String> {
    let ws = workspaces
        .get(&routine.workspace)
        .ok_or("the Routine's workspace is gone")?;
    let run = new_id().ok_or("cannot make a run id")?;
    let dir = ws
        .routine_dir(&routine.name, &run)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let choice = match routine.worker.as_deref() {
        None => WorkerChoice::WorkspaceDefault,
        Some(name) => WorkerChoice::Named(name),
    };
    let mut resolved = workspaces
        .resolve_worker(&routine.workspace, choice, None, fallback)
        .map_err(|e| e.to_string())?;
    let mut text = format!("# {} {occurrence}\n\n{goal}\n", routine.name);
    if let Some(instructions) = &resolved.instructions {
        text.push_str(&format!("\n## Your role\n\n{}\n", instructions.trim_end()));
    }
    brief::write_file(&dir, "TASK.md", &text)?;
    crate::routines::prepare_at(config, &routine.workspace, &routine.name, &dir)?;
    let servers = mcp::chosen(ws, &routine.mcp)?;
    let command = if crate::launch::is_claude_code(&resolved.command) {
        let config = mcp::write(&dir, &servers, &ws.connections)?;
        mcp::with_flags(resolved.command, &config)
    } else {
        mcp::only_claude(crate::launch::program(&resolved.command), &servers)?;
        resolved.command
    };
    let named = mcp::with_connections(&routine.connections, &servers);
    let given = crate::connections::env(ws, &named)?;
    crate::connections::merge(&mut resolved.environment.env, given);
    Ok(Ready {
        argv: argv(&command, START_PROMPT),
        env: resolved.environment.env.into_iter().collect(),
        routine,
        occurrence,
        run,
        dir,
    })
}

/// Pure. The agent with its first prompt as the last argument.
fn argv(command: &LaunchCommand, prompt: &str) -> Vec<String> {
    match command {
        LaunchCommand::Shell(line) => vec![
            "/bin/sh".into(),
            "-c".into(),
            format!("{line} '{}'", prompt.replace('\'', "'\\''")),
        ],
        LaunchCommand::Native { program, args } => std::iter::once((*program).to_owned())
            .chain(args.iter().cloned())
            .chain(std::iter::once(prompt.to_owned()))
            .collect(),
    }
}

fn labels(r: &Ready) -> Labels {
    Labels::from([
        ("muxa.kind".to_owned(), KIND.to_owned()),
        ("muxa.role".to_owned(), "agent".to_owned()),
        ("muxa.dir".to_owned(), r.dir.to_string_lossy().into_owned()),
        ("muxa.workspace".to_owned(), r.routine.workspace.clone()),
        (ROUTINE.to_owned(), r.routine.name.clone()),
        (OCCURRENCE.to_owned(), r.occurrence.clone()),
        (RUN.to_owned(), r.run.clone()),
    ])
}

/// Pure. The run a terminal's labels describe.
fn run_of(t: &TerminalInfo) -> Option<RunAt> {
    let l = &t.labels;
    if l.get("muxa.kind").map(String::as_str) != Some(KIND) {
        return None;
    }
    Some(RunAt {
        workspace: l.get("muxa.workspace")?.clone(),
        name: l.get(ROUTINE)?.clone(),
        occurrence: l.get(OCCURRENCE)?.clone(),
        id: l.get(RUN)?.clone(),
        dir: PathBuf::from(l.get("muxa.dir")?),
    })
}

/// Checks already run: `.muxa/checks/<n>.log`, one per attempt.
fn attempts(dir: &Path) -> u32 {
    std::fs::read_dir(dir.join(".muxa/checks"))
        .map(|d| {
            d.filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
                .count()
        })
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

/// Blocking, under the state file's lock. Closes an open run whose agent is
/// gone, or that ran past the limit; returns the run id of an overdue one,
/// whose agent the caller ends.
fn close_if_gone(
    store: &RoutineStore,
    routine: &Routine,
    state: &RoutineState,
    live: &[String],
    at: u64,
) -> Option<String> {
    let open = state.running.as_ref()?;
    let (why, overdue) = match &open.task {
        Some(id) if !live.contains(id) => ("The run's agent is gone.", false),
        Some(_) if at.saturating_sub(open.started_at) > RUN_LIMIT => {
            ("The run did not publish within 2 hours.", true)
        }
        None if at.saturating_sub(open.started_at) > START_LIMIT => {
            ("The run did not start.", false)
        }
        _ => return None,
    };
    let (occurrence, id) = (
        open.occurrence.clone(),
        open.task.clone().unwrap_or_default(),
    );
    let saved = store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
        close(s, &occurrence, &id, why, at);
        Ok(())
    });
    if let Err(e) = saved {
        tracing::warn!(routine = %routine.name, error = %e, "cannot close a routine run");
    }
    overdue.then_some(id)
}

/// How long a claim may wait for its agent to start.
const START_LIMIT: u64 = 300;

/// Pure. Ends the open run `id` of `occurrence` as the last failure; a run
/// that is no longer the open one changes nothing.
fn close(s: &mut RoutineState, occurrence: &str, id: &str, why: &str, at: u64) {
    let task = s.running.as_ref().and_then(|r| r.task.clone());
    let same = s
        .running
        .as_ref()
        .is_some_and(|r| r.occurrence == occurrence)
        && task.as_deref().unwrap_or_default() == id;
    if !same {
        return;
    }
    s.running = None;
    s.last_failure = Some(RoutineFailure {
        at,
        occurrence: occurrence.to_owned(),
        task,
        reason: why.to_owned(),
    });
}

/// Types `line` and Return into the terminal.
/// Types `line` and Enter into the terminal.
pub(crate) async fn type_line<B: SessionBackend>(
    backend: &B,
    id: &TerminalId,
    line: &str,
) -> Result<(), String> {
    let stream = backend
        .attach(id, SIZE, 0)
        .await
        .map_err(|e| e.to_string())?;
    let mut bytes = line.as_bytes().to_vec();
    bytes.push(b'\r');
    let typed = stream.input(&bytes).map_err(|e| e.to_string());
    // Give the bytes time to leave before the stream goes.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = stream.detach();
    typed
}

async fn end_terminal<B: SessionBackend>(backend: &B, id: &TerminalId) {
    if let Err(e) = backend.kill(id).await {
        tracing::info!(terminal = %id, error = %e, "cannot end a run's agent");
    }
}

fn new_id() -> Option<String> {
    let id = format!("t-{:06x}", getrandom::u32().ok()? & 0x00ff_ffff);
    is_valid_id(&id).then_some(id)
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
