//! `write` Inbox items (ADR 0052 §2): after a Result with `writes` is
//! stored, the daemon's tick makes one item per write in the Result's run
//! file, keyed so a second scan makes none, and links each write to its
//! item. An open item of the same request with the same hash makes no
//! second item. Actions, targets, contents and reasons never reach a log.
use std::collections::HashMap;

use muxa_protocol::{
    AssistResult, AssistantRequest, InboxItem, ItemKind, ItemState, ResultWrite, WriteProposal,
};

use super::store::Shelf;
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, new_id};
use crate::records::Record;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The item's key in its run's file: the write's place on its Result.
pub fn key(request: &str, version: u64, step: u32, index: usize) -> String {
    format!("write:{request}:{version}:{step}:{index}")
}

/// Blocking. Makes the items of every stored write without one.
pub fn scan(config: &Config) {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return;
    };
    let opened = Shelf::<AssistantRequest>::open(dir).and_then(|s| Ok((s.all()?, s)));
    let (all, shelf) = match opened {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read requests for their writes");
            return;
        }
    };
    let waiting: Vec<AssistantRequest> = all.into_iter().filter(unlinked).collect();
    if waiting.is_empty() {
        return;
    }
    let store = match ItemStore::open(dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot open the items for writes");
            return;
        }
    };
    let records = config.records_if_any();
    for q in &waiting {
        match make_all(&store, q, &records) {
            Ok(links) if links.is_empty() => {}
            Ok(links) => link(&shelf, q, &links),
            Err(e) => tracing::warn!(request = %q.id, error = %e, "cannot make write items"),
        }
    }
}

fn unlinked(q: &AssistantRequest) -> bool {
    q.results
        .iter()
        .any(|r| r.writes.iter().any(|w| w.item.is_none()))
}

/// `(version, step, index, item)` of each write that has its item now.
type Links = Vec<(u64, u32, usize, String)>;

fn make_all(
    store: &ItemStore,
    q: &AssistantRequest,
    records: &[Record],
) -> Result<Links, StateError> {
    let mut open: Option<HashMap<String, String>> = None;
    let mut links = Links::new();
    for r in &q.results {
        let step = r.step.unwrap_or(1);
        if !r.writes.iter().any(|w| w.item.is_none()) {
            continue;
        }
        let Some(run) = run_of(records, &q.id, r.version, step) else {
            tracing::debug!(request = %q.id, version = r.version, step, "a result's run is gone; its writes get no items");
            continue;
        };
        let mut made = 0;
        for (i, w) in r
            .writes
            .iter()
            .enumerate()
            .filter(|(_, w)| w.item.is_none())
        {
            let key = key(&q.id, r.version, step, i);
            let file = store.get(run)?;
            if let Some(s) = file.items.iter().find(|s| s.key == key) {
                links.push((r.version, step, i, s.item.id.clone()));
                continue;
            }
            if file.deleted.contains(&key) {
                continue;
            }
            if open.is_none() {
                open = Some(open_hashes(store, &q.id)?);
            }
            let hashes = open.get_or_insert_with(HashMap::new);
            if let Some(id) = hashes.get(&w.hash) {
                links.push((r.version, step, i, id.clone()));
                continue;
            }
            let item = item_for(q, r, w, new_id()?, crate::records::now());
            let id = item.id.clone();
            let added = store.update::<_, StateError>(run, |items| {
                if let Some(s) = items.items.iter().find(|s| s.key == key) {
                    return Ok(s.item.id.clone());
                }
                items.items.push(StoredItem { item, key });
                Ok(id)
            })?;
            hashes.insert(w.hash.clone(), added.clone());
            links.push((r.version, step, i, added));
            made += 1;
        }
        if made > 0 {
            tracing::info!(request = %q.id, version = r.version, step, task = run, items = made, "write items made");
        }
    }
    Ok(links)
}

/// The run record of `request`'s `version` and `step`.
fn run_of<'a>(records: &'a [Record], request: &str, version: u64, step: u32) -> Option<&'a str> {
    records
        .iter()
        .find(|r| {
            r.request_run().is_some_and(|run| {
                run.request == request && run.version == version && run.step() == step
            })
        })
        .map(|r| r.id.as_str())
}

/// The open `write` items of `request`, by hash.
fn open_hashes(store: &ItemStore, request: &str) -> Result<HashMap<String, String>, StateError> {
    let prefix = format!("write:{request}:");
    Ok(store
        .all()?
        .iter()
        .flat_map(|f| f.items.iter())
        .filter(|s| s.key.starts_with(&prefix))
        .filter(|s| matches!(s.item.state, ItemState::Open | ItemState::Later))
        .filter_map(|s| Some((s.item.write.as_ref()?.hash.clone(), s.item.id.clone())))
        .collect())
}

fn item_for(
    q: &AssistantRequest,
    r: &AssistResult,
    w: &ResultWrite,
    id: String,
    at: u64,
) -> InboxItem {
    let who = r.worker.as_deref().unwrap_or("The assistant");
    let question = format!("{who} proposes to write to {}", w.target);
    let mut item = InboxItem {
        id,
        kind: ItemKind::Write,
        state: ItemState::Open,
        question: one_line(&question, MAX_QUESTION),
        workspace: q.workspace.clone(),
        title: one_line(&w.action, MAX_QUESTION),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary: w.reason.clone(),
        task: None,
        session: None,
        pane: None,
        prompt: None,
        attachments: Default::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: None,
        handoff: None,
        write: Some(WriteProposal {
            request: q.id.clone(),
            version: r.version,
            step: r.step,
            worker: r.worker.clone(),
            action: w.action.clone(),
            target: w.target.clone(),
            content: w.content.clone(),
            hash: w.hash.clone(),
        }),
        answers: Vec::new(),
    };
    item.answers = crate::items::answers(&item);
    item
}

/// Blocking. Records each write's item on its Result.
fn link(shelf: &Shelf<AssistantRequest>, q: &AssistantRequest, links: &Links) {
    let linked = shelf.update::<_, StateError>(&q.id, |q| {
        for (version, step, i, id) in links {
            let found = q
                .results
                .iter_mut()
                .find(|r| r.version == *version && r.step.unwrap_or(1) == *step)
                .and_then(|r| r.writes.get_mut(*i));
            if let Some(w) = found {
                w.item.get_or_insert_with(|| id.clone());
            }
        }
        Ok(())
    });
    if let Err(e) = linked {
        tracing::warn!(request = %q.id, error = %e, "cannot link writes to their items");
    }
}

/// Blocking. Records the writer's Task on the write item `item` names.
pub fn record_task(config: &Config, request: &str, item: &str, task: &str) {
    let Ok(shelf) = super::shelf::<AssistantRequest>(config) else {
        return;
    };
    let kept = shelf.update::<_, StateError>(request, |q| {
        let found = q
            .results
            .iter_mut()
            .flat_map(|r| r.writes.iter_mut())
            .filter(|w| w.item.as_deref() == Some(item));
        for w in found {
            w.task = Some(task.to_owned());
        }
        Ok(())
    });
    if let Err(e) = kept {
        tracing::warn!(request, item, error = %e, "cannot record a write's Task on its result");
    }
}
