//! The overall run's plan file, `.muxa/assistant/plan.json` (ADR 0049 §5,
//! schema in PROTOCOL.md "Plan file"): read and checked on its own, before
//! `check` holds it against the store. Reasons name fields and indices,
//! never values.
use std::collections::HashMap;
use std::path::Path;

use muxa_protocol::{
    AssistScope, Basis, BasisKind, DelegateProposal, Minutes, Notes, PlanRow, SortAction,
    SortProposal,
};
use serde::{Deserialize, Serialize};

use crate::assistant::hypotheses::{NewHypothesis, is_id};
use crate::assistant::record::PlanRun;
use crate::assistant::result::{
    MAX_LINE, MAX_TITLE, count, is_work_id, one_line, parse_reason, proposal, read_file, text,
};

pub const PLAN: &str = ".muxa/assistant/plan.json";
const NAME: &str = "plan.json";
const SECTION: &str = "Plan file";
pub const VERSION: u32 = 1;
pub const MAX_TOP: usize = 3;
const MAX_BASIS: usize = 5;
const MAX_MISSING: usize = 5;
const MAX_PROPOSALS: usize = 10;
/// A day: no row takes longer.
const MAX_MINUTES: u32 = 1440;
const MAX_SOURCE: usize = 2048;
/// Bytes: the ask a receiving workspace reads.
const MAX_PURPOSE: usize = 1024;
/// Bytes of an Inbox item id or a workspace name.
const MAX_NAME: usize = 200;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlan {
    version: u32,
    plan: String,
    #[serde(default)]
    top: Vec<RawRow>,
    /// Each one parsed alone, so a reason names its index.
    #[serde(default)]
    proposals: Vec<serde_json::Value>,
    notes: Option<String>,
    /// Each one parsed alone, as proposals are.
    #[serde(default)]
    hypotheses: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    work: Option<String>,
    item: Option<String>,
    why: String,
    next: String,
    minutes: RawMinutes,
    #[serde(default)]
    basis: Vec<RawBasis>,
    #[serde(default)]
    missing: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMinutes {
    low: u32,
    high: u32,
    estimated: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBasis {
    kind: String,
    text: String,
    source: Option<String>,
    at: Option<u64>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum RawProposal {
    Sort {
        capture: String,
        action: String,
        workspace: Option<String>,
        title: Option<String>,
        work: Option<String>,
    },
    Delegate {
        work: String,
        workspace: String,
        scope: String,
        purpose: String,
        reason: String,
        worker: Option<String>,
    },
}

/// A checked proposal, kept on the plan until it becomes an Inbox item (slice B5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Proposal {
    Sort(SortProposal),
    Delegate(DelegateProposal),
}

/// What the file holds once its own rules pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub top: Vec<PlanRow>,
    pub proposals: Vec<Proposal>,
    pub notes: Option<Notes>,
    pub hypotheses: Vec<NewHypothesis>,
}

/// Reads the plan in `dir` for `run`. The reasons are for the agent's check log.
pub fn read(dir: &Path, run: &PlanRun) -> Result<Checked, Vec<String>> {
    let bytes = read_file(dir, NAME).map_err(|r| vec![r])?;
    validate(&bytes, run)
}

/// Every reason the file fails its own rules, or what it holds.
pub fn validate(bytes: &[u8], run: &PlanRun) -> Result<Checked, Vec<String>> {
    if bytes.contains(&0) {
        return Err(vec![format!("{PLAN} contains a NUL byte.")]);
    }
    if std::str::from_utf8(bytes).is_err() {
        return Err(vec![format!("{PLAN} is not UTF-8.")]);
    }
    let raw: RawPlan =
        serde_json::from_slice(bytes).map_err(|e| vec![parse_reason(&e, PLAN, SECTION)])?;
    let mut reasons = Vec::new();
    if raw.version != VERSION {
        reasons.push(format!("version must be {VERSION}."));
    }
    if raw.plan != run.plan {
        reasons.push(format!(
            "plan must be {}, the plan this run answers.",
            run.plan
        ));
    }
    count("top", raw.top.len(), MAX_TOP, &mut reasons);
    let top = rows(raw.top, &mut reasons);
    count(
        "proposals",
        raw.proposals.len(),
        MAX_PROPOSALS,
        &mut reasons,
    );
    let proposals = proposals(raw.proposals, &mut reasons);
    let notes = raw
        .notes
        .and_then(|t| proposal(t, Some(&run.notes_base), &mut reasons));
    let hypotheses = super::hypotheses::parse(raw.hypotheses, &mut reasons);
    if !reasons.is_empty() {
        return Err(reasons);
    }
    Ok(Checked {
        top,
        proposals,
        notes,
        hypotheses,
    })
}

fn rows(raw: Vec<RawRow>, reasons: &mut Vec<String>) -> Vec<PlanRow> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(raw.len());
    for (i, r) in raw.into_iter().enumerate() {
        let at = format!("top[{i}]");
        match (&r.work, &r.item) {
            (Some(w), None) if !is_work_id(w) => reasons.push(format!(
                "{at}.work must be a Work item's id: w- and 8 hex digits."
            )),
            (None, Some(item)) if !is_name(item) => reasons.push(format!(
                "{at}.item must be an Inbox item's id from the brief."
            )),
            (Some(_), None) | (None, Some(_)) => {}
            _ => reasons.push(format!("{at} must name exactly one of work or item.")),
        }
        if let Some(id) = r.work.as_ref().or(r.item.as_ref()) {
            if let Some(j) = seen.insert(id.clone(), i) {
                reasons.push(format!("{at} names the same item as top[{j}]."));
            }
        }
        text(&format!("{at}.why"), &r.why, MAX_LINE, reasons);
        text(&format!("{at}.next"), &r.next, MAX_LINE, reasons);
        let m = &r.minutes;
        if !(m.low <= m.estimated && m.estimated <= m.high && m.high <= MAX_MINUTES) {
            reasons.push(format!(
                "{at}.minutes must be whole numbers with low <= estimated <= high <= {MAX_MINUTES}."
            ));
        }
        count(&format!("{at}.basis"), r.basis.len(), MAX_BASIS, reasons);
        let basis = r
            .basis
            .into_iter()
            .enumerate()
            .filter_map(|(j, b)| basis(&format!("{at}.basis[{j}]"), b, reasons))
            .collect();
        count(
            &format!("{at}.missing"),
            r.missing.len(),
            MAX_MISSING,
            reasons,
        );
        for (j, m) in r.missing.iter().enumerate() {
            text(&format!("{at}.missing[{j}]"), m, MAX_LINE, reasons);
        }
        out.push(PlanRow {
            work: r.work,
            item: r.item,
            why: r.why,
            next: r.next,
            minutes: Minutes {
                low: r.minutes.low,
                high: r.minutes.high,
                estimated: r.minutes.estimated,
            },
            basis,
            missing: r.missing,
            title: None,
            target_state: None,
        });
    }
    out
}

fn basis(at: &str, b: RawBasis, reasons: &mut Vec<String>) -> Option<Basis> {
    let kind = match b.kind.as_str() {
        "due" => Some(BasisKind::Due),
        "dependency" => Some(BasisKind::Dependency),
        "goal" => Some(BasisKind::Goal),
        "time" => Some(BasisKind::Time),
        "manual" => Some(BasisKind::Manual),
        "policy" => Some(BasisKind::Policy),
        "hypothesis" => Some(BasisKind::Hypothesis),
        "correction" => Some(BasisKind::Correction),
        _ => None,
    };
    if kind.is_none() {
        reasons.push(format!(
            "{at}.kind must be due, dependency, goal, time, manual, policy, hypothesis or \
             correction."
        ));
    }
    if let Some((record, prefix)) = kind.and_then(cited) {
        if !b.source.as_deref().is_some_and(|s| is_id(s, prefix)) {
            reasons.push(format!(
                "{at}.source must be the id of the {record} it cites: {prefix} and 8 hex digits."
            ));
        }
    }
    text(&format!("{at}.text"), &b.text, MAX_LINE, reasons);
    let bad_source =
        |s: &String| s.trim().is_empty() || s.len() > MAX_SOURCE || s.chars().any(char::is_control);
    if b.source.as_ref().is_some_and(bad_source) {
        reasons.push(format!(
            "{at}.source must be a URL, a path or an id of at most {MAX_SOURCE} bytes, \
             without control characters."
        ));
    }
    Some(Basis {
        kind: kind?,
        text: b.text,
        source: b.source,
        at: b.at,
    })
}

/// A record a `basis` of `kind` cites, and its ids' prefix.
pub fn cited(kind: BasisKind) -> Option<(&'static str, &'static str)> {
    match kind {
        BasisKind::Policy => Some(("policy", "y-")),
        BasisKind::Hypothesis => Some(("hypothesis", "h-")),
        BasisKind::Correction => Some(("correction", "x-")),
        _ => None,
    }
}

fn proposals(raw: Vec<serde_json::Value>, reasons: &mut Vec<String>) -> Vec<Proposal> {
    let mut delegates: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::with_capacity(raw.len());
    for (i, value) in raw.into_iter().enumerate() {
        let at = format!("proposals[{i}]");
        let Ok(p) = serde_json::from_value::<RawProposal>(value) else {
            reasons.push(format!(
                "{at} does not match the schema: kind is \"sort\" or \"delegate\", with only \
                 that kind's fields, each of the right type."
            ));
            continue;
        };
        let checked = match p {
            RawProposal::Sort {
                capture,
                action,
                workspace,
                title,
                work,
            } => sort(&at, capture, &action, workspace, title, work, reasons),
            RawProposal::Delegate {
                work,
                workspace,
                scope,
                purpose,
                reason,
                worker,
            } => {
                if let Some(j) = delegates.insert(workspace.clone(), i) {
                    reasons.push(format!(
                        "{at} asks the same workspace as proposals[{j}]; at most one delegate \
                         proposal per workspace."
                    ));
                }
                let asked = Asked {
                    work,
                    workspace,
                    purpose,
                    reason,
                    worker,
                };
                delegate(&at, asked, &scope, reasons)
            }
        };
        out.extend(checked);
    }
    out
}

fn sort(
    at: &str,
    capture: String,
    action: &str,
    workspace: Option<String>,
    title: Option<String>,
    work: Option<String>,
    reasons: &mut Vec<String>,
) -> Option<Proposal> {
    if !is_capture_id(&capture) {
        reasons.push(format!(
            "{at}.capture must be a Capture's id: c- and 8 hex digits."
        ));
    }
    let action = match action {
        "work" => Some(SortAction::Work),
        "link" => Some(SortAction::Link),
        "note" => Some(SortAction::Note),
        _ => {
            reasons.push(format!("{at}.action must be work, link or note."));
            None
        }
    };
    let link = action == Some(SortAction::Link);
    if link != work.is_some() || work.as_deref().is_some_and(|w| !is_work_id(w)) {
        reasons.push(format!(
            "{at}.work must be a Work item's id with action link, and absent otherwise."
        ));
    }
    if workspace.as_deref().is_some_and(|w| !is_name(w)) {
        reasons.push(format!("{at}.workspace must be a workspace's name."));
    }
    // As sort-capture takes them, so an approval can run unchanged.
    let work_action = action.is_none_or(|a| a == SortAction::Work);
    if !work_action && (workspace.is_some() || title.is_some()) {
        reasons.push(format!(
            "{at}: workspace and title go with action work only."
        ));
    }
    if let Some(t) = &title {
        one_line(&format!("{at}.title"), t, MAX_TITLE, reasons);
    }
    Some(Proposal::Sort(SortProposal {
        capture,
        action: action?,
        workspace,
        title,
        work,
    }))
}

/// A delegate proposal's text fields, as the file holds them.
struct Asked {
    work: String,
    workspace: String,
    purpose: String,
    reason: String,
    worker: Option<String>,
}

fn delegate(at: &str, a: Asked, scope: &str, reasons: &mut Vec<String>) -> Option<Proposal> {
    let Asked {
        work,
        workspace,
        purpose,
        reason,
        worker,
    } = a;
    if !is_work_id(&work) {
        reasons.push(format!(
            "{at}.work must be a Work item's id: w- and 8 hex digits."
        ));
    }
    if !is_name(&workspace) {
        reasons.push(format!("{at}.workspace must be a workspace's name."));
    }
    let scope = match scope {
        "research" => Some(AssistScope::Research),
        "compare" => Some(AssistScope::Compare),
        "draft" => Some(AssistScope::Draft),
        _ => {
            reasons.push(format!("{at}.scope must be research, compare or draft."));
            None
        }
    };
    if worker.as_deref().is_some_and(|w| !is_name(w)) {
        reasons.push(format!("{at}.worker must be a staff member's name."));
    }
    text(&format!("{at}.purpose"), &purpose, MAX_PURPOSE, reasons);
    text(&format!("{at}.reason"), &reason, MAX_LINE, reasons);
    Some(Proposal::Delegate(DelegateProposal {
        worker,
        work,
        workspace,
        scope: scope?,
        purpose,
        reason,
    }))
}

fn is_capture_id(id: &str) -> bool {
    is_id(id, "c-")
}

/// An id or a name to look up: one line, at most [`MAX_NAME`] bytes.
fn is_name(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_NAME && !value.chars().any(char::is_control)
}

#[cfg(test)]
#[path = "file_tests.rs"]
mod tests;
