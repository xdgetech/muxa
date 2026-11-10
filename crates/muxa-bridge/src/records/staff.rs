//! `staff`, `lead`, `handoffs` and `max_steps` in `[workspace.assistant]`
//! (ADR 0051 §1, §3): which host workers answer a workspace's requests, and
//! how a request passes from one to the next. Parsing and the checks against
//! `[[worker]]` only; `assistant/` runs them. The list lives in host
//! configuration because a member is a grant of a worker's permissions.
use super::workers::{Worker, WorkerAgent};

/// Members of one staff.
pub const MAX_STAFF: usize = 8;
pub const DEFAULT_MAX_STEPS: u32 = 3;
pub const MAX_STEPS: u32 = 5;
/// `auto` steps across the host in one day.
pub const DEFAULT_AUTO_HANDOFFS_PER_DAY: u32 = 10;
/// Bytes: the job description of a worker on a staff, as of every worker.
pub const MAX_DESCRIPTION: usize = 1024;

/// What happens when a step's result asks to hand the request on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Handoffs {
    /// The user approves each hand-off. A page one member read can plant a
    /// purpose the next would follow.
    #[default]
    Review,
    /// The host starts the next step, within every budget.
    Auto,
    /// A result with a hand-off fails the check.
    None,
}

/// A workspace's staff, as configured. Empty `members`: a staff of one, the
/// delegate's `worker` or the workspace's agent (ADR 0051 §1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Staff {
    /// Worker names, in configuration order.
    pub members: Vec<String>,
    /// One of `members`; None without `members`.
    pub lead: Option<String>,
    pub handoffs: Handoffs,
    pub max_steps: u32,
}

impl Staff {
    /// No `staff`: the delegate of ADR 0048 alone.
    pub fn of_one() -> Self {
        Self {
            max_steps: DEFAULT_MAX_STEPS,
            ..Self::default()
        }
    }
}

/// The raw fields, as `RawDelegate` reads them.
#[derive(Debug, Default)]
pub(super) struct RawStaff {
    pub staff: Option<Vec<String>>,
    pub lead: Option<String>,
    pub handoffs: Option<String>,
    pub max_steps: Option<i64>,
}

impl RawStaff {
    /// The shape of each field. The workers themselves are checked by
    /// [`check`] once every `[[worker]]` is read.
    pub(super) fn parse(self, overall: bool, worker: Option<&str>) -> Result<Staff, String> {
        if overall {
            if self.staff.is_some()
                || self.lead.is_some()
                || self.handoffs.is_some()
                || self.max_steps.is_some()
            {
                return Err(
                    "staff, lead, handoffs and max_steps do not apply to the overall workspace"
                        .into(),
                );
            }
            return Ok(Staff::of_one());
        }
        let handoffs = match self.handoffs.as_deref() {
            None | Some("review") => Handoffs::Review,
            Some("auto") => Handoffs::Auto,
            Some("none") => Handoffs::None,
            Some(_) => return Err("handoffs must be \"review\", \"auto\" or \"none\"".into()),
        };
        let max_steps = match self.max_steps {
            None => DEFAULT_MAX_STEPS,
            Some(n) => u32::try_from(n)
                .ok()
                .filter(|n| (1..=MAX_STEPS).contains(n))
                .ok_or_else(|| format!("max_steps must be a whole number from 1 to {MAX_STEPS}"))?,
        };
        let Some(members) = self.staff else {
            if self.lead.is_some() {
                return Err("lead needs staff: list the workers in staff first".into());
            }
            return Ok(Staff {
                handoffs,
                max_steps,
                ..Staff::of_one()
            });
        };
        if worker.is_some() {
            return Err(
                "staff and worker are both set: list the worker in staff and name it lead".into(),
            );
        }
        if members.is_empty() {
            return Err(
                "staff is empty: list at least one worker, or remove staff to use the \
                 workspace's agent"
                    .into(),
            );
        }
        if members.len() > MAX_STAFF {
            return Err(format!("staff has more than {MAX_STAFF} workers"));
        }
        for (i, name) in members.iter().enumerate() {
            segment(name).map_err(|r| format!("staff worker {name:?} {r}"))?;
            if members[..i].contains(name) {
                return Err(format!("staff worker {name:?} appears twice"));
            }
        }
        let lead = match self.lead {
            Some(lead) if !members.contains(&lead) => {
                return Err(format!(
                    "lead {lead:?} is not in staff: name one of the staff workers"
                ));
            }
            Some(lead) => lead,
            None => members[0].clone(),
        };
        Ok(Staff {
            members,
            lead: Some(lead),
            handoffs,
            max_steps,
        })
    }
}

/// Each member must be a Claude Code `[[worker]]` with a description that
/// this workspace may use (ADR 0048 §7, ADR 0051 §1).
pub(super) fn check(staff: &Staff, workspace: &str, workers: &[Worker]) -> Result<(), String> {
    for name in &staff.members {
        let w = workers
            .iter()
            .find(|w| w.name == *name)
            .ok_or_else(|| format!("staff worker {name:?} is not configured"))?;
        if !w.allows(workspace) {
            return Err(format!(
                "staff worker {name:?} is not allowed in this workspace: add it to the \
                 worker's workspaces"
            ));
        }
        if w.agent != WorkerAgent::ClaudeCode {
            return Err(format!(
                "staff worker {name:?} runs {}, which has no automatic prompt delivery yet \
                 (ADR 0028); staff must be Claude Code workers",
                w.agent.id()
            ));
        }
        if w.description.is_none() {
            return Err(format!(
                "staff worker {name:?} has no description: write its job description in the \
                 worker's description"
            ));
        }
    }
    Ok(())
}

/// One path segment, because a member names its notes folder: ASCII letters,
/// digits, `-`, `_` and `.`, not `.` or `..`.
fn segment(name: &str) -> Result<(), &'static str> {
    let allowed = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.');
    if name.is_empty() || name == "." || name == ".." || !name.bytes().all(allowed) {
        return Err(
            "is not a staff name: use ASCII letters, digits, '-', '_' and '.', not '.' or '..'",
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "staff_tests.rs"]
mod tests;
