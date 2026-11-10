//! What a plan may cite (ADR 0050 §3, §4): the ids its brief carried, kept
//! on the plan record where the run cannot write, and still live in the
//! store. A forgotten or deleted record cannot be a reason, and a guess
//! grounded in Later alone, or in one workspace's habits, is refused.
use std::collections::{BTreeMap, HashSet};

use muxa_protocol::{Correction, CorrectionKind, Hypothesis, Policy};
use serde::{Deserialize, Serialize};

use super::file::{Checked, cited};
use crate::assistant::hypotheses::{self, NewHypothesis};
use crate::assistant::policies::{ALL, expired};
use crate::assistant::store::Extra;
use crate::records::Workspaces;
use crate::records::assistant::Role;

/// The plan file's field for what its brief carried.
pub const FIELD: &str = "carried";

/// The ids a run's brief carried, and the hypotheses it may propose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carried {
    #[serde(default)]
    pub policies: Vec<String>,
    /// `proposed` only: a rejected hypothesis is never a reason.
    #[serde(default)]
    pub hypotheses: Vec<String>,
    #[serde(default)]
    pub corrections: Vec<String>,
    #[serde(default)]
    pub slots: usize,
    /// The staff members the brief listed, by workspace (ADR 0051 §2): the
    /// workers a `delegate` may name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub staff: BTreeMap<String, Vec<String>>,
}

#[cfg(test)]
static NOTHING: Carried = Carried {
    policies: Vec::new(),
    hypotheses: Vec::new(),
    corrections: Vec::new(),
    slots: 0,
    staff: BTreeMap::new(),
};

impl Carried {
    /// What the plan file records. A plan briefed before ADR 0050 carried
    /// nothing, so it may cite nothing.
    pub fn of(extra: &Extra) -> Self {
        extra
            .get(FIELD)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    pub fn put(&self, extra: &mut Extra) {
        match serde_json::to_value(self) {
            Ok(v) => {
                extra.insert(FIELD.into(), v);
            }
            Err(e) => tracing::warn!(error = %e, "cannot record what a brief carried"),
        }
    }
}

/// What the user told the overall assistant, as the store holds it at the check.
#[derive(Debug, Clone, Copy)]
pub struct Told<'a> {
    pub carried: &'a Carried,
    pub policies: &'a [Policy],
    pub hypotheses: &'a [Hypothesis],
    pub corrections: &'a [Correction],
    /// Epoch seconds.
    pub now: u64,
}

#[cfg(test)]
impl Told<'static> {
    /// Nothing carried: no citation and no hypothesis passes.
    pub fn nothing() -> Self {
        Told {
            carried: &NOTHING,
            policies: &[],
            hypotheses: &[],
            corrections: &[],
            now: 0,
        }
    }
}

impl Told<'_> {
    /// Whether the brief listed `worker` on `workspace`'s staff.
    pub fn listed(&self, workspace: &str, worker: &str) -> bool {
        self.carried
            .staff
            .get(workspace)
            .is_some_and(|names| names.iter().any(|n| n == worker))
    }

    fn live(&self, record: &str, id: &str) -> bool {
        let (carried, live) = match record {
            "policy" => (
                &self.carried.policies,
                self.policies
                    .iter()
                    .any(|p| p.id == id && !expired(p, self.now)),
            ),
            "hypothesis" => (
                &self.carried.hypotheses,
                self.hypotheses
                    .iter()
                    .any(|h| h.id == id && hypotheses::open(h, self.now)),
            ),
            _ => (
                &self.carried.corrections,
                self.corrections.iter().any(|c| c.id == id),
            ),
        };
        live && carried.iter().any(|c| c == id)
    }

    fn correction(&self, id: &str) -> Option<&Correction> {
        self.live("correction", id)
            .then(|| self.corrections.iter().find(|c| c.id == id))
            .flatten()
    }
}

/// Every citation and hypothesis of `checked` that the brief and the store do not back.
pub fn reasons(checked: &Checked, told: &Told<'_>, workspaces: &Workspaces) -> Vec<String> {
    let mut reasons = Vec::new();
    for (i, row) in checked.top.iter().enumerate() {
        for (j, b) in row.basis.iter().enumerate() {
            let (Some((record, _)), Some(source)) = (cited(b.kind), &b.source) else {
                continue;
            };
            if !told.live(record, source) {
                reasons.push(format!(
                    "top[{i}].basis[{j}].source is not a {record} in \"What the user told \
                     you\": cite only the ids listed there."
                ));
            }
        }
    }
    let live_slots = hypotheses::slots(told.hypotheses, told.now);
    let slots = told.carried.slots.min(live_slots);
    if checked.hypotheses.len() > slots {
        reasons.push(format!(
            "hypotheses has {} entries; this plan may propose {slots}.",
            checked.hypotheses.len()
        ));
    }
    let overall = workspaces.overall().map(|w| w.name.as_str());
    for (k, h) in checked.hypotheses.iter().enumerate() {
        hypothesis(
            &format!("hypotheses[{k}]"),
            h,
            told,
            workspaces,
            overall,
            &mut reasons,
        );
    }
    reasons
}

fn hypothesis(
    at: &str,
    h: &NewHypothesis,
    told: &Told<'_>,
    workspaces: &Workspaces,
    overall: Option<&str>,
    reasons: &mut Vec<String>,
) {
    if h.scope != ALL {
        let ok = workspaces
            .get(&h.scope)
            .is_some_and(|w| w.role != Role::Overall);
        if !ok {
            reasons.push(format!(
                "{at}.scope must be all or a workspace on this host other than yours."
            ));
        }
    }
    let mut evidence = Vec::new();
    for (e, x) in h.evidence.iter().enumerate() {
        match told.correction(x) {
            Some(c) => evidence.push(c),
            None => reasons.push(format!(
                "{at}.evidence[{e}] is not a correction in \"What the user told you\": cite \
                 only the ids listed there."
            )),
        }
    }
    if evidence.is_empty() {
        return;
    }
    if evidence.iter().all(|c| c.kind == CorrectionKind::Deferred) {
        reasons.push(format!(
            "{at}.evidence is deferred corrections only. Later means not now: it is no \
             ground for a hypothesis."
        ));
    }
    // Nobody's rows and the overall workspace's are the user's own.
    let homes: HashSet<Option<&str>> = evidence
        .iter()
        .map(|c| c.workspace.as_deref().filter(|w| Some(*w) != overall))
        .collect();
    let one_workspace = homes.len() == 1 && homes.iter().all(Option::is_some);
    if h.scope == ALL && one_workspace {
        reasons.push(format!(
            "{at}: scope all needs evidence beyond one workspace. Its evidence is all from \
             one workspace: scope it to that workspace."
        ));
    }
}

#[cfg(test)]
#[path = "cite_tests.rs"]
mod tests;
