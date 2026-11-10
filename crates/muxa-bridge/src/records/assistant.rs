//! `[assistant]` and `[workspace.assistant]` in `workspaces.toml` (ADR 0048
//! §1, ADR 0049 §2–§4): whether a workspace takes requests, which agent runs
//! them, what it shares with the overall assistant, and how many runs it may
//! have. Parsing only; `assistant/` runs them.
use muxa_protocol::{OverallRequests, Share};
use serde::Deserialize;

use super::routines::{Trigger, scheduled};
use super::workers::Worker;
use super::workspaces::parse_duration;

/// Runs a day across all workspaces when `[assistant]` gives no number.
pub const DEFAULT_HOST_RUNS_PER_DAY: u32 = 20;
/// Requests the overall assistant sends without approval, across all workspaces.
pub const DEFAULT_AUTO_REQUESTS_PER_DAY: u32 = 5;
pub const DEFAULT_OPEN_RUNS: u32 = 1;
pub const DEFAULT_RUNS_PER_DAY: u32 = 10;
/// Seconds: `"30m"`.
pub const DEFAULT_RUN_TIMEOUT: u64 = 30 * 60;
/// The overall workspace's own defaults (ADR 0049 §9).
pub const OVERALL_OPEN_RUNS: u32 = 1;
pub const OVERALL_RUNS_PER_DAY: u32 = 8;
/// Seconds: `"15m"`.
pub const OVERALL_RUN_TIMEOUT: u64 = 15 * 60;
/// The overall assistant's plan without `plans`: weekday mornings at 07:30,
/// in `timezone` or the host's own zone (ADR 0049 §5).
pub const DEFAULT_PLAN: &str = "30 7 * * 1-5";
/// Schedules in `plans`.
pub const MAX_PLANS: usize = 3;

/// The host's switch and budget (`[assistant]`). No table: off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssistantHost {
    pub enabled: bool,
    pub max_runs_per_day: u32,
    pub max_auto_requests_per_day: u32,
}

impl Default for AssistantHost {
    fn default() -> Self {
        Self {
            enabled: false,
            max_runs_per_day: DEFAULT_HOST_RUNS_PER_DAY,
            max_auto_requests_per_day: DEFAULT_AUTO_REQUESTS_PER_DAY,
        }
    }
}

/// How a workspace's assistant runs start and stop: a delegate's
/// (`delegate = true`) or the overall assistant's (`overall = true`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delegate {
    /// None: the workspace's default agent (ADR 0048 §7).
    pub worker: Option<String>,
    pub max_open_runs: u32,
    pub max_runs_per_day: u32,
    /// Seconds before a run without a result is `timed-out`.
    pub run_timeout: u64,
    /// Who writes a run's proposed notes to the live file (ADR 0049 §1).
    pub notes: NotesMode,
    /// When the overall assistant plans (ADR 0049 §5); empty for a delegate.
    pub plans: Vec<Trigger>,
}

/// What a workspace is to the overall assistant (ADR 0049 §2–§4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Role {
    /// No `[workspace.assistant]`: it shares nothing and is never asked.
    #[default]
    Absent,
    /// It has `[workspace.assistant]` without `overall`.
    Member {
        share: Share,
        overall_requests: OverallRequests,
    },
    /// The overall assistant's own workspace. It takes no requests.
    Overall,
}

/// `notes = "review" | "auto"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NotesMode {
    /// The user applies each proposal with `apply-notes`. A page or document
    /// a run read could plant text that every later run would trust.
    #[default]
    Review,
    /// The host applies a passing proposal at publish, while the base holds.
    Auto,
}

impl NotesMode {
    fn parse(value: Option<&str>) -> Result<Self, String> {
        match value {
            None | Some("review") => Ok(Self::Review),
            Some("auto") => Ok(Self::Auto),
            Some(_) => Err("notes must be \"review\" or \"auto\"".into()),
        }
    }
}

fn parse_share(value: &str) -> Result<Share, String> {
    match value {
        "none" => Ok(Share::None),
        "outline" => Ok(Share::Outline),
        "summaries" => Ok(Share::Summaries),
        _ => Err("share must be \"none\", \"outline\" or \"summaries\"".into()),
    }
}

fn parse_overall_requests(value: Option<&str>) -> Result<OverallRequests, String> {
    match value {
        None | Some("review") => Ok(OverallRequests::Review),
        Some("none") => Ok(OverallRequests::None),
        Some("auto") => Ok(OverallRequests::Auto),
        Some(_) => Err("overall_requests must be \"none\", \"review\" or \"auto\"".into()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawHost {
    enabled: bool,
    max_runs_per_day: Option<i64>,
    max_auto_requests_per_day: Option<i64>,
}

impl RawHost {
    pub(super) fn check(self) -> Result<AssistantHost, String> {
        Ok(AssistantHost {
            enabled: self.enabled,
            max_runs_per_day: positive(
                "[assistant] max_runs_per_day",
                self.max_runs_per_day,
                DEFAULT_HOST_RUNS_PER_DAY,
            )?,
            max_auto_requests_per_day: positive(
                "[assistant] max_auto_requests_per_day",
                self.max_auto_requests_per_day,
                DEFAULT_AUTO_REQUESTS_PER_DAY,
            )?,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawDelegate {
    delegate: Option<bool>,
    #[serde(default)]
    overall: bool,
    share: Option<String>,
    overall_requests: Option<String>,
    worker: Option<String>,
    max_open_runs: Option<i64>,
    max_runs_per_day: Option<i64>,
    run_timeout: Option<String>,
    notes: Option<String>,
    plans: Option<Vec<String>>,
    timezone: Option<String>,
}

impl RawDelegate {
    /// The run settings, None when the workspace runs nothing, and its role.
    /// Every field is checked anyway, so a typo fails before the day it is
    /// switched on.
    pub(super) fn check(self) -> Result<(Option<Delegate>, Role), String> {
        let (open, per_day, timeout) = if self.overall {
            (OVERALL_OPEN_RUNS, OVERALL_RUNS_PER_DAY, OVERALL_RUN_TIMEOUT)
        } else {
            (DEFAULT_OPEN_RUNS, DEFAULT_RUNS_PER_DAY, DEFAULT_RUN_TIMEOUT)
        };
        let settings = Delegate {
            worker: self
                .worker
                .filter(|name| name != muxa_protocol::DEFAULT_WORKER),
            max_open_runs: positive("max_open_runs", self.max_open_runs, open)?,
            max_runs_per_day: positive("max_runs_per_day", self.max_runs_per_day, per_day)?,
            run_timeout: self
                .run_timeout
                .as_deref()
                .map(parse_duration)
                .transpose()
                .map_err(|r| format!("run_timeout {r}"))?
                .unwrap_or(timeout),
            notes: NotesMode::parse(self.notes.as_deref())?,
            plans: Vec::new(),
        };
        let share = self.share.as_deref().map(parse_share).transpose()?;
        let overall_requests = parse_overall_requests(self.overall_requests.as_deref())?;
        if self.overall {
            // The overall assistant reads the others; nobody reads or asks it.
            if self.delegate == Some(true) {
                return Err(
                    "overall: the overall workspace takes no requests; remove delegate".into(),
                );
            }
            if self.share.is_some() || self.overall_requests.is_some() {
                return Err(
                    "overall: share and overall_requests do not apply to the overall workspace"
                        .into(),
                );
            }
            let plans = plans(self.plans.as_deref(), self.timezone.as_deref())?;
            return Ok((Some(Delegate { plans, ..settings }), Role::Overall));
        }
        if self.plans.is_some() || self.timezone.is_some() {
            return Err("plans and timezone apply only to the overall workspace".into());
        }
        let delegate = self.delegate.ok_or("delegate is required: true or false")?;
        let default_share = if delegate {
            Share::Summaries
        } else {
            Share::None
        };
        let role = Role::Member {
            share: share.unwrap_or(default_share),
            overall_requests,
        };
        Ok((delegate.then_some(settings), role))
    }
}

/// `plans` and `timezone`: at most [`MAX_PLANS`] schedules, [`DEFAULT_PLAN`]
/// without the field. An empty list plans on demand only.
fn plans(texts: Option<&[String]>, zone: Option<&str>) -> Result<Vec<Trigger>, String> {
    let default = [DEFAULT_PLAN.to_owned()];
    let texts = texts.unwrap_or(&default);
    if texts.len() > MAX_PLANS {
        return Err(format!("plans has more than {MAX_PLANS} schedules"));
    }
    let mut out: Vec<Trigger> = Vec::with_capacity(texts.len());
    for (i, text) in texts.iter().enumerate() {
        let t = scheduled(text, zone).map_err(|r| format!("plans[{i}]: {r}"))?;
        if out.iter().any(|o| o.schedule == t.schedule) {
            return Err(format!("plans[{i}] appears twice"));
        }
        out.push(t);
    }
    Ok(out)
}

/// The delegate's `worker` must be a configured `[[worker]]` this workspace may use.
pub(super) fn check_worker(
    delegate: &Delegate,
    workspace: &str,
    workers: &[Worker],
) -> Result<(), String> {
    let Some(name) = &delegate.worker else {
        return Ok(());
    };
    let found = workers
        .iter()
        .find(|w| w.name == *name)
        .ok_or("worker is not configured")?;
    if !found.allows(workspace) {
        return Err("worker is not allowed in this workspace".into());
    }
    Ok(())
}

/// TOML integers are signed; reading them as such lets the error name the field.
fn positive(field: &str, value: Option<i64>, default: u32) -> Result<u32, String> {
    match value {
        None => Ok(default),
        Some(n) => u32::try_from(n)
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| format!("{field} must be a whole number from 1 to {}", u32::MAX)),
    }
}

#[cfg(test)]
mod tests;
