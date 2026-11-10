//! A workspace's staff at run time (ADR 0051 §1, §2, §5): which member a
//! request runs as, what `inbox-list` and a brief say about the staff, and
//! which notes file a member keeps. Logs carry worker names, never
//! descriptions.
use muxa_protocol::{ErrorBody, ErrorCode, StaffMember};

use super::notes::{self, Place};
use crate::convert::error_body;
use crate::records::Workspaces;
use crate::records::assistant::Delegate;
use crate::records::workspaces::Workspace;

/// One member, as a brief and `inbox-list` show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub worker: String,
    /// The worker's job description, at most 1 KiB.
    pub description: String,
    pub lead: bool,
}

/// The staff of `workspace`, in configuration order; empty without `staff`.
pub fn members(workspaces: &Workspaces, workspace: &str) -> Vec<Member> {
    let Some(d) = workspaces.get(workspace).and_then(|w| w.assistant.as_ref()) else {
        return Vec::new();
    };
    d.staff
        .members
        .iter()
        .map(|name| Member {
            worker: name.clone(),
            description: workspaces
                .worker(name)
                .and_then(|w| w.description.clone())
                .unwrap_or_default(),
            lead: d.staff.lead.as_deref() == Some(name.as_str()),
        })
        .collect()
}

/// The worker a request runs as: `named`, which must be on the staff, else
/// the lead. None: the workspace's agent.
pub fn pick(
    d: &Delegate,
    workspace: &str,
    named: Option<&str>,
) -> Result<Option<String>, ErrorBody> {
    let Some(name) = named else {
        return Ok(d.lead().map(str::to_owned));
    };
    if d.staff.members.iter().any(|m| m == name) {
        return Ok(Some(name.to_owned()));
    }
    // A staff of one may be named: it is the delegate of ADR 0048.
    if d.staff.members.is_empty() && d.worker.as_deref() == Some(name) {
        return Ok(Some(name.to_owned()));
    }
    let message = if d.staff.members.is_empty() {
        format!("workspace {workspace:?} has no staff; ask it without a worker")
    } else {
        format!(
            "worker {name:?} is not on the staff of workspace {workspace:?}: {}",
            d.staff.members.join(", ")
        )
    };
    Err(error_body(ErrorCode::BadRequest, message))
}

/// The notes file `worker` keeps in a workspace with this delegate: the
/// lead keeps the workspace's, every other member its own.
pub fn notes_of<'a>(d: &Delegate, worker: Option<&'a str>) -> Place<'a> {
    match worker {
        Some(w) if !d.staff.members.is_empty() && d.lead() != Some(w) => Place::Member(w),
        _ => Place::Workspace,
    }
}

/// Blocking. `staff[]` of a delegate in `inbox-list`: reads each member's
/// notes time, never its text.
pub fn listed(workspaces: &Workspaces, w: &Workspace) -> Vec<StaffMember> {
    let Some(d) = w.assistant.as_ref() else {
        return Vec::new();
    };
    members(workspaces, &w.name)
        .into_iter()
        .map(|m| {
            let place = notes_of(d, Some(&m.worker));
            let notes_at = notes::modified_at(&w.root, place).unwrap_or_else(|e| {
                tracing::warn!(workspace = %w.name, worker = %m.worker, error = %e, "cannot read the time of a member's notes");
                None
            });
            StaffMember {
                worker: m.worker,
                description: m.description,
                lead: m.lead,
                notes_at,
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "staff_tests.rs"]
mod tests;
