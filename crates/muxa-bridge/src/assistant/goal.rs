//! What a request's run starts with (ADR 0048 §4, ADR 0051 §5): its goal,
//! the worker that runs it, and the notes it reads. A member who is not the
//! lead reads the workspace's notes as data and proposes only its own.
use muxa_protocol::{AssistantRequest, ErrorBody, ErrorCode, WorkItem};

use super::brief::Brief;
use super::notes::{self, Place};
use super::record::RequestRun;
use super::staff_brief::{Earlier, StaffBrief};
use super::{staff, steps};
use crate::convert::error_body;
use crate::records::Workspaces;
use crate::task_records::Config;

/// A run ready to be made.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub goal: String,
    /// None: the workspace's agent.
    pub worker: Option<String>,
    pub run: RequestRun,
}

/// Blocking. The run of `q`'s newest step. `delegate` is the worker a
/// request without steps runs as. An invalid notes file fails here, before
/// any record or session (ADR 0049 §1).
pub fn prepare(
    config: &Config,
    q: &AssistantRequest,
    delegate: Option<&str>,
) -> Result<Prepared, ErrorBody> {
    let item = super::shelf::<WorkItem>(config)?
        .get(&q.work)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(|| bad(format!("no Work item {}", q.work)))?;
    let workspaces = config.load_workspaces()?;
    let ws = workspaces
        .get(&q.workspace)
        .ok_or_else(|| bad(format!("no workspace {}", q.workspace)))?;
    let step = steps::current(q);
    let worker = q
        .steps
        .last()
        .map(|s| s.worker.clone())
        .or_else(|| delegate.map(str::to_owned));
    let place = ws
        .assistant
        .as_ref()
        .map_or(Place::Workspace, |d| staff::notes_of(d, worker.as_deref()));
    let refused = |e: notes::NotesError| super::apply::refused(&e);
    let own = notes::read_at(&ws.root, place).map_err(refused)?;
    let (member, shared) = match place {
        Place::Member(w) => (
            Some(w.to_owned()),
            Some(notes::read(&ws.root).map_err(refused)?.base),
        ),
        Place::Workspace => (None, None),
    };
    let mut brief = Brief::gather(config, &item, q)?;
    brief.staff = staff_brief(&workspaces, q, worker.as_deref(), step);
    if brief.staff.is_some() {
        // This version's earlier steps are in the staff section, cut.
        brief.results.retain(|r| r.version != q.version);
    }
    let run = RequestRun {
        request: q.id.clone(),
        version: q.version,
        work: q.work.clone(),
        notes_base: Some(own.base),
        step: (!q.steps.is_empty()).then_some(step),
        member,
        shared_base: shared,
    };
    Ok(Prepared {
        goal: brief.goal(),
        worker,
        run,
    })
}

/// What the goal says about the staff; None for a request without steps.
fn staff_brief(
    workspaces: &Workspaces,
    q: &AssistantRequest,
    worker: Option<&str>,
    step: u32,
) -> Option<StaffBrief> {
    let worker = worker.filter(|_| !q.steps.is_empty())?;
    let members = staff::members(workspaces, &q.workspace);
    let description = members
        .iter()
        .find(|m| m.worker == worker)
        .map(|m| m.description.clone())
        .unwrap_or_default();
    let max_steps = workspaces
        .get(&q.workspace)
        .and_then(|w| w.assistant.as_ref())
        .map_or(1, |d| d.staff.max_steps);
    let mut earlier: Vec<Earlier> = q
        .results
        .iter()
        .filter(|r| r.version == q.version && r.step.unwrap_or(1) < step)
        .map(|r| Earlier {
            step: r.step.unwrap_or(1),
            worker: r.worker.clone().unwrap_or_default(),
            summary: r.summary.clone(),
        })
        .collect();
    earlier.sort_by_key(|e| e.step);
    let asked = q
        .steps
        .iter()
        .find(|s| s.step + 1 == step)
        .and_then(|s| Some((s.worker.clone(), s.handoff.as_ref()?.purpose.clone())));
    Some(StaffBrief {
        worker: worker.to_owned(),
        description,
        members,
        step,
        max_steps,
        earlier,
        asked,
    })
}

fn bad(message: String) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message)
}

#[cfg(test)]
#[path = "goal_tests.rs"]
mod tests;
