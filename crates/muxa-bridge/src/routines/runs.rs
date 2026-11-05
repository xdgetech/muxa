//! Starting a Routine run (run-now or the daemon's tick) and following it to
//! its end (ADR 0026 section 2). One open run per Routine: the claim in its
//! state file is taken before the run's record exists.
use std::sync::Arc;

use jiff::civil::Date;
use muxa_protocol::{RoutineFailure, RoutineRun};

use super::record::RunOf;
use super::schedule;
use super::store::{RoutineState, RoutineStore, StateError};
use crate::records::routines::Routine;
use crate::records::workers::WorkerAgent;
use crate::records::{LoopState, Status, Store, StoreError, Workspaces, now};
use crate::task_records::{Config, Context, NewTask, create};

/// A run that took no record this long after its claim did not start. Seconds.
const START_LIMIT: u64 = 300;

/// What starts a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Scheduled(Date),
    /// Run-now, at this epoch second.
    Now(u64),
}

/// Messages carry names, ids and paths, never instructions or output.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StartError {
    #[error("{0}")]
    Bad(String),
    #[error("{0}")]
    Busy(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Failed(String),
    /// A scheduled occurrence that another tick already handled.
    #[error("the occurrence was already handled")]
    Handled,
}

impl From<StateError> for StartError {
    fn from(e: StateError) -> Self {
        tracing::warn!(error = %e, "routine state failed");
        Self::Failed(e.to_string())
    }
}

/// Codex cannot run a Routine until it has session ownership (ADR 0026,
/// Consequences; ADR 0028): a Routine has no person to type its prompt.
pub const CODEX: &str = "a Routine needs automatic prompt delivery, which Codex does not have yet (ADR 0028). Use a Claude Code worker for this Routine.";

/// Whether the Routine would start Codex: a Codex worker, or a default
/// command whose program is `codex`.
pub fn runs_codex(workspaces: &Workspaces, routine: &Routine, fallback: &str) -> bool {
    match &routine.worker {
        Some(name) => workspaces
            .worker(name)
            .is_some_and(|w| w.agent == WorkerAgent::Codex),
        None => {
            let command = workspaces
                .get(&routine.workspace)
                .and_then(|w| w.agent.as_ref())
                .and_then(|a| a.command.as_deref())
                .unwrap_or(fallback);
            command
                .split_whitespace()
                .next()
                .and_then(|p| p.rsplit('/').next())
                == Some("codex")
        }
    }
}

pub fn occurrence(routine: &Routine, when: When) -> String {
    match (when, &routine.trigger) {
        (When::Scheduled(date), Some(t)) => schedule::occurrence_id(t, date),
        (When::Scheduled(date), None) => date.to_string(),
        (When::Now(at), _) => format!("now-{at}"),
    }
}

/// Starts one run and returns the Routine's state with it. A failure after
/// the claim is kept as the Routine's last failure.
pub async fn start(
    cx: &Context,
    workspace: &str,
    name: &str,
    when: When,
) -> Result<RoutineState, StartError> {
    let (config, ws, nm, fallback) = (
        Arc::clone(&cx.records),
        workspace.to_owned(),
        name.to_owned(),
        cx.tasks.command.clone(),
    );
    let (routine, goal, occurrence) = tokio::task::spawn_blocking(move || {
        let workspaces = load(&config)?;
        let routine = workspaces.routine(&ws, &nm).cloned().ok_or_else(|| {
            StartError::Bad(format!(
                "no routine {nm:?} in workspace {ws:?}; list-routines shows the host's Routines"
            ))
        })?;
        if !routine.enabled {
            return Err(StartError::Bad(format!(
                "routine {nm:?} is not enabled; set enabled = true in workspaces.toml on the host"
            )));
        }
        let occurrence = occurrence(&routine, when);
        let store = RoutineStore::open(&dir(&config)?)?;
        let fail = |e: StartError| fail_claimless(&store, &routine, &occurrence, when, e);
        if runs_codex(&workspaces, &routine, &fallback) {
            return Err(fail(StartError::Unsupported(CODEX.into())));
        }
        let instructions = routine
            .read_instructions()
            .map_err(|e| fail(StartError::Failed(e.to_string())))?;
        let goal = super::brief::goal(&routine, &instructions);
        claim(&store, &routine, &occurrence, when)?;
        Ok((routine, goal, occurrence))
    })
    .await
    .map_err(|_| StartError::Failed("the routine start stopped".into()))??;
    tracing::info!(
        workspace,
        routine = name,
        occurrence,
        "routine run starting"
    );
    let run = RunOf {
        name: routine.name.clone(),
        occurrence: occurrence.clone(),
    };
    let (record, error) = match cx.env().await {
        Ok(env) => {
            let new = NewTask {
                workspace: routine.workspace.clone(),
                worker: routine.worker.clone(),
                title: format!("{} {occurrence}", routine.name),
                goal,
                project: routine.project.clone(),
                routine: Some(run),
                ..NewTask::default()
            };
            let (record, error) = create(&env, new).await;
            (record, error.map(|e| e.message))
        }
        Err(e) => (None, Some(e.message)),
    };
    let config = Arc::clone(&cx.records);
    tokio::task::spawn_blocking(move || {
        finish_start(&config, &routine, &occurrence, record.map(|r| r.id), error)
    })
    .await
    .map_err(|_| StartError::Failed("the routine start stopped".into()))?
}

fn claim(
    store: &RoutineStore,
    routine: &Routine,
    occurrence: &str,
    when: When,
) -> Result<(), StartError> {
    store.update(&routine.workspace, &routine.name, |s| {
        if let When::Scheduled(date) = when {
            if last_date(s).is_some_and(|last| last >= date) {
                return Err(StartError::Handled);
            }
        }
        if let Some(open) = &s.running {
            return Err(StartError::Busy(format!(
                "routine {:?} already runs occurrence {}; wait for it to end",
                routine.name, open.occurrence
            )));
        }
        s.running = Some(RoutineRun {
            occurrence: occurrence.to_owned(),
            started_at: now(),
            task: None,
        });
        if let When::Scheduled(date) = when {
            s.last_occurrence = Some(date.to_string());
        }
        Ok(())
    })
}

/// Keeps a failure that came before the claim; a scheduled occurrence counts
/// as handled, so a broken Routine does not retry every tick.
fn fail_claimless(
    store: &RoutineStore,
    routine: &Routine,
    occurrence: &str,
    when: When,
    error: StartError,
) -> StartError {
    let saved = store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
        s.last_failure = Some(RoutineFailure {
            at: now(),
            occurrence: occurrence.to_owned(),
            task: None,
            reason: format!("The run did not start: {error}"),
        });
        if let When::Scheduled(date) = when {
            s.last_occurrence = Some(date.to_string());
        }
        Ok(())
    });
    if let Err(e) = saved {
        tracing::warn!(routine = %routine.name, error = %e, "cannot keep a routine failure");
    }
    tracing::warn!(routine = %routine.name, occurrence, error = %error, "routine run did not start");
    error
}

fn finish_start(
    config: &Config,
    routine: &Routine,
    occurrence: &str,
    task: Option<String>,
    error: Option<String>,
) -> Result<RoutineState, StartError> {
    let store = RoutineStore::open(&dir(config)?)?;
    if let Some(why) = error {
        if let Some(id) = &task {
            drop_run(config, id, &format!("The run did not start: {why}"));
        }
        store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
            if s.running
                .as_ref()
                .is_some_and(|r| r.occurrence == occurrence)
            {
                s.running = None;
            }
            s.last_failure = Some(RoutineFailure {
                at: now(),
                occurrence: occurrence.to_owned(),
                task: task.clone(),
                reason: format!("The run did not start: {why}"),
            });
            Ok(())
        })?;
        tracing::warn!(routine = %routine.name, occurrence, task, "routine run did not start");
        return Err(StartError::Failed(why));
    }
    let state = store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
        if let Some(open) = s.running.as_mut().filter(|r| r.occurrence == occurrence) {
            open.task = task.clone();
        }
        Ok(s.clone())
    })?;
    tracing::info!(routine = %routine.name, occurrence, task, "routine run started");
    Ok(state)
}

/// One daemon tick: closes ended runs, starts the due ones in the background,
/// and clears what published runs left (ADR 0037 §2).
pub async fn tick(cx: &Context) {
    let config = Arc::clone(&cx.records);
    let due = tokio::task::spawn_blocking(move || scan(&config, now()))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the routine scan stopped");
            Vec::new()
        });
    for (workspace, name, date) in due {
        let cx2 = cx.clone();
        cx.pending.spawn(async move {
            match start(&cx2, &workspace, &name, When::Scheduled(date)).await {
                Ok(_) | Err(StartError::Handled | StartError::Busy(_)) => {}
                Err(e) => tracing::info!(routine = %name, error = %e, "scheduled run not started"),
            }
        });
    }
    super::cleanup::tick(cx).await;
}

/// Blocking. The due `(workspace, name, date)`s after closing ended runs.
fn scan(config: &Config, at: u64) -> Vec<(String, String, Date)> {
    let Some(state_dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Vec::new();
    };
    let workspaces = match load(config) {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };
    if workspaces.routines().is_empty() {
        return Vec::new();
    }
    let (store, records) = match (RoutineStore::open(state_dir), Store::open(state_dir)) {
        (Ok(s), Ok(r)) => (s, r),
        (Err(e), _) => {
            tracing::warn!(error = %e, "cannot open routine state");
            return Vec::new();
        }
        (_, Err(e)) => {
            tracing::warn!(error = %e, "cannot open task records for routines");
            return Vec::new();
        }
    };
    let Ok(now_ts) = jiff::Timestamp::from_second(at as i64) else {
        return Vec::new();
    };
    let mut due = Vec::new();
    for routine in workspaces.routines() {
        let state = match store.get(&routine.workspace, &routine.name) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(routine = %routine.name, error = %e, "cannot read routine state");
                continue;
            }
        };
        if state.running.is_some() {
            reconcile(&store, &records, routine, &state, at);
            continue;
        }
        let Some(trigger) = &routine.trigger else {
            continue;
        };
        let decision = match schedule::decide(trigger, now_ts, last_date(&state)) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(routine = %routine.name, error = %e, "cannot schedule");
                continue;
            }
        };
        match decision.run {
            Some(date) if routine.enabled => {
                due.push((routine.workspace.clone(), routine.name.clone(), date))
            }
            // A disabled Routine moves on too, so enabling it later runs no catch-up.
            _ if last_date(&state) != Some(decision.last) => {
                if !routine.enabled && state.last_occurrence.is_none() {
                    continue;
                }
                let saved = store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
                    s.last_occurrence = Some(decision.last.to_string());
                    Ok(())
                });
                if let Err(e) = saved {
                    tracing::warn!(routine = %routine.name, error = %e, "cannot save routine state");
                }
            }
            _ => {}
        }
    }
    due
}

/// Closes the open run when its record ended: a publish closes it as a
/// success; anything else is the Routine's last failure.
fn reconcile(
    store: &RoutineStore,
    records: &Store,
    routine: &Routine,
    state: &RoutineState,
    at: u64,
) {
    let Some(open) = &state.running else { return };
    let ended = match &open.task {
        None if at.saturating_sub(open.started_at) > START_LIMIT => {
            Some((None, "The run did not start.".to_owned()))
        }
        None => None,
        Some(task) => match records.get(task) {
            Err(StoreError::NotFound(_)) => Some((
                Some(task.clone()),
                "The run's record is missing.".to_owned(),
            )),
            Err(e) => {
                tracing::debug!(task, error = %e, "cannot read a routine run");
                None
            }
            Ok(r) => ended(&r).map(|why| (Some(task.clone()), why)),
        },
    };
    let Some((task, why)) = ended else { return };
    if let Some(id) = &task {
        drop_run_with(records, id, &why);
    }
    let occurrence = open.occurrence.clone();
    let saved = store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
        if s.running.as_ref().map(|r| (&r.occurrence, &r.task)) != Some((&occurrence, &task)) {
            return Ok(());
        }
        s.running = None;
        s.last_failure = Some(RoutineFailure {
            at,
            occurrence: occurrence.clone(),
            task: task.clone(),
            reason: why.clone(),
        });
        Ok(())
    });
    match saved {
        Ok(()) => {
            tracing::info!(routine = %routine.name, task, "routine run ended without a publish")
        }
        Err(e) => tracing::warn!(routine = %routine.name, error = %e, "cannot close a routine run"),
    }
}

const NOT_STARTED: &str = "The start prompt was not typed: the agent did not become idle \
                           within 30 seconds. Open the run's session to see why.";

/// Why the run's record says it ended, or None while it may still publish.
fn ended(r: &crate::records::Record) -> Option<String> {
    match r.status {
        Status::Done => return Some("The run ended without a publish.".into()),
        Status::Dropped => return Some("The run was dropped.".into()),
        Status::Open | Status::Review => {}
    }
    match r.task_loop() {
        Ok(Some(lp)) if lp.never_started() => Some(NOT_STARTED.into()),
        Ok(Some(lp)) if lp.state == LoopState::Stopped => Some(format!(
            "The output check stopped: {}.",
            lp.reason.map_or("unknown".to_owned(), String::from)
        )),
        Ok(Some(_)) => None,
        // No check means the start never completed.
        Ok(None) if r.pending_prompt().is_none() && !r.runs.is_empty() => {
            Some("The run has no output check.".into())
        }
        Ok(None) => None,
        Err(_) => Some("The run's check is unreadable.".into()),
    }
}

fn drop_run(config: &Config, task: &str, note: &str) {
    match config.state_dir.as_ref().map(Store::open) {
        Some(Ok(records)) => drop_run_with(&records, task, note),
        Some(Err(e)) => tracing::warn!(task, error = %e, "cannot open task records"),
        None => {}
    }
}

/// The run's record ends as `dropped`, with why. A done run stays done.
fn drop_run_with(records: &Store, task: &str, note: &str) {
    let Ok(r) = records.get(task) else { return };
    if matches!(r.status, Status::Done | Status::Dropped) || !r.is_routine_run() {
        return;
    }
    if let Err(e) = records.update(task, r.rev, |r| {
        r.status = Status::Dropped;
        r.note = Some(note.to_owned());
    }) {
        tracing::warn!(task, error = %e, "cannot close a routine run's record");
    }
}

pub fn last_date(state: &RoutineState) -> Option<Date> {
    state.last_occurrence.as_deref()?.parse().ok()
}

pub(super) fn load(config: &Config) -> Result<Workspaces, StartError> {
    let Some(path) = &config.workspaces else {
        return Ok(Workspaces::default());
    };
    Workspaces::load(path, config.home.as_deref()).map_err(|e| {
        tracing::warn!(error = %e, "cannot load workspaces for routines");
        StartError::Failed(format!("{e}. Fix the workspaces file on the host."))
    })
}

pub(super) fn dir(config: &Config) -> Result<std::path::PathBuf, StartError> {
    config.state_dir.clone().ok_or_else(|| {
        StartError::Unsupported(
            "this host keeps no task records: HOME is not set; pass --state-dir".into(),
        )
    })
}
