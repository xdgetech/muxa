//! An assistant run is a task record of kind `assistant-run` (ADR 0046 §4):
//! the runs, claims, observation and Loop of a Task, as a Routine run has,
//! with the request version it answers. Task lists skip it.
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::result::STAGE_DIR;
use crate::records::{KIND_FIELD, Loop, LoopSpec, Record};

pub const ASSISTANT_RUN: &str = "assistant-run";
const ASSISTANT_FIELD: &str = "assistant";
/// Not a shell command: the engine runs the result check in process for this kind.
pub const CHECK: &str = "muxa: assistant result";
pub const MAX_ATTEMPTS: u32 = 3;
/// Seconds; the in-process check never comes near it.
const TIMEOUT: u64 = 60;

/// The request version a run answers. Its workspace is the record's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFor {
    /// `q-…`.
    pub request: String,
    pub version: u64,
    /// `w-…`: the request's Work item.
    pub work: String,
}

impl RunFor {
    /// The record fields that make a record an assistant run.
    pub fn fields(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut extra = serde_json::Map::new();
        extra.insert(KIND_FIELD.into(), ASSISTANT_RUN.into());
        extra.insert(
            ASSISTANT_FIELD.into(),
            serde_json::json!({"request": self.request, "version": self.version, "work": self.work}),
        );
        extra
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

/// The fixed prompt after a failed result check (ADR 0046 §4).
pub fn fail_prompt(attempt: u32) -> String {
    format!(
        "The assistant result did not pass the check. The reasons are in .muxa/checks/{attempt}.log. \
         Fix .muxa/assistant/result.json, then end your turn."
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
        let run = RunFor {
            request: "q-0000000a".into(),
            version: 2,
            work: "w-0000000b".into(),
        };
        let mut r = record(run.fields());
        assert!(r.is_assistant_run() && r.is_background_run() && !r.is_routine_run());
        assert_eq!(r.assistant_run(), Some(run));
        arm(&mut r, 50);
        let lp = r.task_loop().unwrap().unwrap();
        assert_eq!((lp.check.as_str(), lp.max_attempts), (CHECK, MAX_ATTEMPTS));
        assert_eq!((lp.state, lp.armed_at), (LoopState::Armed, 50));
        assert!(fail_prompt(2).contains(".muxa/checks/2.log"));
        assert!(fail_prompt(2).contains(".muxa/assistant/result.json"));
    }

    #[test]
    fn a_task_or_a_routine_run_is_not_armed() {
        let mut task = record(serde_json::Map::new());
        arm(&mut task, 50);
        assert_eq!(task.assistant_run(), None);
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
