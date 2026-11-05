//! The run's private output stage, `.muxa/routine/output.json` (ADR 0026
//! section 4, schema in `protocol/PROTOCOL.md`): read, validated, and
//! applied to the Routine's state in one step. Source text is untrusted:
//! reasons name items and fields, never their values.
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use muxa_protocol::{
    Disposition, InboxEntry, InboxEntryKind, Proposal, RoutineSuccess, SourceLink,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::store::{RoutineState, Stored};

/// The stage folder, relative to the run's directory.
pub const STAGE_DIR: &str = ".muxa/routine";
pub const OUTPUT: &str = ".muxa/routine/output.json";
pub const STATE: &str = ".muxa/routine/state.json";
pub const VERSION: u32 = 1;
const MAX_STAGE: u64 = 256 << 10;
const MAX_KEY: usize = 200;
const MAX_TITLE: usize = 200;
const MAX_BODY: usize = 4096;
const MAX_LINKS: usize = 10;
const MAX_URL: usize = 2048;
const MAX_ACTION: usize = 200;
const MAX_TARGET: usize = 500;
const MAX_CONTENT: usize = 8192;
const MAX_STATE: usize = 16 << 10;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawOutput {
    version: u32,
    #[serde(default)]
    items: Vec<RawItem>,
    #[serde(default)]
    state: Option<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawItem {
    kind: String,
    key: String,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    links: Vec<RawLink>,
    action: Option<String>,
    target: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLink {
    url: String,
    title: Option<String>,
}

/// One validated item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub kind: InboxEntryKind,
    pub key: String,
    pub title: String,
    pub body: String,
    pub links: Vec<SourceLink>,
    pub proposal: Option<Proposal>,
}

/// A stage that passed every check.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub items: Vec<Item>,
    /// None keeps the committed state.
    pub state: Option<serde_json::Value>,
}

/// Reads the stage in `dir`. The reasons are for the agent's check log.
pub fn read(dir: &Path, limit: u32) -> Result<Output, Vec<String>> {
    let path = dir.join(OUTPUT);
    let bytes = read_stage(&path).map_err(|r| vec![r])?;
    validate(&bytes, limit)
}

fn read_stage(path: &PathBuf) -> Result<Vec<u8>, String> {
    // NOFOLLOW: a symlink could point the host at any file.
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(|e| match e {
        rustix::io::Errno::NOENT => {
            format!("{OUTPUT} does not exist. Write it, then end your turn.")
        }
        rustix::io::Errno::LOOP => format!("{OUTPUT} is a symlink; write a regular file."),
        e => format!("cannot open {OUTPUT}: {e}"),
    })?;
    let file = File::from(fd);
    let meta = file
        .metadata()
        .map_err(|e| format!("cannot read {OUTPUT}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{OUTPUT} is not a regular file."));
    }
    let mut bytes = Vec::new();
    file.take(MAX_STAGE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {OUTPUT}: {e}"))?;
    if bytes.len() as u64 > MAX_STAGE {
        return Err(format!("{OUTPUT} is larger than 256 KiB."));
    }
    Ok(bytes)
}

/// Every reason the stage fails, or the output.
pub fn validate(bytes: &[u8], limit: u32) -> Result<Output, Vec<String>> {
    let raw: RawOutput = serde_json::from_slice(bytes).map_err(|e| {
        vec![format!(
            "{OUTPUT} is not valid: {e}. The schema is in muxa's PROTOCOL.md, \"Routine output\"."
        )]
    })?;
    let mut reasons = Vec::new();
    if raw.version != VERSION {
        reasons.push(format!("version must be {VERSION}."));
    }
    if raw.items.len() > limit as usize {
        reasons.push(format!(
            "items has {} entries; this Routine allows at most {limit}.",
            raw.items.len()
        ));
    }
    let mut items = Vec::with_capacity(raw.items.len());
    for (i, item) in raw.items.into_iter().enumerate() {
        match check_item(item) {
            Ok(item) => {
                if items.iter().any(|o: &Item| o.key == item.key) {
                    reasons.push(format!("items[{i}].key repeats an earlier key."));
                }
                items.push(item);
            }
            Err(mut r) => {
                for reason in &mut r {
                    *reason = format!("items[{i}].{reason}");
                }
                reasons.extend(r);
            }
        }
    }
    if let Some(state) = &raw.state {
        let size = serde_json::to_vec(state).map_or(usize::MAX, |v| v.len());
        if size > MAX_STATE {
            reasons.push("state is larger than 16 KiB.".into());
        }
    }
    if reasons.is_empty() {
        Ok(Output {
            items,
            state: raw.state,
        })
    } else {
        Err(reasons)
    }
}

fn check_item(raw: RawItem) -> Result<Item, Vec<String>> {
    let mut reasons = Vec::new();
    let kind = match raw.kind.as_str() {
        "briefing" => Some(InboxEntryKind::Briefing),
        "proposal" => Some(InboxEntryKind::Proposal),
        _ => {
            reasons.push("kind must be briefing or proposal.".into());
            None
        }
    };
    bytes_line("key", &raw.key, MAX_KEY, &mut reasons);
    chars_line("title", &raw.title, MAX_TITLE, &mut reasons);
    text("body", &raw.body, MAX_BODY, true, &mut reasons);
    if raw.links.len() > MAX_LINKS {
        reasons.push(format!("links has more than {MAX_LINKS} entries."));
    }
    let mut links = Vec::new();
    for (j, link) in raw.links.into_iter().enumerate() {
        if !valid_url(&link.url) {
            reasons.push(format!(
                "links[{j}].url must be an http or https URL of at most {MAX_URL} bytes, without spaces."
            ));
        }
        if let Some(title) = &link.title {
            chars_line(&format!("links[{j}].title"), title, MAX_TITLE, &mut reasons);
        }
        links.push(SourceLink {
            url: link.url,
            title: link.title,
        });
    }
    let fields = (raw.action, raw.target, raw.content);
    let proposal = match (kind, fields) {
        (Some(InboxEntryKind::Briefing), (None, None, None)) | (None, _) => None,
        (Some(InboxEntryKind::Briefing), _) => {
            reasons.push("a briefing has no action, target or content.".into());
            None
        }
        (Some(InboxEntryKind::Proposal), (Some(action), Some(target), Some(content))) => {
            chars_line("action", &action, MAX_ACTION, &mut reasons);
            bytes_line("target", &target, MAX_TARGET, &mut reasons);
            text("content", &content, MAX_CONTENT, false, &mut reasons);
            let hash = proposal_hash(&action, &target, &content);
            Some(Proposal {
                action,
                target,
                content,
                hash,
            })
        }
        (Some(InboxEntryKind::Proposal), _) => {
            reasons.push("a proposal needs action, target and content.".into());
            None
        }
    };
    match kind {
        Some(kind) if reasons.is_empty() => Ok(Item {
            kind,
            key: raw.key,
            title: raw.title,
            body: raw.body,
            links,
            proposal,
        }),
        _ => Err(reasons),
    }
}

/// `sha256:<hex>` of action, NUL, target, NUL, content.
pub fn proposal_hash(action: &str, target: &str, content: &str) -> String {
    let mut h = Sha256::new();
    h.update(action.as_bytes());
    h.update([0]);
    h.update(target.as_bytes());
    h.update([0]);
    h.update(content.as_bytes());
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256:{hex}")
}

fn bytes_line(field: &str, value: &str, max: usize, reasons: &mut Vec<String>) {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        reasons.push(format!(
            "{field} must be 1 to {max} bytes on one line, without control characters."
        ));
    }
}

fn chars_line(field: &str, value: &str, max: usize, reasons: &mut Vec<String>) {
    let n = value.chars().count();
    if value.trim().is_empty() || n > max || value.chars().any(char::is_control) {
        reasons.push(format!(
            "{field} must be 1 to {max} characters on one line."
        ));
    }
}

fn text(field: &str, value: &str, max: usize, may_be_empty: bool, reasons: &mut Vec<String>) {
    let bad_control = value
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t');
    if value.len() > max || bad_control || (!may_be_empty && value.is_empty()) {
        reasons.push(format!(
            "{field} must be at most {max} bytes; line breaks and tabs are the only control characters."
        ));
    }
}

pub(crate) fn valid_url(url: &str) -> bool {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    url.len() <= MAX_URL
        && rest.is_some_and(|r| !r.is_empty() && !r.starts_with('/'))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PublishError {
    #[error("run {0} is no longer the Routine's open run")]
    NotCurrent(String),
    #[error("no random source for an entry id")]
    Random,
}

/// Stores the new entries, skips keys the Routine already stored, commits
/// the state, and closes the run. A run that already published publishes
/// nothing again. Returns how many entries were stored.
pub fn publish(
    state: &mut RoutineState,
    run: &str,
    occurrence: &str,
    output: Output,
    now: u64,
    mut new_id: impl FnMut() -> Option<String>,
) -> Result<u32, PublishError> {
    if state.last_success.as_ref().is_some_and(|s| s.task == run) {
        return Ok(0);
    }
    if state.running.as_ref().and_then(|r| r.task.as_deref()) != Some(run) {
        return Err(PublishError::NotCurrent(run.to_owned()));
    }
    let mut published = 0;
    for item in output.items {
        if state.entries.iter().any(|s| s.entry.key == item.key) {
            continue;
        }
        let id = loop {
            let id = new_id().ok_or(PublishError::Random)?;
            if !state.entries.iter().any(|s| s.entry.id == id) {
                break id;
            }
        };
        state.entries.push(Stored {
            entry: InboxEntry {
                id,
                workspace: state.workspace.clone(),
                routine: state.name.clone(),
                key: item.key,
                kind: item.kind,
                title: item.title,
                body: item.body,
                links: item.links,
                created_at: now,
                run: run.to_owned(),
                occurrence: occurrence.to_owned(),
                disposition: Disposition::Open,
                read_at: None,
                later_at: None,
                until: None,
                proposal: item.proposal,
                task: None,
            },
            approving: None,
        });
        published += 1;
    }
    if let Some(next) = output.state {
        state.state = Some(next);
    }
    state.last_success = Some(RoutineSuccess {
        at: now,
        occurrence: occurrence.to_owned(),
        task: run.to_owned(),
        published,
    });
    state.running = None;
    Ok(published)
}

/// `e-` and 8 hex digits.
pub fn entry_id() -> Option<String> {
    getrandom::u32().ok().map(|n| format!("e-{n:08x}"))
}

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
