//! Today rows that say what to do (ADR 0059), at `work` level 17: the
//! secretary takes a message about a Work item, and `inbox-list` says how
//! many overall runs a Re-rank has left today.
use serde::{Deserialize, Serialize};

/// Level 17: `start-session` with `root` takes `text` and `work`, and
/// `assistant.overallRuns` in `inbox-list`.
pub const WORK_TODAY_ACTIONS: u32 = 17;

/// The overall workspace's runs today, for "N of M runs left today".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunBudget {
    /// Runs a plan may still start today: the smaller of what the overall
    /// workspace's and the host's `max_runs_per_day` leave.
    pub left: u32,
    /// The overall workspace's `max_runs_per_day`.
    pub max: u32,
}
