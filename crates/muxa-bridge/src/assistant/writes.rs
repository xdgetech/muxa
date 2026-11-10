//! A Result's proposed writes (ADR 0052 §1): "write this text there". The
//! check validates shape only: sizes, lines, control characters and count,
//! and refuses any write where the workspace takes none. It never reads a
//! target or a content; reasons name fields and indices, never values.
use muxa_protocol::ResultWrite;
use serde::Deserialize;

use super::result::{MAX_LINE, text};
use crate::routines::output::proposal_hash;

/// Writes one Result may propose.
pub const MAX_WRITES: usize = 3;
/// The Routine proposal's limits (ADR 0026, `PROTOCOL.md` "Routine output").
const MAX_ACTION: usize = 200;
const MAX_TARGET: usize = 500;
const MAX_CONTENT: usize = 8192;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawWrite {
    action: String,
    target: String,
    content: String,
    reason: Option<String>,
}

/// The writes as stored, with their hash, or none with the reasons they
/// fail. `takes` is false where the workspace has no active writer.
pub(super) fn check(
    raw: Vec<RawWrite>,
    takes: bool,
    reasons: &mut Vec<String>,
) -> Vec<ResultWrite> {
    if raw.is_empty() {
        return Vec::new();
    }
    if !takes {
        reasons.push(
            "writes: this workspace takes no write proposals; put each change you suggest \
             outside this host in next instead, and leave writes out."
                .into(),
        );
        return Vec::new();
    }
    let before = reasons.len();
    if raw.len() > MAX_WRITES {
        reasons.push(format!("writes has more than {MAX_WRITES} entries."));
    }
    for (i, w) in raw.iter().enumerate() {
        let at = format!("writes[{i}]");
        let n = w.action.chars().count();
        if w.action.trim().is_empty() || n > MAX_ACTION || w.action.chars().any(char::is_control) {
            reasons.push(format!(
                "{at}.action must be 1 to {MAX_ACTION} characters on one line."
            ));
        }
        if w.target.trim().is_empty()
            || w.target.len() > MAX_TARGET
            || w.target.chars().any(char::is_control)
        {
            reasons.push(format!(
                "{at}.target must be 1 to {MAX_TARGET} bytes on one line, without control \
                 characters."
            ));
        }
        text(&format!("{at}.content"), &w.content, MAX_CONTENT, reasons);
        if let Some(r) = &w.reason {
            text(&format!("{at}.reason"), r, MAX_LINE, reasons);
        }
    }
    if reasons.len() > before {
        return Vec::new();
    }
    raw.into_iter()
        .map(|w| ResultWrite {
            hash: proposal_hash(&w.action, &w.target, &w.content),
            action: w.action,
            target: w.target,
            content: w.content,
            reason: w.reason,
            item: None,
            task: None,
        })
        .collect()
}

#[cfg(test)]
#[path = "writes_tests.rs"]
mod tests;
