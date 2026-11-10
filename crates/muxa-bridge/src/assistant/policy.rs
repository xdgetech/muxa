//! Who runs a request and how many runs may start (ADR 0048 §7, §8). Checked
//! before any run folder exists. A refusal names its reason; a budget's names
//! the limit and when it resets.
use muxa_protocol::{ErrorBody, ErrorCode};

use crate::convert::error_body;
use crate::records::assistant::{AssistantHost, Delegate};
use crate::records::workers::{LaunchCommand, WorkerChoice};
use crate::records::{Record, Status, Workspaces};
use crate::task_records::reading;

/// The program of the one adapter with automatic delivery and finished turns.
const CLAUDE: &str = "claude";
const CODEX: &str = "codex";

/// The workspace's delegate, as a run starts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegated {
    pub workspace: String,
    /// None: the workspace's default agent.
    pub worker: Option<String>,
    pub delegate: Delegate,
    pub host: AssistantHost,
}

/// The delegate of `workspace`, or why it takes no request. `fallback` is
/// `--task-command`.
pub fn delegated(
    workspaces: &Workspaces,
    workspace: &str,
    fallback: &str,
) -> Result<Delegated, ErrorBody> {
    if workspaces.get(workspace).is_none() {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!("no workspace {workspace:?}; list-tasks shows the host's workspaces"),
        ));
    }
    if !workspaces.assistant().enabled {
        return Err(error_body(
            ErrorCode::Unsupported,
            "this host takes no assistant requests: set [assistant] enabled = true in \
             workspaces.toml on the host",
        ));
    }
    if workspaces.overall().is_some_and(|w| w.name == workspace) {
        return Err(error_body(
            ErrorCode::Unsupported,
            format!(
                "workspace {workspace:?} is the overall assistant's; it takes no requests. \
                 Ask another workspace"
            ),
        ));
    }
    let delegate = workspaces.delegate(workspace).cloned().ok_or_else(|| {
        error_body(
            ErrorCode::Unsupported,
            format!(
                "workspace {workspace:?} takes no assistant requests: set \
                 [workspace.assistant] delegate = true for it in workspaces.toml on the host"
            ),
        )
    })?;
    // Without `staff`, the lead is `worker`, else the workspace's agent (ADR 0051 §1).
    let choice = match delegate.lead() {
        Some(name) => WorkerChoice::Named(name),
        None => WorkerChoice::WorkspaceDefault,
    };
    let resolved = workspaces
        .resolve_worker(workspace, choice, None, fallback)
        .map_err(|e| error_body(ErrorCode::BadRequest, e.to_string()))?;
    check_agent(workspace, &resolved.command)?;
    Ok(Delegated {
        workspace: workspace.to_owned(),
        worker: resolved.worker,
        delegate,
        host: workspaces.assistant(),
    })
}

/// The overall assistant, as a plan run starts it (ADR 0049 §2, §5), or why
/// this host makes no plans.
pub fn overall(workspaces: &Workspaces, fallback: &str) -> Result<Delegated, ErrorBody> {
    if !workspaces.assistant().enabled {
        return Err(error_body(
            ErrorCode::Unsupported,
            "this host makes no plans: set [assistant] enabled = true in workspaces.toml on the host",
        ));
    }
    let (w, delegate) = workspaces
        .overall()
        .and_then(|w| Some((w, w.assistant.clone()?)))
        .ok_or_else(|| {
            error_body(
                ErrorCode::Unsupported,
                "this host has no overall assistant: set [workspace.assistant] overall = true \
                 for one workspace in workspaces.toml on the host",
            )
        })?;
    let choice = match delegate.worker.as_deref() {
        Some(name) => WorkerChoice::Named(name),
        None => WorkerChoice::WorkspaceDefault,
    };
    let resolved = workspaces
        .resolve_worker(&w.name, choice, None, fallback)
        .map_err(|e| error_body(ErrorCode::BadRequest, e.to_string()))?;
    check_agent(&w.name, &resolved.command)?;
    Ok(Delegated {
        workspace: w.name.clone(),
        worker: resolved.worker,
        delegate,
        host: workspaces.assistant(),
    })
}

/// A run needs automatic prompt delivery and a finished turn: Claude Code today.
fn check_agent(workspace: &str, command: &LaunchCommand) -> Result<(), ErrorBody> {
    match reading::program(command) {
        Some(CLAUDE) => Ok(()),
        Some(CODEX) => Err(error_body(
            ErrorCode::Unsupported,
            format!(
                "workspace {workspace:?} delegates to Codex, which has no automatic prompt \
                 delivery yet (ADR 0028). Set [workspace.assistant] worker to a Claude Code worker."
            ),
        )),
        _ => Err(error_body(
            ErrorCode::Unsupported,
            format!(
                "workspace {workspace:?} delegates to an agent with no muxa adapter, so a \
                 request would never be delivered. Set [workspace.assistant] worker to a \
                 Claude Code worker."
            ),
        )),
    }
}

/// What counts against a workspace's budgets now.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// The workspace's open runs and runs being created.
    pub open: u32,
    /// Runs the workspace started today.
    pub today: u32,
    /// Runs the host started today.
    pub today_host: u32,
}

/// A run being created: a request's or a plan's fresh claim, before its
/// record exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Starting {
    /// `q-…` or `p-…`.
    pub request: String,
    pub workspace: String,
    pub since: u64,
}

/// Counts runs from `day_start`. The runs of `replacing`, requests getting a
/// new version or plans a new plan replaces, are not open: the new run ends them.
pub fn usage(
    records: &[Record],
    starting: &[Starting],
    workspace: &str,
    replacing: &[&str],
    day_start: u64,
) -> Usage {
    let mut u = Usage::default();
    let mut count = |ws: &str, open: bool, at: u64| {
        if at >= day_start {
            u.today_host += 1;
            if ws == workspace {
                u.today += 1;
            }
        }
        if open && ws == workspace {
            u.open += 1;
        }
    };
    for r in records {
        let Some(run) = r.assistant_run() else {
            continue;
        };
        let open =
            matches!(r.status, Status::Open | Status::Review) && !replacing.contains(&run.source());
        count(&r.workspace, open, r.created_at);
    }
    for s in starting {
        count(
            &s.workspace,
            !replacing.contains(&s.request.as_str()),
            s.since,
        );
    }
    u
}

/// Refuses a start past a limit, with `conflict`. `reset` says when the day ends.
pub fn check(u: Usage, d: &Delegated, reset: &str) -> Result<(), ErrorBody> {
    let ws = &d.workspace;
    let refuse = |m: String| Err(error_body(ErrorCode::Conflict, m));
    if u.open >= d.delegate.max_open_runs {
        return refuse(format!(
            "workspace {ws:?} has {} open assistant runs, its max_open_runs. It frees when a \
             run ends or is cancelled.",
            u.open
        ));
    }
    if u.today >= d.delegate.max_runs_per_day {
        return refuse(format!(
            "workspace {ws:?} started {} assistant runs today, its max_runs_per_day. The count \
             resets at {reset}.",
            u.today
        ));
    }
    if u.today_host >= d.host.max_runs_per_day {
        return refuse(format!(
            "this host started {} assistant runs today, its [assistant] max_runs_per_day. The \
             count resets at {reset}.",
            u.today_host
        ));
    }
    Ok(())
}

/// The start of `at`'s day in the host's time zone, and its end as text.
pub fn day(at: u64) -> (u64, String) {
    let zone = jiff::tz::TimeZone::system();
    let start = i64::try_from(at)
        .ok()
        .and_then(|s| jiff::Timestamp::from_second(s).ok())
        .map(|ts| ts.to_zoned(zone))
        .and_then(|z| z.start_of_day().ok());
    let Some(start) = start else {
        return (at, "midnight".into());
    };
    let next = start
        .tomorrow()
        .ok()
        .and_then(|z| z.start_of_day().ok())
        .map_or_else(
            || "midnight".into(),
            |z| z.strftime("%Y-%m-%d %H:%M %Z").to_string(),
        );
    (
        u64::try_from(start.timestamp().as_second()).unwrap_or(0),
        next,
    )
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
