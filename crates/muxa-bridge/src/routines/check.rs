//! The Loop check of a Routine run (ADR 0026 section 3): muxa's own
//! validation of the output stage, and the publish when it passes. It runs in
//! the daemon's process, not as a shell command. The reasons go to
//! `.muxa/checks/<attempt>.log` for the agent; host logs get counts only.
use std::path::Path;
use std::time::Instant;

use super::output::{self, PublishError};
use super::runs;
use super::store::{RoutineStore, StateError};
use crate::config::Config;
use crate::loops::CheckEnd;
use crate::loops::check::{REPORT, log_path};
use crate::records::now;

/// A Routine run by its parts: no task record needed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunAt {
    pub workspace: String,
    pub name: String,
    pub occurrence: String,
    /// The run's id, as the Routine's open run names it.
    pub id: String,
    pub dir: std::path::PathBuf,
}

/// Blocking. Checks and publishes a run's output; exit 0 once the output
/// is published (also when it was already).
pub fn run_at(config: &Config, run: &RunAt, attempt: u32) -> CheckEnd {
    let started = Instant::now();
    let result = publish(config, run);
    let seconds = started.elapsed().as_secs();
    let (code, lines) = match &result {
        Ok(n) => (0, vec![format!("passed: {n} new Inbox entries stored.")]),
        Err(reasons) => (1, reasons.clone()),
    };
    match &result {
        Ok(n) => {
            tracing::info!(task = %run.id, attempt, entries = n, "routine output published")
        }
        Err(r) => {
            tracing::info!(task = %run.id, attempt, reasons = r.len(), "routine output refused")
        }
    }
    if let Err(e) = report(&run.dir, attempt, code == 0, &lines, "the Routine output") {
        tracing::warn!(task = %run.id, error = %e, "cannot write the routine check log");
    }
    CheckEnd::Exited { code, seconds }
}

fn publish(config: &Config, run: &RunAt) -> Result<u32, Vec<String>> {
    let one = |r: String| vec![r];
    let workspaces = runs::load(config).map_err(|e| one(e.to_string()))?;
    let routine = workspaces
        .routine(&run.workspace, &run.name)
        .ok_or_else(|| one("The Routine is no longer in workspaces.toml.".into()))?;
    let out = output::read(&run.dir, routine.items)?;
    let state_dir = runs::dir(config).map_err(|e| one(e.to_string()))?;
    let store = RoutineStore::open(&state_dir).map_err(|e| one(e.to_string()))?;
    let at = now();
    let stored = store.update::<_, Failure>(&run.workspace, &run.name, |s| {
        output::publish(s, &run.id, &run.occurrence, out, at, output::entry_id)
            .map_err(Failure::Publish)
    });
    match stored {
        Ok(n) => Ok(n),
        Err(Failure::Publish(PublishError::NotCurrent(_))) => Err(one(
            "This run is no longer the Routine's open run; its output is not published.".into(),
        )),
        Err(e) => {
            tracing::warn!(task = %run.id, error = %e, "cannot store routine output");
            Err(one(
                "muxa could not store the output. It checks again after your next turn.".into(),
            ))
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error(transparent)]
    Publish(PublishError),
    #[error(transparent)]
    State(#[from] StateError),
}

/// `.muxa/checks/<attempt>.log` and `.muxa/CHECK.md`, as a shell check
/// leaves them. `check` names what was checked.
pub(crate) fn report(
    dir: &Path,
    attempt: u32,
    passed: bool,
    lines: &[String],
    check: &str,
) -> std::io::Result<()> {
    let log = log_path(dir, attempt);
    if let Some(parent) = log.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&log, &text)?;
    let result = if passed { "passed" } else { "failed" };
    std::fs::write(
        dir.join(REPORT),
        format!(
            "# Check\n\n- Attempt: {attempt}\n- Result: {result}\n- Check: {check}\n\n```\n{text}```\n"
        ),
    )
}
