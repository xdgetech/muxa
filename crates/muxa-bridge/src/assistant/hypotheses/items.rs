//! `hypothesis` Inbox items (ADR 0050 §3): one per `proposed` hypothesis of
//! the newest plan, in the plan run's item file, keyed by the hypothesis so
//! a second scan makes none. The same text as an active policy or an open
//! hypothesis item makes no item.
use std::collections::HashSet;

use muxa_protocol::{
    Delivery, ErrorBody, Hypothesis, HypothesisProposal, InboxItem, ItemKind, ItemState, Plan,
};

use super::{normalized, open};
use crate::assistant::plan::{WORKSPACE_FIELD, newest_first};
use crate::assistant::policies::expired;
use crate::assistant::state_error;
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, TaskItems, new_id};
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The item's key in its file: the hypothesis it asks about.
pub fn key(hypothesis: &str) -> String {
    format!("hypothesis:{hypothesis}")
}

/// Blocking. Makes the items of the newest plan's open hypotheses. Failures
/// are logged and tried again next tick.
pub fn publish(config: &Config, now: u64) {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return;
    }
    if let Err(e) = make(config, now) {
        tracing::warn!(error = %e.message, "cannot make hypothesis items");
    }
}

fn make(config: &Config, now: u64) -> Result<(), ErrorBody> {
    let plans = crate::assistant::plan::shelf(config)?;
    let mut all = plans.all().map_err(|e| state_error(&e))?;
    all.retain(|p| p.delivery == Delivery::Resulted);
    newest_first(&mut all);
    let Some(newest) = all.into_iter().next() else {
        return Ok(());
    };
    let hypotheses: Vec<Hypothesis> = crate::assistant::shelf::<Hypothesis>(config)?
        .all()
        .map_err(|e| state_error(&e))?
        .into_iter()
        .filter(|h| h.plan == newest.id && open(h, now))
        .collect();
    let (Some(task), false) = (newest.run.clone(), hypotheses.is_empty()) else {
        return Ok(());
    };
    let overall = overall_of(config, &newest);
    let dir = config.state_dir.as_ref().ok_or_else(|| {
        crate::convert::error_body(muxa_protocol::ErrorCode::Internal, "no state directory")
    })?;
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let mut taken: HashSet<String> = crate::assistant::policies::list(config, now)?
        .iter()
        .filter(|p| !expired(p, now))
        .map(|p| normalized(&p.text))
        .collect();
    let others = store.all().map_err(|e| state_error(&e))?;
    taken.extend(
        others
            .iter()
            .filter(|t| t.task != task)
            .flat_map(open_texts),
    );
    store
        .update::<_, StateError>(&task, |items| {
            add(items, &hypotheses, &overall, &mut taken, now)
        })
        .map_err(|e| state_error(&e))
}

/// The overall workspace's name, as the plan recorded it.
fn overall_of(config: &Config, plan: &Plan) -> String {
    let stored = crate::assistant::plan::shelf(config)
        .ok()
        .and_then(|s| s.get_with_extra(&plan.id).ok().flatten())
        .and_then(|(_, extra)| {
            extra
                .get(WORKSPACE_FIELD)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    stored.unwrap_or_default()
}

/// Normalized texts of the open `hypothesis` items in `items`.
fn open_texts(items: &TaskItems) -> impl Iterator<Item = String> + '_ {
    items
        .items
        .iter()
        .filter(|s| matches!(s.item.state, ItemState::Open | ItemState::Later))
        .filter_map(|s| s.item.hypothesis.as_ref())
        .map(|h| normalized(&h.text))
}

/// Under the file's lock: one item per hypothesis.
fn add(
    items: &mut TaskItems,
    hypotheses: &[Hypothesis],
    overall: &str,
    taken: &mut HashSet<String>,
    now: u64,
) -> Result<(), StateError> {
    taken.extend(open_texts(items).collect::<Vec<_>>());
    for h in hypotheses {
        let key = key(&h.id);
        if items.has_key(&key) {
            continue;
        }
        if !taken.insert(normalized(&h.text)) {
            tracing::info!(hypothesis = %h.id, "hypothesis repeats a policy or an open item; no item");
            continue;
        }
        let item = item_for(h, overall, new_id()?, now);
        tracing::info!(hypothesis = %h.id, item = %item.id, "hypothesis item made");
        items.items.push(StoredItem { item, key });
    }
    Ok(())
}

fn item_for(h: &Hypothesis, overall: &str, id: String, at: u64) -> InboxItem {
    let mut item = InboxItem {
        memory: None,
        write: None,
        handoff: None,
        id,
        kind: ItemKind::Hypothesis,
        state: ItemState::Open,
        question: one_line(&format!("Make this a policy: {}", h.text), MAX_QUESTION),
        workspace: overall.to_owned(),
        title: one_line(&h.text, MAX_QUESTION),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary: None,
        task: None,
        session: None,
        pane: None,
        prompt: None,
        attachments: Default::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: Some(HypothesisProposal {
            hypothesis: h.id.clone(),
            text: h.text.clone(),
            scope: h.scope.clone(),
            evidence: h.evidence.clone(),
            confidence: h.confidence,
        }),
        answers: Vec::new(),
    };
    item.answers = crate::items::answers(&item);
    item
}

/// Blocking. Applies `change` to each item file that holds an item asking
/// about one of `hypotheses`, under its lock, with the items' keys. Returns
/// how many items it changed.
pub fn each(
    config: &Config,
    hypotheses: &HashSet<String>,
    change: impl Fn(&mut TaskItems, &HashSet<String>) -> usize,
) -> Result<usize, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(0);
    };
    if hypotheses.is_empty() {
        return Ok(0);
    }
    let keys: HashSet<String> = hypotheses.iter().map(|h| key(h)).collect();
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let files = store.all().map_err(|e| state_error(&e))?;
    let mut changed = 0;
    for file in files {
        if !file.items.iter().any(|s| keys.contains(&s.key)) {
            continue;
        }
        changed += store
            .update::<_, StateError>(&file.task, |items| Ok(change(items, &keys)))
            .map_err(|e| state_error(&e))?;
    }
    Ok(changed)
}

/// The items under `keys` that are still open become `resolved`.
pub fn resolve(items: &mut TaskItems, keys: &HashSet<String>) -> usize {
    let mut n = 0;
    for s in items.items.iter_mut().filter(|s| keys.contains(&s.key)) {
        if matches!(s.item.state, ItemState::Open | ItemState::Later) {
            s.item.state = ItemState::Resolved;
            s.item.until = None;
            s.item.answers = crate::items::answers(&s.item);
            n += 1;
        }
    }
    n
}
