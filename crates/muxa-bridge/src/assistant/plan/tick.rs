//! The daemon's tick for plans (ADR 0047 §5), as for requests (ADR 0046
//! §4–§6): finds a run again after a crash, keeps each plan's delivery,
//! ends runs no plan waits for, closes published runs' sessions, and removes
//! published runs' folders once a newer plan published or after a week.
use std::path::PathBuf;

use muxa_protocol::{Delivery, Plan};
use muxa_tmux::Session;

use super::start::CREATING_FIELD;
use crate::assistant::cleanup::KEEP;
use crate::assistant::delivery::{self, Facts};
use crate::assistant::store::Shelf;
use crate::assistant::tick::{agent_of, closable, drop_run};
use crate::records::{Record, Status, Workspaces};
use crate::routines::store::StateError;
use crate::task_records::Config;

/// Seconds after which a claimed plan without a run did not start.
const START_LIMIT: u64 = 300;

/// What one scan found to do outside the store.
#[derive(Debug, Default)]
pub struct Todo {
    /// `(plan, run)` whose delivery needs the adapter.
    pub watch: Vec<(String, String)>,
    /// Runs whose session closes.
    pub close: Vec<String>,
    /// Published runs' folders to remove.
    pub dirs: Vec<PathBuf>,
}

/// Blocking. Every plan and every plan run once.
pub fn scan(config: &Config, workspaces: &Workspaces, now: u64) -> Todo {
    let mut todo = Todo::default();
    let Some(state_dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return todo;
    };
    let opened = Shelf::<Plan>::open(state_dir).and_then(|s| Ok((s.all()?, s)));
    let (plans, shelf) = match opened {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read plans");
            return todo;
        }
    };
    let records: Vec<Record> = config
        .records_if_any()
        .into_iter()
        .filter(|r| r.plan_run().is_some())
        .collect();
    if plans.is_empty() && records.is_empty() {
        return todo;
    }
    let timeout = super::overall_timeout(config);
    for p in plans.iter().filter(|p| !delivery::settled(p.delivery)) {
        one(&shelf, p, &records, timeout, now, &mut todo);
    }
    for r in &records {
        ended(config, &plans, r, &mut todo);
    }
    todo.dirs
        .extend(leftovers(workspaces, &plans, &records, now));
    todo
}

/// One unsettled plan: its run found again, or failed, or its delivery kept.
fn one(shelf: &Shelf<Plan>, p: &Plan, records: &[Record], timeout: u64, now: u64, todo: &mut Todo) {
    let Some(task) = &p.run else {
        unstarted(shelf, p, records, now);
        return;
    };
    let Some(record) = records.iter().find(|r| r.id == *task) else {
        return;
    };
    let facts = Facts {
        record: Some(record),
        agent: None,
        timeout,
        now,
    };
    let (state, at) = delivery::derive_plan(p, &facts);
    if matches!(state, Delivery::Received | Delivery::Processing) {
        todo.watch.push((p.id.clone(), task.clone()));
    }
    save(shelf, p, state, at);
}

/// The run of a plan that is answered, failed, replaced or gone: an open
/// record is dropped and a published session closes.
fn ended(config: &Config, plans: &[Plan], r: &Record, todo: &mut Todo) {
    let Some(run) = r.plan_run() else { return };
    let plan = plans.iter().find(|p| p.id == run.plan);
    let current = plan.is_some_and(|p| p.run.as_deref().is_none_or(|t| t == r.id));
    let state = plan.map(|p| p.delivery);
    if current && !state.is_some_and(delivery::settled) {
        return;
    }
    if crate::assistant::ask::open(r) {
        drop_run(config, &r.id, "No plan waits for this run.");
    }
    // A failed run keeps its session for inspection, as a request's does.
    let inspect = current && state == Some(Delivery::Failed);
    if closable(r) && !inspect {
        todo.close.push(r.id.clone());
    }
}

/// A claimed plan without a run: found again by its marker, or failed once
/// nobody can still make it.
fn unstarted(shelf: &Shelf<Plan>, p: &Plan, records: &[Record], now: u64) {
    let mark = crate::assistant::overall_brief::marker(&p.id);
    let found = records
        .iter()
        .find(|r| {
            r.plan_run().is_some_and(|run| run.plan == p.id)
                && r.goal.lines().next_back() == Some(mark.as_str())
        })
        .map(|r| r.id.clone());
    let changed = shelf.update_with_extra(&p.id, |p, extra| -> Result<_, StateError> {
        if p.run.is_some() || delivery::settled(p.delivery) {
            return Ok(None);
        }
        if let Some(task) = &found {
            p.run = Some(task.clone());
            extra.remove(CREATING_FIELD);
            p.rev += 1;
            return Ok(Some("overall run found again"));
        }
        let since = extra
            .get(CREATING_FIELD)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(p.delivery_at);
        if now.saturating_sub(since) <= START_LIMIT {
            return Ok(None);
        }
        extra.remove(CREATING_FIELD);
        extra.insert(super::REASON_FIELD.into(), "The run did not start.".into());
        p.delivery = Delivery::Failed;
        p.delivery_at = now;
        p.rev += 1;
        Ok(Some("overall run did not start"))
    });
    match changed {
        Ok(Some(Some(what))) => tracing::info!(plan = %p.id, "{what}"),
        Ok(_) => {}
        Err(e) => tracing::warn!(plan = %p.id, error = %e, "cannot settle an unstarted plan"),
    }
}

/// Writes a changed delivery, unless the plan moved on meanwhile.
fn save(shelf: &Shelf<Plan>, seen: &Plan, state: Delivery, at: u64) {
    if (state, at) == (seen.delivery, seen.delivery_at) {
        return;
    }
    let saved = shelf.update(&seen.id, |p| -> Result<_, StateError> {
        let same = p.delivery == seen.delivery && p.run == seen.run;
        if same {
            p.delivery = state;
            p.delivery_at = at;
        }
        Ok(same)
    });
    match saved {
        Ok(Some(true)) if state != seen.delivery => {
            tracing::info!(plan = %seen.id, delivery = ?state, "plan delivery")
        }
        Ok(_) => {}
        Err(e) => tracing::warn!(plan = %seen.id, error = %e, "cannot save a plan's delivery"),
    }
}

/// Folders of published plan runs: done, session closed, and either a newer
/// plan published or a week passed. A failed run keeps its folder.
fn leftovers(
    workspaces: &Workspaces,
    plans: &[Plan],
    records: &[Record],
    now: u64,
) -> Vec<PathBuf> {
    let newest = plans
        .iter()
        .filter(|p| p.delivery == Delivery::Resulted)
        .map(|p| p.inputs_at)
        .max();
    let mut out = Vec::new();
    for r in records {
        let Some(run) = r.plan_run() else { continue };
        if r.status != Status::Done || r.session.is_some() {
            continue;
        }
        let at = plans.iter().find(|p| p.id == run.plan).map(|p| p.inputs_at);
        let superseded = match (at, newest) {
            (Some(at), Some(newest)) => at < newest,
            (None, _) => true,
            _ => false,
        };
        if !superseded && now.saturating_sub(r.updated_at) < KEEP {
            continue;
        }
        let Some(dir) = workspaces
            .get(&r.workspace)
            .and_then(|ws| ws.assistant_dir(&r.title, &r.id).ok())
        else {
            continue;
        };
        if r.dir.as_deref() == Some(dir.as_path()) && dir.is_dir() {
            out.push(dir);
        }
    }
    out
}

/// Blocking. The adapter's word on each watched run, written as delivery.
pub fn watch(config: &Config, watched: &[(String, String)], sessions: &[Session]) {
    let Some(dir) = config.state_dir.as_ref() else {
        return;
    };
    let (Ok(shelf), Ok(store)) = (Shelf::<Plan>::open(dir), crate::records::Store::open(dir))
    else {
        return;
    };
    let timeout = super::overall_timeout(config);
    let now = crate::records::now();
    for (id, task) in watched {
        let (Ok(Some(p)), Ok(record)) = (shelf.get(id), store.get(task)) else {
            continue;
        };
        let facts = Facts {
            record: Some(&record),
            agent: agent_of(sessions, task),
            timeout,
            now,
        };
        let (state, at) = delivery::derive_plan(&p, &facts);
        save(&shelf, &p, state, at);
    }
}
