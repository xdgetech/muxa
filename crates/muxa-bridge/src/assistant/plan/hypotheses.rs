//! The plan file's `hypotheses[]` (ADR 0048 §3), checked on their own:
//! shape, sizes and ids. `cite` holds them against the brief and the store.
//! Reasons name fields and indices, never values.
use std::collections::HashSet;

use muxa_protocol::Confidence;
use serde::Deserialize;

use crate::assistant::hypotheses::{MAX_PER_PLAN, NewHypothesis, is_id};
use crate::assistant::result::{MAX_LINE, count};

/// Correction ids one hypothesis cites.
const MAX_EVIDENCE: usize = 5;
/// Bytes of a workspace name.
const MAX_NAME: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    text: String,
    scope: String,
    evidence: Vec<String>,
    confidence: String,
}

/// Each value parsed alone, so a reason names its index.
pub fn parse(raw: Vec<serde_json::Value>, reasons: &mut Vec<String>) -> Vec<NewHypothesis> {
    count("hypotheses", raw.len(), MAX_PER_PLAN, reasons);
    raw.into_iter()
        .enumerate()
        .filter_map(|(i, v)| one(&format!("hypotheses[{i}]"), v, reasons))
        .collect()
}

fn one(at: &str, value: serde_json::Value, reasons: &mut Vec<String>) -> Option<NewHypothesis> {
    let Ok(h) = serde_json::from_value::<Raw>(value) else {
        reasons.push(format!(
            "{at} does not match the schema: text, scope, evidence and confidence, each of \
             the right type, and nothing else."
        ));
        return None;
    };
    // One paragraph, so that confirm can make a policy of it unchanged.
    let paragraph = !h.text.trim().is_empty()
        && h.text.len() <= MAX_LINE
        && !h.text.chars().any(char::is_control);
    if !paragraph {
        reasons.push(format!(
            "{at}.text must be 1 to {MAX_LINE} bytes, one paragraph without control characters."
        ));
    }
    let named = !h.scope.trim().is_empty()
        && h.scope.len() <= MAX_NAME
        && !h.scope.chars().any(char::is_control);
    if !named {
        reasons.push(format!("{at}.scope must be all or a workspace's name."));
    }
    if h.evidence.is_empty() || h.evidence.len() > MAX_EVIDENCE {
        reasons.push(format!(
            "{at}.evidence must name 1 to {MAX_EVIDENCE} corrections."
        ));
    }
    let mut seen = HashSet::new();
    for (j, x) in h.evidence.iter().enumerate() {
        if !is_id(x, "x-") {
            reasons.push(format!(
                "{at}.evidence[{j}] must be a correction's id: x- and 8 hex digits."
            ));
        } else if !seen.insert(x.as_str()) {
            reasons.push(format!("{at}.evidence[{j}] names a correction twice."));
        }
    }
    let confidence = match h.confidence.as_str() {
        "low" => Some(Confidence::Low),
        "medium" => Some(Confidence::Medium),
        _ => {
            reasons.push(format!("{at}.confidence must be low or medium."));
            None
        }
    };
    Some(NewHypothesis {
        text: h.text,
        scope: h.scope,
        evidence: h.evidence,
        confidence: confidence?,
    })
}
