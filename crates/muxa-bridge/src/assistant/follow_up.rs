//! A version that continues a Result (ADR 0058 §3, §4): Send on a `result`
//! item or Revise on a `write` item. The request file keeps which Result,
//! the step's run gets it whole in `.muxa/assistant/followup.json`, and its
//! goal says to continue from it. Errors name paths, never content.
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use muxa_protocol::{AssistResult, AssistantRequest};
use serde::{Deserialize, Serialize};

use super::record::RequestRun;
use super::store::Extra;
use crate::task_records::Config;

/// The request file's field: the Result the current version continues.
pub const FIELD: &str = "followUp";
/// Relative to the run folder.
pub const FILE: &str = ".muxa/assistant/followup.json";
const FILE_MODE: u32 = 0o600;

/// Which Result a version continues; `write` names the one to draft again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowUp {
    pub version: u64,
    pub step: u32,
    /// Index into that Result's `writes`: Revise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub write: Option<usize>,
}

pub fn get(extra: &Extra) -> Option<FollowUp> {
    serde_json::from_value(extra.get(FIELD)?.clone()).ok()
}

/// A new version continues `f`, or nothing.
pub fn set(extra: &mut Extra, f: Option<&FollowUp>) {
    match f.map(serde_json::to_value) {
        Some(Ok(v)) => {
            extra.insert(FIELD.into(), v);
        }
        Some(Err(e)) => tracing::warn!(error = %e, "cannot keep a follow-up's mark"),
        None => {
            extra.remove(FIELD);
        }
    }
}

/// The continued Result of `q`'s current version, for its first step.
pub fn of<'a>(q: &'a AssistantRequest, extra: &Extra) -> Option<(FollowUp, &'a AssistResult)> {
    let f = get(extra)?;
    let r = q
        .results
        .iter()
        .find(|r| r.version == f.version && r.step.unwrap_or(1) == f.step)?;
    Some((f, r))
}

/// What the goal says about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Brief {
    pub version: u64,
    pub step: u32,
    pub worker: Option<String>,
    /// The write to draft again: its number from 1, action and target.
    pub write: Option<(usize, String, String)>,
}

impl Brief {
    pub fn new(f: &FollowUp, r: &AssistResult) -> Self {
        Self {
            version: f.version,
            step: f.step,
            worker: r.worker.clone(),
            write: f.write.and_then(|i| {
                let w = r.writes.get(i)?;
                Some((i + 1, w.action.clone(), w.target.clone()))
            }),
        }
    }

    pub fn section(&self) -> String {
        let by = self
            .worker
            .as_deref()
            .map(|w| format!(" by {w}"))
            .unwrap_or_default();
        let mut out = format!(
            "## The result you continue\n\nThe user read the result of version {}, step {}{by}, \
             and wrote what is under \"What the user asked\". It is whole in {FILE}. Continue \
             from it; do not start over.\n\n",
            self.version, self.step
        );
        if let Some((n, action, target)) = &self.write {
            out.push_str(&format!(
                "They ask for a new draft of its proposed write {n}, \"{action}\" to {target}. \
                 Propose the new text in `writes`; the old proposal is closed.\n\n"
            ));
        }
        out
    }
}

/// The file's content.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct File<'a> {
    version: u64,
    step: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    write: Option<usize>,
    result: AssistResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    worker: Option<&'a str>,
}

/// Blocking. Writes `followup.json` into `dir` for the first step of a
/// version that continues a Result. False when there is none.
pub fn write(config: &Config, run: &RequestRun, dir: &Path) -> Result<bool, String> {
    if run.step() > 1 {
        return Ok(false);
    }
    let (q, extra) = super::shelf::<AssistantRequest>(config)
        .map_err(|e| e.message)?
        .get_with_extra(&run.request)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no request {}", run.request))?;
    if q.version != run.version {
        return Ok(false);
    }
    let Some((f, r)) = of(&q, &extra) else {
        return Ok(false);
    };
    let file = File {
        version: f.version,
        step: f.step,
        write: f.write,
        // Another member's memory stays out, as in the steps' files.
        result: AssistResult {
            notes: None,
            ..r.clone()
        },
        worker: r.worker.as_deref(),
    };
    let bytes = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
    let path = dir.join(FILE);
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FILE_MODE)
        .open(&path)
        .and_then(|mut f| f.write_all(&bytes))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
#[path = "follow_up_tests.rs"]
mod tests;
