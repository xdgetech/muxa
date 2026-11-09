//! Proposals become Inbox items once their plan is stored: one per plan and
//! proposal index, in the plan run's item file, so a second scan makes none.
//! Only the newest plan's proposals are asked; an older plan's are
//! superseded. A proposal whose Capture or Work item is already asked about
//! by an open item, or that no longer applies, makes no item.
use std::collections::{HashMap, HashSet};

use muxa_protocol::{
    Capture, CaptureState, Delivery, ErrorBody, InboxItem, ItemKind, ItemState, OverallRequests,
    Plan, SortAction, WorkItem,
};

use super::approval;
use super::approve::{self, Found};
use crate::assistant::brief::scope_name;
use crate::assistant::plan::check::PROPOSALS_FIELD;
use crate::assistant::plan::file::Proposal;
use crate::assistant::plan::{WORKSPACE_FIELD, newest_first};
use crate::assistant::policy;
use crate::assistant::store::{Extra, Shelf};
use crate::assistant::{shelf, state_error};
use crate::items::produce::{MAX_QUESTION, one_line};
use crate::items::store::{ItemStore, StoredItem, TaskItems, new_id};
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The plan file's mark: its proposals were made into items.
pub const PUBLISHED_FIELD: &str = "proposalsPublished";

/// A delegate item the host may send itself (`overall_requests = "auto"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Auto {
    pub item: String,
    pub workspace: String,
}

/// The item's key in its file: what made it.
pub fn key(plan: &str, index: usize) -> String {
    format!("proposal:{plan}:{index}")
}

/// Blocking. Makes the items of every stored plan not yet published, and
/// returns the delegate items the host may send, at most one per workspace.
pub fn publish(config: &Config) -> Vec<Auto> {
    if !config.state_dir.as_ref().is_some_and(|d| d.is_dir()) {
        return Vec::new();
    }
    let opened = crate::assistant::plan::shelf(config).and_then(|s| {
        let all = s.all().map_err(|e| state_error(&e))?;
        Ok((s, all))
    });
    let (plans, mut all) = match opened {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e.message, "cannot read plans for their proposals");
            return Vec::new();
        }
    };
    all.retain(|p| p.delivery == Delivery::Resulted);
    newest_first(&mut all);
    let mut autos = Vec::new();
    for (i, p) in all.iter().enumerate() {
        let Ok(Some((p, extra))) = plans.get_with_extra(&p.id) else {
            continue;
        };
        if extra.contains_key(PUBLISHED_FIELD) {
            continue;
        }
        let proposals = proposals_of(&extra);
        if i == 0 && !proposals.is_empty() {
            match make(config, &p, &extra, &proposals) {
                Ok(found) => autos.extend(found),
                Err(e) => {
                    tracing::warn!(plan = %p.id, error = %e.message, "cannot make a plan's proposal items");
                    continue;
                }
            }
        }
        mark(&plans, &p.id);
    }
    autos
}

fn proposals_of(extra: &Extra) -> Vec<Proposal> {
    extra
        .get(PROPOSALS_FIELD)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Bookkeeping only: the plan's `rev` stays, so `apply-notes` still matches.
fn mark(plans: &Shelf<Plan>, id: &str) {
    let marked = plans.update_with_extra::<_, StateError>(id, |_, extra| {
        extra.insert(PUBLISHED_FIELD.into(), true.into());
        Ok(())
    });
    if let Err(e) = marked {
        tracing::warn!(plan = %id, error = %e, "cannot mark a plan's proposals published");
    }
}

/// What items are made from: the store as it is now.
struct Facts {
    overall: String,
    work: HashMap<String, WorkItem>,
    captures: HashMap<String, Capture>,
    /// Targets of the open proposal items in other files.
    asked: HashSet<String>,
}

fn make(
    config: &Config,
    p: &Plan,
    extra: &Extra,
    proposals: &[Proposal],
) -> Result<Vec<Auto>, ErrorBody> {
    let Some(task) = p.run.clone() else {
        return Ok(Vec::new());
    };
    let dir = config.state_dir.as_ref().ok_or_else(|| {
        crate::convert::error_body(muxa_protocol::ErrorCode::Internal, "no state directory")
    })?;
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let others = store.all().map_err(|e| state_error(&e))?;
    let facts = Facts {
        overall: extra
            .get(WORKSPACE_FIELD)
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        work: by_id(shelf::<WorkItem>(config)?.all(), |w| w.id.clone())?,
        captures: by_id(shelf::<Capture>(config)?.all(), |c| c.id.clone())?,
        asked: others
            .iter()
            .filter(|t| t.task != task)
            .flat_map(open_targets)
            .collect(),
    };
    let candidates = store
        .update::<_, StateError>(&task, |items| {
            add(items, &p.id, proposals, &facts, p.delivery_at)
        })
        .map_err(|e| state_error(&e))?;
    let workspaces = config.load_workspaces()?;
    let mut seen = HashSet::new();
    Ok(candidates
        .into_iter()
        .filter(|a| workspaces.overall_requests(&a.workspace) == OverallRequests::Auto)
        .filter(|a| seen.insert(a.workspace.clone()))
        .collect())
}

fn by_id<T>(
    all: Result<Vec<T>, StateError>,
    id: impl Fn(&T) -> String,
) -> Result<HashMap<String, T>, ErrorBody> {
    Ok(all
        .map_err(|e| state_error(&e))?
        .into_iter()
        .map(|t| (id(&t), t))
        .collect())
}

/// Under the file's lock: one item per proposal index. Returns the open,
/// never approved delegate items.
fn add(
    items: &mut TaskItems,
    plan: &str,
    proposals: &[Proposal],
    facts: &Facts,
    at: u64,
) -> Result<Vec<Auto>, StateError> {
    let mut asked: HashSet<String> = open_targets(items).collect();
    let mut made = 0;
    for (i, proposal) in proposals.iter().enumerate() {
        let key = key(plan, i);
        if items.items.iter().any(|s| s.key == key) {
            continue;
        }
        let target = target(proposal);
        if facts.asked.contains(&target) || asked.contains(&target) {
            tracing::info!(
                plan,
                index = i,
                "proposal already asked about by an open item"
            );
            continue;
        }
        let Some(item) = item_for(proposal, facts, new_id()?, at) else {
            tracing::info!(plan, index = i, "proposal no longer applies; no item");
            continue;
        };
        tracing::info!(plan, index = i, item = %item.id, kind = ?item.kind, "proposal item made");
        asked.insert(target);
        items.items.push(StoredItem { item, key });
        made += 1;
    }
    if made > 0 {
        tracing::info!(plan, items = made, "plan proposals in the inbox");
    }
    Ok(items
        .items
        .iter()
        .filter(|s| s.key.starts_with(&format!("proposal:{plan}:")))
        .filter(|s| s.item.state == ItemState::Open && s.item.kind == ItemKind::Delegate)
        .filter(|s| {
            approval::hash(&s.item).is_some_and(|h| approval::get(items, &s.item.id, &h).is_none())
        })
        .filter_map(|s| {
            Some(Auto {
                item: s.item.id.clone(),
                workspace: s.item.delegate.as_ref()?.workspace.clone(),
            })
        })
        .collect())
}

/// What a proposal asks about: its Capture, or its Work item and workspace.
fn target(p: &Proposal) -> String {
    match p {
        Proposal::Sort(s) => format!("sort:{}", s.capture),
        Proposal::Delegate(d) => format!("delegate:{}:{}", d.work, d.workspace),
    }
}

fn open_targets(items: &TaskItems) -> impl Iterator<Item = String> + '_ {
    items
        .items
        .iter()
        .filter(|s| matches!(s.item.state, ItemState::Open | ItemState::Later))
        .filter_map(|s| match (&s.item.sort, &s.item.delegate) {
            (Some(p), _) => Some(target(&Proposal::Sort(p.clone()))),
            (_, Some(p)) => Some(target(&Proposal::Delegate(p.clone()))),
            _ => None,
        })
}

/// The item, or None when the Capture is sorted or the Work item closed.
fn item_for(p: &Proposal, facts: &Facts, id: String, at: u64) -> Option<InboxItem> {
    let open_work = |id: &str| {
        facts
            .work
            .get(id)
            .filter(|w| !crate::assistant::work::closed(w.state))
    };
    let (question, title) = match p {
        Proposal::Sort(s) => {
            let c = facts
                .captures
                .get(&s.capture)
                .filter(|c| c.state == CaptureState::Received)?;
            let line = first_line(&c.text);
            match s.action {
                SortAction::Work => {
                    let title = s.title.clone().unwrap_or(line);
                    let question = match s.workspace.as_ref().or(c.workspace.as_ref()) {
                        Some(ws) => format!("Make a Work item in {ws}: {title}"),
                        None => format!("Make a Work item: {title}"),
                    };
                    (question, title)
                }
                SortAction::Link => {
                    let w = open_work(s.work.as_deref()?)?;
                    (format!("Add the Capture to: {}", w.title), w.title.clone())
                }
                SortAction::Note => (format!("Keep the Capture as a note: {line}"), line),
            }
        }
        Proposal::Delegate(d) => {
            let w = open_work(&d.work)?;
            let question = format!(
                "Ask {} to {}: {}",
                d.workspace,
                scope_name(d.scope),
                w.title
            );
            (question, w.title.clone())
        }
    };
    let (sort, delegate) = match p {
        Proposal::Sort(s) => (Some(s.clone()), None),
        Proposal::Delegate(d) => (None, Some(d.clone())),
    };
    let mut item = InboxItem {
        id,
        kind: if sort.is_some() {
            ItemKind::Sort
        } else {
            ItemKind::Delegate
        },
        state: ItemState::Open,
        question: one_line(&question, MAX_QUESTION),
        workspace: facts.overall.clone(),
        title: one_line(&title, MAX_QUESTION),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
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
        sort,
        delegate,
        answers: Vec::new(),
    };
    item.answers = crate::items::answers(&item);
    Some(item)
}

fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default();
    one_line(line, MAX_QUESTION)
}

/// Blocking. The item when the host may send it now: still open, never
/// tried, the workspace still `auto`, and the day's
/// `max_auto_requests_per_day` not reached. Past it, the item keeps the
/// reason and waits for review.
pub fn may_send(config: &Config, a: &Auto, at: u64) -> Result<Option<Found>, ErrorBody> {
    let Some(found) = approve::find(config, &a.item)? else {
        return Ok(None);
    };
    if found.approval.is_some() || found.item.state != ItemState::Open {
        return Ok(None);
    }
    let workspaces = config.load_workspaces()?;
    if workspaces.overall_requests(&a.workspace) != OverallRequests::Auto {
        return Ok(None);
    }
    let limit = workspaces.assistant().max_auto_requests_per_day;
    let (day_start, reset) = policy::day(at);
    let sent = sent_since(config, day_start)?;
    if sent >= limit {
        let why = format!(
            "this host sent {sent} requests without review today, its [assistant] \
             max_auto_requests_per_day. The count resets at {reset}."
        );
        approve::release(config, &found, Some(&why), at);
        tracing::info!(item = %a.item, sent, limit = "max_auto_requests_per_day", "auto request held for review");
        return Ok(None);
    }
    Ok(Some(found))
}

/// Blocking. Requests the host sent without review since `day_start`, or is sending.
fn sent_since(config: &Config, day_start: u64) -> Result<u32, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(0);
    };
    let files = ItemStore::open(dir)
        .and_then(|s| s.all())
        .map_err(|e| state_error(&e))?;
    let n = files
        .iter()
        .flat_map(|t| approval::all(t).into_values())
        .filter(|a| a.auto_since(day_start))
        .count();
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}
