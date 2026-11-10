//! The overall run's brief on disk (ADR 0049 §3, §5, ADR 0050 §4). It is longer than a
//! goal may be, so the host writes it to `.muxa/assistant/BRIEF.md` in the
//! run folder before the agent starts, and the goal points to it. The host
//! reads the inputs then, and stamps `inputsAt` and `workspaces` on the plan.
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use muxa_protocol::{Capture, Delivery, Hypothesis, Plan};

use super::cite::Carried;
use crate::assistant::corrections;
use crate::assistant::memory::{self, Memory, Records};
use crate::assistant::overall_brief::{OverallBrief, marker};
use crate::assistant::result::STAGE_DIR;
use crate::assistant::share::{Input, shared};
use crate::records::Workspaces;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// Relative to the run folder.
pub const BRIEF: &str = ".muxa/assistant/BRIEF.md";
const FILE_MODE: u32 = 0o600;

/// The run's goal: where the brief is, then the marker that finds the run
/// again after a crash. Ids only.
pub fn goal(plan: &str) -> String {
    format!(
        "You are the user's overall assistant. Read {BRIEF} in this folder: it is your \
         brief. Follow it: write {} and end your turn.\n\n{}",
        super::file::PLAN,
        marker(plan)
    )
}

/// Blocking. Writes the brief of `plan` into the run folder `dir`, then
/// stamps the plan. Errors name paths and counts, never content.
pub fn write(config: &Config, plan: &str, dir: &Path) -> Result<(), String> {
    let at = crate::records::now();
    let workspaces = config.load_workspaces().map_err(|e| e.message)?;
    let shelf = super::shelf(config).map_err(|e| e.message)?;
    let mut plans = shelf.all().map_err(|e| e.to_string())?;
    super::newest_first(&mut plans);
    let this = plans
        .iter()
        .find(|p| p.id == plan)
        .ok_or_else(|| format!("no plan {plan}"))?;
    let since = previous(&plans, plan).map(|p| p.inputs_at);
    let listing = crate::assistant::list(config).map_err(|e| e.message)?;
    let captures: Vec<Capture> = crate::assistant::shelf::<Capture>(config)
        .map_err(|e| e.message)?
        .all()
        .map_err(|e| e.to_string())?;
    let entries = crate::routines::inbox::list(config).map_err(|e| e.message)?;
    let items = crate::items::list(config, &entries).map_err(|e| e.message)?;
    let input = Input {
        work: &listing.work,
        captures: &captures,
        requests: &listing.requests,
        items: &items,
        now: at,
        since,
    };
    let part = shared(&workspaces, &input);
    let (told, slots) = memory(config, &workspaces, at)?;
    let [policies, hypotheses, _, corrections] = told.ids.clone();
    let carried = Carried {
        policies,
        hypotheses,
        corrections,
        slots,
    };
    let text = OverallBrief {
        plan: plan.to_owned(),
        minutes: this.minutes,
        shared: part.clone(),
        memory: told.text.clone(),
        slots,
    }
    .render();
    let path = dir.join(BRIEF);
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(&path)
        .and_then(|mut f| f.write_all(text.as_bytes()).and_then(|()| f.sync_all()));
    written.map_err(|e| format!("cannot write {}: {e}", dir.join(STAGE_DIR).display()))?;
    // On the plan record, where the run cannot write: what it may cite.
    shelf
        .update_with_extra(plan, |p, extra| -> Result<(), StateError> {
            p.inputs_at = at;
            p.workspaces = part.workspaces.clone();
            p.rev += 1;
            carried.put(extra);
            Ok(())
        })
        .map_err(|e| e.to_string())?;
    tracing::info!(
        plan,
        bytes = text.len(),
        workspaces = part.workspaces.len(),
        left_out = part.left_out,
        policies = told.held[0],
        hypotheses = told.held[1] + told.held[2],
        corrections = told.held[3],
        memory_left_out = told.left_out.iter().sum::<usize>(),
        slots,
        "overall brief written"
    );
    Ok(())
}

/// Blocking. "What the user told you", from the records live at `now`, and
/// the hypotheses the plan may propose.
fn memory(config: &Config, workspaces: &Workspaces, now: u64) -> Result<(Memory, usize), String> {
    let policies = crate::assistant::policies::list(config, now).map_err(|e| e.message)?;
    let hypotheses = crate::assistant::shelf::<Hypothesis>(config)
        .map_err(|e| e.message)?
        .all()
        .map_err(|e| e.to_string())?;
    let corrections = corrections::live(config, now).map_err(|e| e.message)?;
    let records = Records {
        policies: &policies,
        hypotheses: &hypotheses,
        corrections: &corrections,
        now,
    };
    let slots = crate::assistant::hypotheses::slots(&hypotheses, now);
    Ok((memory::memory(workspaces, &records), slots))
}

/// The newest plan older than `plan` that has an answer.
fn previous<'a>(newest_first: &'a [Plan], plan: &str) -> Option<&'a Plan> {
    newest_first
        .iter()
        .skip_while(|p| p.id != plan)
        .skip(1)
        .find(|p| p.delivery == Delivery::Resulted)
}

#[cfg(test)]
#[path = "brief_tests.rs"]
mod tests;
