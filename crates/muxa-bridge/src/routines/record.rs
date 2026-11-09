//! A Routine run is a task record of kind `routine-run` (ADR 0026 section 3):
//! the same runs, claims, observation and Loop as a Task, and two extra
//! fields. Task lists skip it.
use serde::{Deserialize, Serialize};

use crate::records::{KIND_FIELD, Loop, LoopSpec, Record};

pub const ROUTINE_RUN: &str = "routine-run";
const ROUTINE_FIELD: &str = "routine";
/// Not a shell command: the engine runs the output check in process for this kind.
pub const CHECK: &str = "muxa: routine output";
pub const MAX_ATTEMPTS: u32 = 3;
/// Seconds; the in-process check never comes near it.
const TIMEOUT: u64 = 60;

/// Which Routine and occurrence a run belongs to. Its workspace is the record's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOf {
    pub name: String,
    pub occurrence: String,
}

impl RunOf {
    /// The record fields that make a record a run.
    pub fn fields(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut extra = serde_json::Map::new();
        extra.insert(KIND_FIELD.into(), ROUTINE_RUN.into());
        extra.insert(
            ROUTINE_FIELD.into(),
            serde_json::json!({"name": self.name, "occurrence": self.occurrence}),
        );
        extra
    }
}

impl Record {
    pub fn is_routine_run(&self) -> bool {
        self.extra
            .get(KIND_FIELD)
            .and_then(serde_json::Value::as_str)
            == Some(ROUTINE_RUN)
    }

    /// None for a Task, or a run whose fields are unreadable.
    pub fn routine_run(&self) -> Option<RunOf> {
        if !self.is_routine_run() {
            return None;
        }
        serde_json::from_value(self.extra.get(ROUTINE_FIELD)?.clone()).ok()
    }
}

/// Arms the run's output check from `now`. Called in the write that
/// records the run's first agent, before the start prompt is typed.
pub fn arm(record: &mut Record, now: u64) {
    if !record.is_routine_run() {
        return;
    }
    let spec = LoopSpec {
        check: CHECK.into(),
        max_attempts: Some(MAX_ATTEMPTS),
        timeout: Some(TIMEOUT),
    };
    if let Err(e) = record.set_loop(Some(&Loop::start(spec, None, false, now))) {
        tracing::warn!(task = %record.id, error = %e, "cannot arm a routine run's check");
    }
}

/// The fixed prompt after a failed output check (ADR 0026 section 3).
pub fn fail_prompt(attempt: u32) -> String {
    format!(
        "The Routine output did not pass the check. The reasons are in .muxa/checks/{attempt}.log. \
         Fix .muxa/routine/output.json, then end your turn."
    )
}
