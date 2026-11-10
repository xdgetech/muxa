//! An assistant run is a task record of kind `assistant-run` (ADR 0048 §4):
//! the runs, claims, observation and Loop of a Task, as a Routine run has,
//! with the request version or the plan it answers (ADR 0049 §5). Task lists
//! skip it.
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::result::STAGE_DIR;
use crate::records::{KIND_FIELD, Loop, LoopSpec, Record};
use crate::task_records::Config;

pub const ASSISTANT_RUN: &str = "assistant-run";
const ASSISTANT_FIELD: &str = "assistant";
/// Not a shell command: the engine runs the result check in process for this kind.
pub const CHECK: &str = "muxa: assistant result";
pub const MAX_ATTEMPTS: u32 = 3;
/// Seconds; the in-process check never comes near it.
const TIMEOUT: u64 = 60;

/// What a run answers: a request version, or a plan. Its workspace is the record's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RunFor {
    Request(RequestRun),
    Plan(PlanRun),
}

/// The request version a run answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestRun {
    /// `q-…`.
    pub request: String,
    pub version: u64,
    /// `w-…`: the request's Work item.
    pub work: String,
    /// The SHA-256 of the notes the run read at its start (ADR 0049 §1).
    /// None: a run from before notes, which may not propose any.
    #[serde(default, rename = "notesBase", skip_serializing_if = "Option::is_none")]
    pub notes_base: Option<String>,
}

/// The plan an overall run answers (ADR 0049 §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRun {
    /// `p-…`.
    pub plan: String,
    /// The SHA-256 of the overall notes the run read at its start.
    #[serde(rename = "notesBase")]
    pub notes_base: String,
}

impl RunFor {
    /// The record fields that make a record an assistant run.
    pub fn fields(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut extra = serde_json::Map::new();
        extra.insert(KIND_FIELD.into(), ASSISTANT_RUN.into());
        match serde_json::to_value(self) {
            Ok(run) => {
                extra.insert(ASSISTANT_FIELD.into(), run);
            }
            Err(e) => tracing::warn!(error = %e, "cannot encode an assistant run's fields"),
        }
        extra
    }

    pub fn notes_base(&self) -> Option<&str> {
        match self {
            Self::Request(r) => r.notes_base.as_deref(),
            Self::Plan(p) => Some(&p.notes_base),
        }
    }

    /// The request or the plan the run answers.
    pub fn source(&self) -> &str {
        match self {
            Self::Request(r) => &r.request,
            Self::Plan(p) => &p.plan,
        }
    }
}

#[cfg(test)]
impl RequestRun {
    pub fn fields(&self) -> serde_json::Map<String, serde_json::Value> {
        RunFor::Request(self.clone()).fields()
    }
}

impl Record {
    pub fn is_assistant_run(&self) -> bool {
        self.extra
            .get(KIND_FIELD)
            .and_then(serde_json::Value::as_str)
            == Some(ASSISTANT_RUN)
    }

    /// None for a Task, or a run whose fields are unreadable.
    pub fn assistant_run(&self) -> Option<RunFor> {
        if !self.is_assistant_run() {
            return None;
        }
        serde_json::from_value(self.extra.get(ASSISTANT_FIELD)?.clone()).ok()
    }

    /// A run that answers a request version.
    pub fn request_run(&self) -> Option<RequestRun> {
        match self.assistant_run()? {
            RunFor::Request(r) => Some(r),
            RunFor::Plan(_) => None,
        }
    }

    /// An overall run that answers a plan.
    pub fn plan_run(&self) -> Option<PlanRun> {
        match self.assistant_run()? {
            RunFor::Plan(p) => Some(p),
            RunFor::Request(_) => None,
        }
    }
}

/// Arms the run's result check from `now`. Called in the write that records
/// the run's first agent, before the start prompt is typed.
pub fn arm(record: &mut Record, now: u64) {
    if !record.is_assistant_run() {
        return;
    }
    let spec = LoopSpec {
        check: CHECK.into(),
        max_attempts: Some(MAX_ATTEMPTS),
        timeout: Some(TIMEOUT),
    };
    if let Err(e) = record.set_loop(Some(&Loop::start(spec, None, false, now))) {
        tracing::warn!(task = %record.id, error = %e, "cannot arm an assistant run's check");
    }
}

/// The fixed prompt after a failed result or plan check (ADR 0048 §4).
pub fn fail_prompt(record: &Record, attempt: u32) -> String {
    let (what, file) = match record.plan_run() {
        Some(_) => ("plan", super::plan::file::PLAN),
        None => ("assistant result", super::result::RESULT),
    };
    format!(
        "The {what} did not pass the check. The reasons are in .muxa/checks/{attempt}.log. \
         Fix {file}, then end your turn."
    )
}

/// Before the agent starts: the private result folder (0700), and the run
/// folder with it.
pub fn prepare(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir.join(STAGE_DIR))
}

/// Before the agent starts: [`prepare`], the copy of the notes whose base
/// the record holds, and an overall run's brief. `root` is the workspace's.
pub fn prepare_run(
    config: &Config,
    record: &Record,
    root: &Path,
    dir: &Path,
) -> Result<(), String> {
    prepare(dir).map_err(|e| format!("cannot create the assistant run folder: {e}"))?;
    let run = record.assistant_run();
    if let Some(base) = run.as_ref().and_then(RunFor::notes_base) {
        super::notes::copy_into(root, dir, base).map_err(|e| e.to_string())?;
    }
    match run {
        Some(RunFor::Plan(p)) => super::plan::brief::write(config, &p.plan, dir),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{LoopState, Status};

    fn record(extra: serde_json::Map<String, serde_json::Value>) -> Record {
        Record {
            version: 1,
            id: "t-00000a".into(),
            rev: 1,
            workspace: "w".into(),
            title: "t".into(),
            goal: "g".into(),
            status: Status::Open,
            created_at: 1,
            updated_at: 1,
            project: None,
            repo: None,
            branch: None,
            dir: None,
            session: None,
            note: None,
            runs: Vec::new(),
            extra,
        }
    }

    #[test]
    fn a_run_reads_its_fields_and_arms_its_own_check() {
        let run = RequestRun {
            request: "q-0000000a".into(),
            version: 2,
            work: "w-0000000b".into(),
            notes_base: Some("e3b0".into()),
        };
        let mut r = record(RunFor::Request(run.clone()).fields());
        assert!(r.is_assistant_run() && r.is_background_run() && !r.is_routine_run());
        assert_eq!(r.request_run(), Some(run));
        assert_eq!(r.plan_run(), None);
        arm(&mut r, 50);
        let lp = r.task_loop().unwrap().unwrap();
        assert_eq!((lp.check.as_str(), lp.max_attempts), (CHECK, MAX_ATTEMPTS));
        assert_eq!((lp.state, lp.armed_at), (LoopState::Armed, 50));
        assert!(fail_prompt(&r, 2).contains(".muxa/checks/2.log"));
        assert!(fail_prompt(&r, 2).contains(".muxa/assistant/result.json"));
    }

    #[test]
    fn a_plan_run_names_its_plan_and_its_own_file() {
        let run = PlanRun {
            plan: "p-0000000a".into(),
            notes_base: "e3b0".into(),
        };
        let mut r = record(RunFor::Plan(run.clone()).fields());
        assert!(r.is_assistant_run() && r.is_background_run());
        assert_eq!(
            r.extra["assistant"],
            serde_json::json!({"plan": "p-0000000a", "notesBase": "e3b0"})
        );
        assert_eq!(r.plan_run(), Some(run));
        assert_eq!(r.request_run(), None);
        assert_eq!(r.assistant_run().unwrap().source(), "p-0000000a");
        arm(&mut r, 50);
        assert_eq!(r.task_loop().unwrap().unwrap().check, CHECK);
        assert!(fail_prompt(&r, 1).contains(".muxa/assistant/plan.json"));
    }

    #[test]
    fn a_task_or_a_routine_run_is_not_armed() {
        let mut task = record(serde_json::Map::new());
        arm(&mut task, 50);
        assert_eq!(task.assistant_run(), None);
        assert_eq!(task.request_run(), None);
        assert!(task.task_loop().unwrap().is_none());
        let mut routine = record(
            crate::routines::record::RunOf {
                name: "n".into(),
                occurrence: "o".into(),
            }
            .fields(),
        );
        arm(&mut routine, 50);
        assert!(routine.task_loop().unwrap().is_none());
    }
}
