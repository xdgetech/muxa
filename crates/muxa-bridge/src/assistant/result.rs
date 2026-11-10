//! The run's result file, `.muxa/assistant/result.json` (ADR 0048 §4, schema
//! in `protocol/PROTOCOL.md`): read, validated, and stored on the request.
//! The agent's text is untrusted: reasons name fields and indices, never values.
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsFd, OwnedFd};
use std::path::Path;

use muxa_protocol::{
    AssistResult, AssistantRequest, Delivery, Finding, Handoff, Notes, PossibleDuplicate,
    ResultSource,
};
use rustix::fs::{Mode, OFlags};
use serde::Deserialize;

use super::handoff::{Chain, RawHandoff};
use super::notes::{MAX_NOTES, base_of};
use super::record::RequestRun;
use crate::routines::output::valid_url;

/// The result folder, relative to the run's directory.
pub const STAGE_DIR: &str = ".muxa/assistant";
pub const RESULT: &str = ".muxa/assistant/result.json";
pub const VERSION: u32 = 1;
/// Results a request keeps, newest first.
pub const MAX_RESULTS: usize = 10;
/// Ten results of this size fit the request's 1 MiB file.
const MAX_FILE: u64 = 64 << 10;
const MAX_SUMMARY: usize = 4096;
const MAX_FINDINGS: usize = 20;
const MAX_FINDING: usize = 1000;
const MAX_SOURCES: usize = 10;
pub(super) const MAX_TITLE: usize = 200;
const MAX_PATH: usize = 4096;
const MAX_LIST: usize = 10;
pub(super) const MAX_LINE: usize = 500;
const MAX_FILES: usize = 20;
const MAX_DUPLICATES: usize = 5;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawResult {
    version: u32,
    request: String,
    request_version: u64,
    summary: String,
    #[serde(default)]
    findings: Vec<RawFinding>,
    #[serde(default)]
    open_questions: Vec<String>,
    #[serde(default)]
    next: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    possible_duplicates: Vec<RawDuplicate>,
    notes: Option<String>,
    handoff: Option<RawHandoff>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFinding {
    text: String,
    sources: Vec<RawSource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct RawSource {
    url: Option<String>,
    path: Option<String>,
    title: Option<String>,
    read_at: Option<u64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDuplicate {
    work: String,
    reason: String,
}

/// A result that passed, and the hand-off it asks for (ADR 0051 §3).
#[derive(Debug, Clone, PartialEq)]
pub struct Checked {
    pub result: AssistResult,
    pub handoff: Option<Handoff>,
}

/// Reads the result in `dir` for `run`, a step of `chain`. The reasons are
/// for the agent's check log.
pub fn read(dir: &Path, run: &RequestRun, chain: &Chain) -> Result<Checked, Vec<String>> {
    let bytes = read_file(dir, "result.json").map_err(|r| vec![r])?;
    check(&bytes, run, chain)
}

/// Reads `.muxa/assistant/<name>` in `dir`, at most 64 KiB. Each folder is
/// opened without following a symlink, so the agent cannot point the host
/// at another file.
pub(super) fn read_file(dir: &Path, name: &str) -> Result<Vec<u8>, String> {
    let file_path = format!("{STAGE_DIR}/{name}");
    let shown = file_path.as_str();
    let folder = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let run = rustix::fs::open(dir, folder, Mode::empty())
        .map_err(|e| format!("cannot open the run folder: {e}"))?;
    let muxa = step(&run, ".muxa", folder | OFlags::NOFOLLOW, ".muxa", shown)?;
    let stage = step(
        &muxa,
        "assistant",
        folder | OFlags::NOFOLLOW,
        STAGE_DIR,
        shown,
    )?;
    let file_flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = step(&stage, name, file_flags, shown, shown)?;
    let file = File::from(fd);
    let meta = file
        .metadata()
        .map_err(|e| format!("cannot read {shown}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{shown} is not a regular file."));
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {shown}: {e}"))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(format!("{shown} is larger than 64 KiB."));
    }
    Ok(bytes)
}

/// `wanted` is the file the agent writes.
fn step(
    at: &OwnedFd,
    name: &str,
    flags: OFlags,
    shown: &str,
    wanted: &str,
) -> Result<OwnedFd, String> {
    rustix::fs::openat(at.as_fd(), name, flags, Mode::empty()).map_err(|e| match e {
        rustix::io::Errno::NOENT => {
            format!("{shown} does not exist. Write {wanted}, then end your turn.")
        }
        rustix::io::Errno::LOOP | rustix::io::Errno::NOTDIR => {
            format!("{shown} is a symlink or not of the right type; write a regular file in a plain folder.")
        }
        e => format!("cannot open {shown}: {e}"),
    })
}

/// Every reason the result fails, or the result for `run`'s version. A
/// request without staff has an empty `chain`: no hand-off passes.
#[cfg(test)]
pub fn validate(bytes: &[u8], run: &RequestRun) -> Result<AssistResult, Vec<String>> {
    check(bytes, run, &Chain::default()).map(|c| c.result)
}

/// Every reason the result fails, or the result for `run`, a step of `chain`.
pub fn check(bytes: &[u8], run: &RequestRun, chain: &Chain) -> Result<Checked, Vec<String>> {
    if bytes.contains(&0) {
        return Err(vec![format!("{RESULT} contains a NUL byte.")]);
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(vec![format!("{RESULT} is not UTF-8.")]);
    }
    let raw: RawResult = serde_json::from_slice(bytes)
        .map_err(|e| vec![parse_reason(&e, RESULT, "Captures and Work items")])?;
    let mut reasons = Vec::new();
    if raw.version != VERSION {
        reasons.push(format!("version must be {VERSION}."));
    }
    if raw.request != run.request {
        reasons.push(format!(
            "request must be {}, the request this run answers.",
            run.request
        ));
    }
    if raw.request_version != run.version {
        reasons.push(format!(
            "requestVersion must be {}, the version this run answers.",
            run.version
        ));
    }
    text("summary", &raw.summary, MAX_SUMMARY, &mut reasons);
    let findings = findings(raw.findings, &mut reasons);
    lines("openQuestions", &raw.open_questions, &mut reasons);
    lines("next", &raw.next, &mut reasons);
    count("files", raw.files.len(), MAX_FILES, &mut reasons);
    for (i, f) in raw.files.iter().enumerate() {
        path(&format!("files[{i}]"), f, &mut reasons);
    }
    count(
        "possibleDuplicates",
        raw.possible_duplicates.len(),
        MAX_DUPLICATES,
        &mut reasons,
    );
    for (i, d) in raw.possible_duplicates.iter().enumerate() {
        if !is_work_id(&d.work) || d.work == run.work {
            reasons.push(format!(
                "possibleDuplicates[{i}].work must be another Work item's id: w- and 8 hex digits."
            ));
        }
        text(
            &format!("possibleDuplicates[{i}].reason"),
            &d.reason,
            MAX_LINE,
            &mut reasons,
        );
    }
    let notes = raw
        .notes
        .and_then(|text| proposal(text, run.notes_base.as_deref(), &mut reasons));
    let handoff = raw
        .handoff
        .and_then(|h| super::handoff::check(h, chain, &mut reasons));
    if !reasons.is_empty() {
        return Err(reasons);
    }
    let result = AssistResult {
        version: run.version,
        at: 0,
        summary: raw.summary,
        stale: false,
        findings,
        open_questions: raw.open_questions,
        next: raw.next,
        files: raw.files,
        possible_duplicates: raw
            .possible_duplicates
            .into_iter()
            .map(|d| PossibleDuplicate {
                work: d.work,
                reason: d.reason,
            })
            .collect(),
        notes,
        worker: None,
        step: None,
    };
    Ok(Checked { result, handoff })
}

/// The proposed notes, stored with the base the run read (ADR 0049 §1).
/// None when the text is the copy's own: nothing to apply.
pub(super) fn proposal(
    text: String,
    base: Option<&str>,
    reasons: &mut Vec<String>,
) -> Option<Notes> {
    if text.len() > MAX_NOTES || text.contains('\0') {
        reasons.push("notes must be at most 16 KiB of text, without NUL.".into());
        return None;
    }
    let Some(base) = base else {
        reasons.push("notes: this run read no notes; leave notes out.".into());
        return None;
    };
    (base_of(text.as_bytes()) != base).then(|| Notes {
        text,
        base: base.to_owned(),
    })
}

/// serde's own message can quote a value, so only its kind and place are
/// kept. `section` is where PROTOCOL.md describes the file.
pub(super) fn parse_reason(e: &serde_json::Error, file: &str, section: &str) -> String {
    let what = match e.classify() {
        serde_json::error::Category::Data => {
            "does not match the schema (an unknown or missing field, or a wrong type)"
        }
        serde_json::error::Category::Eof => "ends too early",
        _ => "is not valid JSON",
    };
    format!(
        "{file} {what} at line {}, column {}. The schema is in muxa's PROTOCOL.md, \"{section}\".",
        e.line(),
        e.column()
    )
}

fn findings(raw: Vec<RawFinding>, reasons: &mut Vec<String>) -> Vec<Finding> {
    count("findings", raw.len(), MAX_FINDINGS, reasons);
    let mut out = Vec::with_capacity(raw.len());
    for (i, f) in raw.into_iter().enumerate() {
        text(
            &format!("findings[{i}].text"),
            &f.text,
            MAX_FINDING,
            reasons,
        );
        if f.sources.is_empty() || f.sources.len() > MAX_SOURCES {
            reasons.push(format!(
                "findings[{i}].sources must have 1 to {MAX_SOURCES} entries."
            ));
        }
        let mut sources = Vec::with_capacity(f.sources.len());
        for (j, s) in f.sources.into_iter().enumerate() {
            let at = format!("findings[{i}].sources[{j}]");
            if s.url.is_none() && s.path.is_none() {
                reasons.push(format!("{at} needs a url or a path."));
            }
            if s.url.as_deref().is_some_and(|u| !valid_url(u)) {
                reasons.push(format!(
                    "{at}.url must be an http or https URL of at most 2048 bytes, without spaces."
                ));
            }
            if let Some(p) = &s.path {
                path(&format!("{at}.path"), p, reasons);
            }
            if let Some(t) = &s.title {
                one_line(&format!("{at}.title"), t, MAX_TITLE, reasons);
            }
            sources.push(ResultSource {
                url: s.url,
                path: s.path,
                title: s.title,
                read_at: s.read_at,
            });
        }
        out.push(Finding {
            text: f.text,
            sources,
        });
    }
    out
}

fn lines(field: &str, values: &[String], reasons: &mut Vec<String>) {
    count(field, values.len(), MAX_LIST, reasons);
    for (i, v) in values.iter().enumerate() {
        text(&format!("{field}[{i}]"), v, MAX_LINE, reasons);
    }
}

pub(super) fn count(field: &str, n: usize, max: usize, reasons: &mut Vec<String>) {
    if n > max {
        reasons.push(format!("{field} has more than {max} entries."));
    }
}

/// 1 to `max` bytes; line breaks and tabs are the only control characters.
pub(super) fn text(field: &str, value: &str, max: usize, reasons: &mut Vec<String>) {
    let bad_control = value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t');
    if value.trim().is_empty() || value.len() > max || bad_control {
        reasons.push(format!(
            "{field} must be 1 to {max} bytes; line breaks and tabs are the only control characters."
        ));
    }
}

pub(super) fn one_line(field: &str, value: &str, max: usize, reasons: &mut Vec<String>) {
    let n = value.chars().count();
    if value.trim().is_empty() || n > max || value.chars().any(char::is_control) {
        reasons.push(format!(
            "{field} must be 1 to {max} characters on one line."
        ));
    }
}

pub(super) fn path(field: &str, value: &str, reasons: &mut Vec<String>) {
    if !value.starts_with('/') || value.len() > MAX_PATH || value.chars().any(char::is_control) {
        reasons.push(format!(
            "{field} must be an absolute path of at most {MAX_PATH} bytes, without control characters."
        ));
    }
}

/// `w-` and 8 lowercase hex digits, as the store makes them.
pub(super) fn is_work_id(id: &str) -> bool {
    id.strip_prefix("w-").is_some_and(|h| {
        h.len() == 8
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// What [`store`] did with a result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stored {
    New,
    /// This version already has a result; nothing changed.
    Already,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    #[error("request {request} is at version {current}; the run answers version {run}")]
    NotCurrent {
        request: String,
        current: u64,
        run: u64,
    },
    #[error("request {0} was cancelled")]
    Cancelled(String),
}

/// Adds `result` as the request's newest. Older results become stale; at
/// most [`MAX_RESULTS`] stay. The Work item never changes here.
pub fn store(
    q: &mut AssistantRequest,
    mut result: AssistResult,
    at: u64,
) -> Result<Stored, Refused> {
    if q.version != result.version {
        return Err(Refused::NotCurrent {
            request: q.id.clone(),
            current: q.version,
            run: result.version,
        });
    }
    let step = result.step.unwrap_or(1);
    let same = |r: &AssistResult| r.version == result.version && r.step.unwrap_or(1) == step;
    // A step before the newest has its result: the next step began after it.
    if q.results.iter().any(same) || step < super::steps::current(q) {
        return Ok(Stored::Already);
    }
    if matches!(q.delivery, Delivery::CancelRequested | Delivery::Cancelled) {
        return Err(Refused::Cancelled(q.id.clone()));
    }
    // Inside a version nothing is stale: a chain is one answer in parts (ADR 0051 §3).
    for older in q.results.iter_mut().filter(|r| r.version < result.version) {
        older.stale = true;
    }
    result.at = at;
    result.stale = false;
    q.results.insert(0, result);
    q.results.truncate(MAX_RESULTS);
    q.delivery = Delivery::Resulted;
    q.delivery_at = at;
    q.updated_at = at;
    super::steps::sync(q);
    Ok(Stored::New)
}

#[cfg(test)]
#[path = "result_tests.rs"]
mod tests;
