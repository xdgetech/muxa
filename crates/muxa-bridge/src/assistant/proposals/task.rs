//! Start Task (ADR 0060 §2, §4): `approve` or `start-task` on a `result`
//! item that offers a Task, or `approve` on a `new-task` item, starts one
//! Task through `create-task` on the workspace's default Task worker, in
//! plan mode. The goal's last line marks the proposal, so a second approve,
//! also of another item offering the same proposal, returns the same Task.
//! Logs carry ids and the action, never the title or the goal.
use std::sync::Arc;

use muxa_protocol::{
    ErrorBody, ErrorCode, InboxAction, InboxItem, ItemKind, ItemState, TaskProposal, WorkItem,
};

use super::approval::{self, Approval};
use super::approve::{APPROVAL_LIMIT, Approved, Found, Made, bad, stored, with_items};
use crate::assistant::{NO_DAEMON, blocking, shelf, state_error};
use crate::convert::error_body;
use crate::items::{answers, shown};
use crate::records::{Record, now};
use crate::task_records::{Config, Context, NewTask, create};

/// The proposal an item offers to start now, if any.
pub fn offered(item: &InboxItem) -> Option<&TaskProposal> {
    if item.task.is_some() {
        return None;
    }
    match item.kind {
        ItemKind::Result => item.result.as_ref()?.task.as_ref(),
        ItemKind::NewTask => item.new_task.as_ref(),
        _ => None,
    }
}

/// `approve` starts the Task: a `new-task` item, or a `result` item that
/// offers a Task and no hand-off.
pub fn approve_starts(item: &InboxItem) -> bool {
    offered(item).is_some() && item.result.as_ref().is_none_or(|r| r.handoff.is_none())
}

/// What approve creates.
struct Plan {
    proposal: TaskProposal,
    work: Option<String>,
    goal: String,
}

enum Begun {
    /// An earlier approve made it.
    Made(Box<Record>),
    Create(Plan),
}

pub(super) async fn start(
    cx: &Context,
    f: Found,
    answer: InboxAction,
) -> Result<Approved, ErrorBody> {
    if offered(&f.item).is_none() {
        return Err(bad("the item offers no Task to start"));
    }
    if !cx.daemon {
        return Err(error_body(ErrorCode::Unsupported, NO_DAEMON));
    }
    let (config, found, at) = (Arc::clone(&cx.records), f.clone(), now());
    let plan = match blocking(move || begin(&config, &found, at)).await? {
        Begun::Made(record) => return done(cx, f, *record, answer, None).await,
        Begun::Create(plan) => plan,
    };
    let new = NewTask {
        workspace: plan.proposal.workspace.clone(),
        worker: None,
        title: plan.proposal.title.clone(),
        goal: plan.goal,
        repo: plan.proposal.repo.clone(),
        work: plan.work,
        plan: true,
        ..NewTask::default()
    };
    let (record, error) = match cx.env().await {
        Ok(env) => create(&env, new).await,
        Err(e) => (None, Some(e)),
    };
    // A Task that exists, also one whose agent did not start, is the approval's Task.
    if let Some(record) = record {
        tracing::info!(item = %f.item.id, task = %record.id, answer = ?answer, "proposed task started");
        return done(cx, f, record, answer, error).await;
    }
    let config = Arc::clone(&cx.records);
    let released = blocking(move || {
        release(&config, &f);
        Ok(())
    })
    .await;
    if let Err(r) = released {
        tracing::warn!(error = %r.message, "cannot release a task proposal's approval");
    }
    Err(error.unwrap_or_else(|| error_body(ErrorCode::Internal, "the Task was not made")))
}

/// The goal's last line: one per proposal. A Result's proposal is its
/// request version's step, so every item offering it finds the same Task.
pub fn marker(item: &InboxItem, hash: &str) -> String {
    match (&item.result, offered(item).and_then(|t| t.step)) {
        (Some(r), Some(step)) => {
            format!("muxa-task-proposal: {} v{} s{step}", r.request, r.version)
        }
        _ => format!("muxa-task-proposal: {} {hash}", item.id),
    }
}

/// Pure. The Task's goal: the proposal's, where it came from, the plan
/// first, then the marker.
pub fn goal(p: &TaskProposal, who: &str, work: Option<(&str, &str)>, mark: &str) -> String {
    let about = work
        .map(|(id, title)| format!(" for Work item \"{title}\" ({id})"))
        .unwrap_or_default();
    format!(
        "{goal}\n\n\
         {who} proposed this Task{about}, and the user approved it in the Inbox.\n\
         Plan first: read what you need, then present your plan for the user's approval \
         before you change any file.\n\
         {mark}",
        goal = p.goal.trim_end(),
    )
}

/// Blocking. The Task an earlier approve made, or the claim and the plan.
fn begin(config: &Config, f: &Found, at: u64) -> Result<Begun, ErrorBody> {
    let p = offered(&f.item)
        .cloned()
        .ok_or_else(|| bad("the item offers no Task to start"))?;
    let mark = marker(&f.item, &f.hash);
    if let Some(record) = existing(config, &mark) {
        return Ok(Begun::Made(Box::new(record)));
    }
    if let Some(task) = claim(config, f, at)? {
        let record = config.records_if_any().into_iter().find(|r| r.id == task);
        return record.map(|r| Begun::Made(Box::new(r))).ok_or_else(|| {
            bad(format!(
                "the proposal's Task {task} is no longer on this host"
            ))
        });
    }
    let work = p
        .work
        .clone()
        .or_else(|| f.item.result.as_ref().map(|r| r.work.clone()));
    let title = match &work {
        Some(w) => shelf::<WorkItem>(config)?
            .get(w)
            .map_err(|e| state_error(&e))?
            .map(|w| crate::items::produce::one_line(&w.title, 200)),
        None => None,
    };
    let who = match f.item.kind {
        ItemKind::NewTask => "The secretary".to_owned(),
        _ => p
            .worker
            .clone()
            .unwrap_or_else(|| "The workspace's assistant".into()),
    };
    let about = work.as_deref().zip(title.as_deref());
    Ok(Begun::Create(Plan {
        goal: goal(&p, &who, about, &mark),
        // A Work item gone from the host links nothing.
        work: work.filter(|_| title.is_some()),
        proposal: p,
    }))
}

/// The Task whose goal ends with `mark`. The last line only: a proposed
/// goal cannot carry another proposal's marker.
fn existing(config: &Config, mark: &str) -> Option<Record> {
    config
        .records_if_any()
        .into_iter()
        .find(|r| r.goal.lines().next_back() == Some(mark))
}

/// Blocking. Claims the approval. The Task an earlier approve recorded, if any.
fn claim(config: &Config, f: &Found, at: u64) -> Result<Option<String>, ErrorBody> {
    with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        if let Some(task) = &a.task {
            return Ok(Some(task.clone()));
        }
        let item = stored(items, &f.item.id)?;
        if !matches!(item.state, ItemState::Open | ItemState::Later) {
            return Err(bad(format!("Inbox item {} is already closed", item.id)).into());
        }
        if a.since
            .is_some_and(|s| at.saturating_sub(s) < APPROVAL_LIMIT)
        {
            return Err(error_body(
                ErrorCode::Conflict,
                format!(
                    "Inbox item {} is being approved; wait for its Task",
                    f.item.id
                ),
            )
            .into());
        }
        a.since = Some(at);
        approval::put(items, &f.item.id, a);
        Ok(None)
    })
}

async fn done(
    cx: &Context,
    f: Found,
    record: Record,
    answer: InboxAction,
    error: Option<ErrorBody>,
) -> Result<Approved, ErrorBody> {
    let (config, task, title) = (
        Arc::clone(&cx.records),
        record.id.clone(),
        record.title.clone(),
    );
    let work = record.work().map(str::to_owned);
    let item = blocking(move || {
        if let Some(work) = &work {
            crate::assistant::start::link_task(&config, work, &task);
        }
        let item = finish(&config, &f, &task, &title, answer)?;
        // The item closed with its hand-off unstarted: the chain ends here,
        // as Done ends it (ADR 0058 §2).
        if let Some(r) = f.item.result.as_ref().filter(|r| r.handoff.is_some()) {
            crate::assistant::chain::done(&config, &r.request, r.version)?;
        }
        Ok(item)
    })
    .await?;
    Ok(Approved {
        made: Some(Made::Task(Box::new(record))),
        item,
        error,
    })
}

/// Blocking. The item is answered and names the Task it started.
fn finish(
    config: &Config,
    f: &Found,
    task: &str,
    title: &str,
    answer: InboxAction,
) -> Result<InboxItem, ErrorBody> {
    let at = now();
    with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.since = None;
        a.task = Some(task.to_owned());
        approval::put(items, &f.item.id, a);
        let item = stored(items, &f.item.id)?;
        if item.task.is_none() {
            let started = format!("Started Task {task}: {title}");
            item.summary = Some(match item.summary.take() {
                Some(s) => format!("{started}\n\n{s}"),
                None => started,
            });
            item.state = ItemState::Answered;
            item.answer = Some(answer);
            item.answered_at = Some(at);
            item.until = None;
            item.task = Some(task.to_owned());
            item.answers = answers(item);
        }
        Ok(shown(item))
    })
}

/// Blocking. Ends a claim that made nothing; the item stays open.
fn release(config: &Config, f: &Found) {
    let released = with_items(config, &f.task, |items| {
        let mut a =
            approval::get(items, &f.item.id, &f.hash).unwrap_or_else(|| Approval::new(&f.hash));
        a.since = None;
        approval::put(items, &f.item.id, a);
        Ok(())
    });
    if let Err(e) = released {
        tracing::warn!(item = %f.item.id, error = %e.message, "cannot release a task proposal's approval");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proposal() -> TaskProposal {
        TaskProposal {
            workspace: "muxa".into(),
            repo: Some("muxa".into()),
            title: "Fix the crash".into(),
            goal: "Guard the empty summary.\n".into(),
            work: None,
            step: Some(2),
            worker: Some("muxa-research".into()),
        }
    }

    #[test]
    fn the_goal_keeps_the_proposal_asks_for_a_plan_and_ends_with_the_marker() {
        let g = goal(
            &proposal(),
            "muxa-research",
            Some(("w-0000000a", "Fix the Inbox crash")),
            "muxa-task-proposal: q-0000000b v1 s2",
        );
        assert!(g.starts_with("Guard the empty summary.\n\nmuxa-research proposed this Task for Work item \"Fix the Inbox crash\" (w-0000000a)"));
        assert!(g.contains("Plan first"));
        assert_eq!(
            g.lines().next_back(),
            Some("muxa-task-proposal: q-0000000b v1 s2")
        );
        let long = TaskProposal {
            goal: "g".repeat(crate::assistant::task_proposal::MAX_GOAL),
            ..proposal()
        };
        let mark = format!("muxa-task-proposal: i-0000000c sha256:{}", "0".repeat(64));
        let title = "t".repeat(200);
        assert!(
            goal(&long, &"w".repeat(64), Some(("w-0000000a", &title)), &mark).len()
                < crate::records::MAX_GOAL
        );
    }

    #[test]
    fn a_result_marks_its_version_and_step_and_a_new_task_item_its_hash() {
        let mut item = crate::items::store::tests_item("i-0000000c", ItemState::Open);
        item.task = None;
        item.kind = ItemKind::Result;
        item.result = Some(muxa_protocol::ResultReview {
            request: "q-0000000b".into(),
            version: 3,
            step: 3,
            work: "w-0000000a".into(),
            worker: None,
            findings: Vec::new(),
            open_questions: Vec::new(),
            next: Vec::new(),
            files: Vec::new(),
            handoff: None,
            task: Some(proposal()),
        });
        assert_eq!(marker(&item, "h"), "muxa-task-proposal: q-0000000b v3 s2");
        assert!(approve_starts(&item));
        if let Some(r) = item.result.as_mut() {
            r.handoff = Some(muxa_protocol::Handoff {
                worker: "b".into(),
                purpose: "p".into(),
            });
        }
        assert!(
            !approve_starts(&item),
            "approve hands off; start-task starts"
        );
        assert!(offered(&item).is_some());
        item.task = Some("t-000001".into());
        assert!(offered(&item).is_none(), "started already");

        let mut new = crate::items::store::tests_item("i-0000000d", ItemState::Open);
        new.task = None;
        new.kind = ItemKind::NewTask;
        new.new_task = Some(TaskProposal {
            step: None,
            ..proposal()
        });
        assert_eq!(
            marker(&new, "sha256:ab"),
            "muxa-task-proposal: i-0000000d sha256:ab"
        );
        assert!(approve_starts(&new));
    }
}
