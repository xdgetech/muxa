//! The Routine schedule and the run claim (ADR 0026 section 2), shared with
//! the runner on the session backend (`v2::runner`). One open run per
//! Routine: the claim in its state file is taken before the run starts.

use jiff::civil::DateTime;
use muxa_protocol::{RoutineFailure, RoutineRun};

use super::schedule;
use super::store::{RoutineState, RoutineStore, StateError};
use crate::config::Config;
use crate::records::routines::Routine;
use crate::records::workers::WorkerAgent;
use crate::records::{Workspaces, now};

/// What starts a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Scheduled(DateTime),
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
    /// Unattended work while every subscription it may use is at its limit
    /// (ADR 0091 §9): not claimed, so a later tick starts it.
    #[error("every subscription it may use is at its limit until {0}")]
    Waiting(u64),
}

impl From<StateError> for StartError {
    fn from(e: StateError) -> Self {
        tracing::warn!(error = %e, "routine state failed");
        Self::Failed(e.to_string())
    }
}

/// Whether the Routine would start Codex: a Codex worker, or a default
/// command whose program is `codex`.
pub fn runs_codex(workspaces: &Workspaces, routine: &Routine, fallback: &str) -> bool {
    // A Bot's Routine always runs Claude Code (ADR 0087).
    if routine.bot.is_some() {
        return false;
    }
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
        (When::Scheduled(slot), _) => schedule::occurrence_id(slot),
        (When::Now(at), _) => format!("now-{at}"),
    }
}

pub(crate) fn claim(
    store: &RoutineStore,
    routine: &Routine,
    occurrence: &str,
    when: When,
) -> Result<(), StartError> {
    store.update(&routine.workspace, &routine.name, |s| {
        if let When::Scheduled(slot) = when {
            if last_slot(s).is_some_and(|last| last >= slot) {
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
        if let When::Scheduled(slot) = when {
            s.last_occurrence = Some(schedule::occurrence_id(slot));
        }
        Ok(())
    })
}

/// Keeps a failure that came before the claim; a scheduled occurrence counts
/// as handled, so a broken Routine does not retry every tick.
pub(crate) fn fail_claimless(
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
        if let When::Scheduled(slot) = when {
            s.last_occurrence = Some(schedule::occurrence_id(slot));
        }
        Ok(())
    });
    if let Err(e) = saved {
        tracing::warn!(routine = %routine.name, error = %e, "cannot keep a routine failure");
    }
    tracing::warn!(routine = %routine.name, occurrence, error = %error, "routine run did not start");
    error
}

pub(crate) fn finish_start(
    config: &Config,
    routine: &Routine,
    occurrence: &str,
    task: Option<String>,
    error: Option<String>,
) -> Result<RoutineState, StartError> {
    let store = RoutineStore::open(&dir(config)?)?;
    if let Some(why) = error {
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

/// Blocking. The due `(workspace, name, occurrence)`s; `open` sees each Routine
/// that has an open run, to close it when it ended.
pub(crate) fn scan_with(
    config: &Config,
    at: u64,
    mut open: impl FnMut(&RoutineStore, &Workspaces, &Routine, &RoutineState),
) -> Vec<(String, String, DateTime)> {
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
    let store = match RoutineStore::open(state_dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open routine state");
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
            open(&store, &workspaces, routine, &state);
            continue;
        }
        let Some(trigger) = &routine.trigger else {
            continue;
        };
        let decision = match schedule::decide(trigger, now_ts, last_slot(&state)) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(routine = %routine.name, error = %e, "cannot schedule");
                continue;
            }
        };
        match decision.run {
            Some(slot) if routine.enabled => {
                due.push((routine.workspace.clone(), routine.name.clone(), slot))
            }
            // A disabled Routine moves on too, so enabling it later runs no catch-up.
            _ if last_slot(&state) != Some(decision.last) => {
                if !routine.enabled && state.last_occurrence.is_none() {
                    continue;
                }
                let saved = store.update::<_, StateError>(&routine.workspace, &routine.name, |s| {
                    s.last_occurrence = Some(schedule::occurrence_id(decision.last));
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

pub fn last_slot(state: &RoutineState) -> Option<DateTime> {
    schedule::parse_occurrence(state.last_occurrence.as_deref()?)
}

pub(crate) fn load(config: &Config) -> Result<Workspaces, StartError> {
    let Some(path) = &config.workspaces else {
        return Ok(Workspaces::default());
    };
    Workspaces::load(path, config.home.as_deref()).map_err(|e| {
        tracing::warn!(error = %e, "cannot load workspaces for routines");
        StartError::Failed(format!("{e}. Fix the workspaces file on the host."))
    })
}

pub(crate) fn dir(config: &Config) -> Result<std::path::PathBuf, StartError> {
    config.state_dir.clone().ok_or_else(|| {
        StartError::Unsupported(
            "this host keeps no task records: HOME is not set; pass --state-dir".into(),
        )
    })
}
