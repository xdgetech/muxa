//! The earlier steps of a version in full (ADR 0051 §5): before a later
//! step's agent starts, the host writes each earlier step's Result to
//! `.muxa/assistant/steps/<k>.json` in the run folder. The goal keeps only
//! each step's worker and a cut summary, so the 16 KiB goal keeps room.
//! Proposed notes stay out: they are another member's memory.
use std::fs::{DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

use muxa_protocol::{AssistResult, AssistantRequest, Handoff};
use serde::Serialize;

use super::record::RequestRun;
use super::result::STAGE_DIR;
use crate::task_records::Config;

/// Relative to the run folder.
pub const STEPS_DIR: &str = ".muxa/assistant/steps";
const FILE_MODE: u32 = 0o600;
const DIR_MODE: u32 = 0o700;

/// One earlier step, as its file holds it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StepFile<'a> {
    step: u32,
    worker: &'a str,
    result: AssistResult,
    /// What it asked of the member after it.
    #[serde(skip_serializing_if = "Option::is_none")]
    handoff: Option<&'a Handoff>,
}

/// The path of step `k`'s file, relative to the run folder.
pub fn path(step: u32) -> String {
    format!("{STEPS_DIR}/{step}.json")
}

/// Blocking. Writes the files of `run`'s earlier steps into `dir`, from the
/// request as stored. Nothing for step 1. Errors name paths, never content.
pub fn write(config: &Config, run: &RequestRun, dir: &Path) -> Result<usize, String> {
    if run.step() <= 1 {
        return Ok(0);
    }
    let q = super::shelf::<AssistantRequest>(config)
        .map_err(|e| e.message)?
        .get(&run.request)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no request {}", run.request))?;
    let files = earlier(&q, run.version, run.step());
    if files.is_empty() {
        return Ok(0);
    }
    let steps = dir.join(STEPS_DIR);
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(&steps)
        .map_err(|e| format!("cannot create {}: {e}", dir.join(STAGE_DIR).display()))?;
    for f in &files {
        let bytes = serde_json::to_vec_pretty(f).map_err(|e| e.to_string())?;
        let path = dir.join(path(f.step));
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(FILE_MODE)
            .open(&path)
            .and_then(|mut file| file.write_all(&bytes))
            .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    }
    Ok(files.len())
}

/// Pure. The steps of `version` before `step` that have a result, oldest first.
fn earlier(q: &AssistantRequest, version: u64, step: u32) -> Vec<StepFile<'_>> {
    let mut out: Vec<StepFile<'_>> = q
        .steps
        .iter()
        .filter(|s| s.step < step)
        .filter_map(|s| {
            let r = q
                .results
                .iter()
                .find(|r| r.version == version && r.step.unwrap_or(1) == s.step)?;
            Some(StepFile {
                step: s.step,
                worker: &s.worker,
                result: AssistResult {
                    notes: None,
                    ..r.clone()
                },
                handoff: s.handoff.as_ref(),
            })
        })
        .collect();
    out.sort_by_key(|f| f.step);
    out
}

#[cfg(test)]
mod tests {
    use muxa_protocol::{Delivery, Notes, StepBy};
    use serde_json::json;

    use super::*;
    use crate::assistant::steps;

    fn result(version: u64, step: u32, summary: &str) -> AssistResult {
        serde_json::from_value(json!({
            "version": version, "at": 1, "summary": summary, "step": step,
            "worker": "w", "files": [format!("/root/step-{step}.md")],
        }))
        .unwrap()
    }

    #[test]
    fn only_this_versions_earlier_steps_are_written_whole_without_notes() {
        let mut q: AssistantRequest = serde_json::from_value(json!({
            "id": "q-0000000a", "work": "w-0000000b", "version": 2, "workspace": "lisk",
            "scope": "research", "delivery": "pending", "deliveryAt": 1,
            "createdAt": 1, "updatedAt": 1,
        }))
        .unwrap();
        q.steps = vec![
            steps::fresh(1, "lead", StepBy::Lead, 1),
            steps::fresh(2, "strategist", StepBy::Handoff, 1),
            steps::fresh(3, "architect", StepBy::Handoff, 1),
        ];
        q.steps[0].handoff = Some(Handoff {
            worker: "strategist".into(),
            purpose: "Compare.".into(),
        });
        q.steps[0].delivery = Delivery::Resulted;
        let mut with_notes = result(2, 1, &"long ".repeat(400));
        with_notes.notes = Some(Notes {
            text: "notes-mark".into(),
            base: "sha".into(),
        });
        q.results = vec![
            result(2, 2, "Second."),
            with_notes,
            result(1, 1, "Older version."),
        ];
        let files = earlier(&q, 2, 3);
        assert_eq!(files.iter().map(|f| f.step).collect::<Vec<_>>(), [1, 2]);
        let first = serde_json::to_string(&files[0]).unwrap();
        assert!(first.contains(&"long ".repeat(400)), "the summary is whole");
        assert!(first.contains("/root/step-1.md") && first.contains("Compare."));
        assert!(!first.contains("notes-mark"));
        assert!(earlier(&q, 2, 1).is_empty());
        assert_eq!(path(2), ".muxa/assistant/steps/2.json");
    }
}
