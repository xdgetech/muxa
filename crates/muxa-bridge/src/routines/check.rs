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
    let mut out = output::read(&run.dir, routine.items)?;
    let bot = routine
        .bot
        .as_deref()
        .and_then(|b| workspaces.bot(&run.workspace, b));
    if bot.is_none() && out.memory.is_some() {
        return Err(one("memory is only for a Bot's Routine; remove it.".into()));
    }
    let handoffs = std::mem::take(&mut out.handoffs);
    if !handoffs.is_empty() {
        check_handoffs(bot, &handoffs, step(&run.dir))?;
    }
    let memory = out.memory.take();
    let state_dir = runs::dir(config).map_err(|e| one(e.to_string()))?;
    let store = RoutineStore::open(&state_dir).map_err(|e| one(e.to_string()))?;
    let at = now();
    let bot_name = bot.map(|b| b.name.as_str());
    let stored = store.update::<_, Failure>(&run.workspace, &run.name, |s| {
        output::publish(
            s,
            &run.id,
            &run.occurrence,
            out,
            bot_name,
            at,
            output::entry_id,
        )
        .map_err(Failure::Publish)
    });
    if let (Ok(_), Some(bot), Some(memory)) = (&stored, bot, memory) {
        // The output is already published; a memory that cannot be written
        // is lost for this run only.
        match bot.write_memory(&memory) {
            Ok(()) => tracing::info!(bot = %bot.name, bytes = memory.len(), "bot memory written"),
            Err(e) => tracing::warn!(bot = %bot.name, error = %e, "cannot write bot memory"),
        }
    }
    if stored.is_ok() && !handoffs.is_empty() {
        let text = serde_json::to_string(&handoffs).unwrap_or_default();
        if let Err(e) = std::fs::write(run.dir.join(HANDOFFS), text) {
            tracing::warn!(task = %run.id, error = %e, "cannot keep a lead's handoffs");
        }
    }
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

/// A passed lead run's handoffs, for the runner to start (ADR 0085).
pub const HANDOFFS: &str = ".muxa/routine/handoffs.json";

/// The lead's step this run is, from `.muxa/team-step`; 1 when missing.
pub fn step(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join(".muxa").join(crate::v2::runner::STEP_FILE))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1)
}

/// Only a team lead hands off, only to its members, and not in its last step.
fn check_handoffs(
    bot: Option<&crate::records::bots::Bot>,
    handoffs: &[output::Handoff],
    step: u32,
) -> Result<(), Vec<String>> {
    let Some(lead) = bot.filter(|b| !b.members.is_empty()) else {
        return Err(vec![
            "handoffs are only for a team lead's Routine; remove them.".into(),
        ]);
    };
    if step >= super::brief::MAX_STEPS {
        return Err(vec![format!(
            "this is step {step} of {}, the last; leave out handoffs.",
            super::brief::MAX_STEPS
        )]);
    }
    let mut reasons = Vec::new();
    for (i, h) in handoffs.iter().enumerate() {
        if !lead.members.contains(&h.to) {
            reasons.push(format!("handoffs[{i}].to is not one of your members."));
        } else if handoffs[..i].iter().any(|o| o.to == h.to) {
            reasons.push(format!(
                "handoffs[{i}].to repeats a member; one handoff each."
            ));
        }
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons)
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
