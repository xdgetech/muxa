//! Routines and the stored Inbox entries they publish (ADR 0026). Gated by
//! the shared `work` capability at level 1 or higher (ADR 0027 section 5).
use serde::{Deserialize, Serialize};

/// The shared capability of the newer features. The host advertises it as
/// `work:<level>`; a client reads the level with [`work_level`].
pub const WORK_CAPABILITY: &str = "work";
/// Level 1: Routines (`list-routines`, `run-routine`) and stored Inbox
/// entries (`list-inbox`, `update-inbox`).
pub const WORK_ROUTINES: u32 = 1;
/// Level 2: repos relative to the workspace root, a branch from the title,
/// and `start-session` (ADR 0031).
pub const WORK_ASK: u32 = 2;
/// Level 4: reading Tasks, `branch-task` and `park-task` (ADR 0034).
pub const WORK_READ: u32 = 4;

/// `work:<level>` as `hello` carries it.
pub fn work_capability(level: u32) -> String {
    format!("{WORK_CAPABILITY}:{level}")
}

/// The highest `work` level in `capabilities`; 0 when the host has none.
pub fn work_level(capabilities: &[String]) -> u32 {
    capabilities
        .iter()
        .filter_map(|c| c.strip_prefix(WORK_CAPABILITY)?.strip_prefix(':'))
        .filter_map(|level| level.parse().ok())
        .max()
        .unwrap_or(0)
}

/// A Routine as the host's `workspaces.toml` defines it, with its schedule
/// state. `(workspace, name)` is its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineInfo {
    pub workspace: String,
    pub name: String,
    /// Absent: the Routine runs only on `run-routine`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<RoutineTrigger>,
    /// Nothing runs, also not `run-routine`, while this is false.
    pub enabled: bool,
    /// A worker name; absent is the workspace default agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    /// The label stamped on Tasks the Routine's proposals create.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// Epoch seconds of the next scheduled run; absent without a trigger or while disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_due: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<RoutineSuccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<RoutineFailure>,
    /// The open run. At most one runs at a time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<RoutineRun>,
}

/// A local wall-clock time on the days of a schedule, in an IANA time zone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineTrigger {
    /// `HH:MM`, 24-hour.
    pub at: String,
    /// e.g. `Europe/Berlin`.
    pub timezone: String,
    /// The five cron fields (ADR 0036). Absent from an older host: every day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<String>,
}

/// A workspace whose `.muxa/routines.toml` could not be used. It lists no Routines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutineProblem {
    pub workspace: String,
    /// Names the file and the Routine. A TOML parse error may quote the file.
    pub reason: String,
}

/// A Routine run that has not ended. Its record has kind `routine-run`; task
/// lists skip it. Its session carries `taskId` = `task`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRun {
    /// e.g. `2026-11-03T09:00` for a scheduled run, `now-1762160400` for run-now.
    pub occurrence: String,
    /// Epoch seconds.
    pub started_at: u64,
    /// The run's record id; absent for a moment while the run starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineSuccess {
    /// Epoch seconds of the publish.
    pub at: u64,
    pub occurrence: String,
    pub task: String,
    /// Entries stored by the publish; a repeated key is not counted.
    pub published: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineFailure {
    pub at: u64,
    pub occurrence: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    /// One sentence, with ids only: never output, instructions or source text.
    pub reason: String,
}

/// A stored Inbox entry: Routine output waiting for the user (ADR 0026).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxEntry {
    /// e.g. `e-1a2b3c4d`.
    pub id: String,
    pub workspace: String,
    pub routine: String,
    /// Chosen by the agent; a key the Routine already stored is skipped.
    pub key: String,
    pub kind: InboxEntryKind,
    pub title: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<SourceLink>,
    /// Epoch seconds.
    pub created_at: u64,
    /// The `routine-run` record that published it.
    pub run: String,
    pub occurrence: String,
    pub disposition: Disposition,
    /// With `later`: epoch seconds when the entry is open again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Only for kind `proposal`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal: Option<Proposal>,
    /// After `approve`: the Task that performs the proposal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InboxEntryKind {
    Briefing,
    Proposal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Disposition {
    Open,
    Later,
    Approved,
    Dismissed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceLink {
    /// `https` or `http`.
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// The action an approval starts. The Task gets it unchanged, with `hash`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    pub action: String,
    pub target: String,
    pub content: String,
    /// `sha256:<hex>` of action, target and content.
    pub hash: String,
}

/// An answer in `update-inbox`. Level 1 has `approve`, `dismiss` and
/// `later`; the rest are Inbox items' answers (level 3, ADR 0033).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InboxAction {
    /// A proposal: creates one Task, also when sent twice. An `ask`: picks the
    /// prompt's first option that is not `cancel`.
    Approve,
    /// Answers the item without acting; a briefing's Done.
    Dismiss,
    /// Needs `until`.
    Later,
    /// An `ask`: picks the prompt's `cancel` option. A proposal: as `dismiss`.
    Reject,
    /// An `ask`: picks the prompt's option at index `option`.
    Choose,
    /// A `review`: marks the Task done.
    Accept,
    /// A `review`: answers the item and leaves the Task open.
    KeepOpen,
    /// A `review`: sends `text` to the agent as feedback on its turn.
    Redo,
    /// A `review` or `failure`: sends `text` to the agent as a new direction.
    ChangeCourse,
    /// A `failure`: restarts the agent with a handoff.
    Retry,
    /// A `failure`: marks the Task dropped.
    Drop,
    /// Marks the item read. It stays open.
    Read,
    /// A reading Task's changed-checkout `proposal`: `branch-task` (level 4).
    Branch,
    /// A reading Task's changed-checkout `proposal`: leaves the changes as they are.
    Keep,
    /// A `review` or `failure`: sends `text` to the agent as it is. An `ask`
    /// about a permission: picks the `cancel` option first (ADR 0033 §4a).
    Reply,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_work_level_is_the_highest_advertised_one() {
        let caps = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(work_level(&caps(&["task-records"])), 0);
        assert_eq!(work_level(&caps(&["work:1"])), 1);
        assert_eq!(
            work_level(&caps(&["work:1", "work:3", "work:x", "workers"])),
            3
        );
        assert_eq!(work_level(&caps(&["work", "work-model:2"])), 0);
        assert_eq!(work_capability(WORK_ROUTINES), "work:1");
    }
}
