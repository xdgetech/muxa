//! Stored Inbox entries: listing, and approve, dismiss and later (ADR 0026
//! section 5). Approval creates one Task through `create-task`'s path; the
//! Task's goal carries the proposal unchanged with a marker line, so a
//! second approval, also after a crash, finds that Task instead of making one.
use std::sync::Arc;

use muxa_protocol::{Disposition, ErrorBody, ErrorCode, InboxAction, InboxEntry, InboxEntryKind};

use super::runs;
use super::store::{RoutineState, RoutineStore, StateError, Stored};
use crate::convert::error_body;
use crate::records::{Store, now};
use crate::task_records::{Config, Context, NewTask, create};

/// The most entries one `inbox-list` carries.
const MAX_LIST: usize = 500;
/// An approval whose Task creation began this long ago and never finished
/// may be tried again. Seconds.
const APPROVAL_LIMIT: u64 = 180;

fn bad(m: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, m.into())
}

fn state_error(e: &StateError) -> ErrorBody {
    match e {
        StateError::Lock(crate::records::StoreError::Busy(_)) => error_body(
            ErrorCode::Conflict,
            "the Inbox entry is being written; try again",
        ),
        e => {
            tracing::warn!(error = %e, "routine state failed");
            error_body(ErrorCode::Internal, e.to_string())
        }
    }
}

fn store(config: &Config) -> Result<RoutineStore, ErrorBody> {
    let dir = runs::dir(config).map_err(|e| error_body(ErrorCode::Unsupported, e.to_string()))?;
    RoutineStore::open(&dir).map_err(|e| state_error(&e))
}

/// The entry as clients see it. A `later` entry stored before ADR 0040 is
/// open and moved down.
pub fn shown(entry: &InboxEntry) -> InboxEntry {
    let mut e = entry.clone();
    if e.disposition == Disposition::Later {
        e.disposition = Disposition::Open;
        e.later_at.get_or_insert(e.created_at);
        e.until = None;
    }
    e
}

/// Blocking. Every stored entry, newest first.
pub fn list(config: &Config) -> Result<Vec<InboxEntry>, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(Vec::new());
    };
    let states = RoutineStore::open(dir)
        .and_then(|s| s.all())
        .map_err(|e| state_error(&e))?;
    let mut entries: Vec<InboxEntry> = states
        .iter()
        .flat_map(|s| s.entries.iter().map(|e| shown(&e.entry)))
        .collect();
    entries.sort_by(|a, b| (b.created_at, &b.id).cmp(&(a.created_at, &a.id)));
    entries.truncate(MAX_LIST);
    Ok(entries)
}

/// `update-inbox`.
pub async fn update(
    cx: &Context,
    entry: String,
    action: InboxAction,
) -> Result<InboxEntry, ErrorBody> {
    let config = Arc::clone(&cx.records);
    let found = blocking(move || {
        let store = store(&config)?;
        let state = owner(&store, &entry)?;
        let at = now();
        match action {
            InboxAction::Read => Ok(Step::Done(read(&store, &state, &entry, at)?)),
            InboxAction::Protect | InboxAction::Unprotect => {
                let on = action == InboxAction::Protect;
                let saved = protect(&store, &state, &entry, on, at)?;
                tracing::info!(entry, action = ?action, "inbox entry updated");
                Ok(Step::Done(saved))
            }
            InboxAction::Dismiss | InboxAction::Later => {
                let saved = close(&store, &state, &entry, action, at)?;
                tracing::info!(entry, action = ?action, "inbox entry updated");
                Ok(Step::Done(saved))
            }
            InboxAction::Approve => begin_approval(&config, &store, &state, &entry, at),
            // A proposal's Reject is its Dismiss; a briefing's Done is Dismiss already.
            InboxAction::Reject if is_proposal(&state, &entry) => {
                let saved = close(&store, &state, &entry, InboxAction::Dismiss, at)?;
                tracing::info!(entry, action = ?action, "inbox entry updated");
                Ok(Step::Done(saved))
            }
            other => Err(bad(format!(
                "{other:?} does not answer a Routine's entry; approve, reject, dismiss, later, read, protect or unprotect do"
            ))),
        }
    })
    .await?;
    match found {
        Step::Done(e) => Ok(e),
        Step::Create { entry, project } => approve(cx, entry, project).await,
    }
}

enum Step {
    Done(InboxEntry),
    /// The approval is claimed; create its Task.
    Create {
        entry: InboxEntry,
        project: Option<String>,
    },
}

fn is_proposal(state: &RoutineState, entry: &str) -> bool {
    state
        .entries
        .iter()
        .any(|e| e.entry.id == entry && e.entry.kind == InboxEntryKind::Proposal)
}

/// The Routine state that holds `entry`.
fn owner(store: &RoutineStore, entry: &str) -> Result<RoutineState, ErrorBody> {
    store
        .all()
        .map_err(|e| state_error(&e))?
        .into_iter()
        .find(|s| s.entries.iter().any(|e| e.entry.id == entry))
        .ok_or_else(|| bad(format!("no Inbox entry {entry}; list-inbox shows them")))
}

fn with_entry<T>(
    store: &RoutineStore,
    state: &RoutineState,
    entry: &str,
    change: impl FnOnce(&mut Stored) -> Result<T, ErrorBody>,
) -> Result<T, ErrorBody> {
    store
        .update::<T, Wrapped>(&state.workspace, &state.name, |s| {
            let stored = s
                .entries
                .iter_mut()
                .find(|e| e.entry.id == entry)
                .ok_or_else(|| Wrapped::Body(bad(format!("no Inbox entry {entry}"))))?;
            change(stored).map_err(Wrapped::Body)
        })
        .map_err(|e| match e {
            Wrapped::Body(b) => b,
            Wrapped::State(s) => state_error(&s),
        })
}

enum Wrapped {
    Body(ErrorBody),
    State(StateError),
}

impl From<StateError> for Wrapped {
    fn from(e: StateError) -> Self {
        Self::State(e)
    }
}

fn close(
    store: &RoutineStore,
    state: &RoutineState,
    entry: &str,
    action: InboxAction,
    at: u64,
) -> Result<InboxEntry, ErrorBody> {
    with_entry(store, state, entry, |s| {
        if shown(&s.entry).disposition != Disposition::Open || s.approving.is_some() {
            return Err(bad(format!("Inbox entry {entry} is already closed")));
        }
        s.entry.until = None;
        if action == InboxAction::Later {
            s.entry.disposition = Disposition::Open;
            s.entry.later_at = Some(at);
        } else {
            s.entry.disposition = Disposition::Dismissed;
        }
        Ok(shown(&s.entry))
    })
}

/// The first read stays; an entry in any state can be read.
fn read(
    store: &RoutineStore,
    state: &RoutineState,
    entry: &str,
    at: u64,
) -> Result<InboxEntry, ErrorBody> {
    with_entry(store, state, entry, |s| {
        s.entry.read_at.get_or_insert(at);
        Ok(shown(&s.entry))
    })
}

/// Sets or clears `protectedAt` on an entry in any state (ADR 0046).
fn protect(
    store: &RoutineStore,
    state: &RoutineState,
    entry: &str,
    on: bool,
    at: u64,
) -> Result<InboxEntry, ErrorBody> {
    with_entry(store, state, entry, |s| {
        if on {
            s.entry.protected_at.get_or_insert(at);
        } else {
            s.entry.protected_at = None;
        }
        Ok(shown(&s.entry))
    })
}

/// Blocking. Deletes every Routine's unprotected entries made before
/// `cutoff` (epoch seconds). Returns how many.
pub fn delete_before(config: &Config, cutoff: u64) -> Result<u32, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(0);
    };
    let store = RoutineStore::open(dir).map_err(|e| state_error(&e))?;
    let mut deleted = 0;
    for state in store.all().map_err(|e| state_error(&e))? {
        if !state.entries.iter().any(|s| s.entry.created_at < cutoff) {
            continue;
        }
        deleted += store
            .update::<_, StateError>(&state.workspace, &state.name, |s| {
                Ok(s.delete_before(cutoff))
            })
            .map_err(|e| state_error(&e))?;
    }
    Ok(deleted)
}

/// The marker line in an approved Task's goal.
fn marker(entry: &InboxEntry) -> Option<String> {
    let hash = &entry.proposal.as_ref()?.hash;
    Some(format!("muxa-proposal: {} {hash}", entry.id))
}

fn begin_approval(
    config: &Config,
    store: &RoutineStore,
    state: &RoutineState,
    entry: &str,
    at: u64,
) -> Result<Step, ErrorBody> {
    let project = runs::load(config)
        .ok()
        .and_then(|w| w.routine(&state.workspace, &state.name).cloned())
        .and_then(|r| r.project);
    // A Task from an earlier approval that did not finish is found, not made again.
    let existing = |e: &InboxEntry| -> Option<String> {
        let mark = marker(e)?;
        let dir = config.state_dir.as_ref()?;
        Store::open(dir)
            .and_then(|s| s.list())
            .ok()?
            .into_iter()
            // The last line only: approved content cannot carry another entry's marker.
            .find(|r| r.goal.lines().next_back() == Some(mark.as_str()))
            .map(|r| r.id)
    };
    with_entry(store, state, entry, |s| {
        if s.entry.kind != InboxEntryKind::Proposal || s.entry.proposal.is_none() {
            return Err(bad(format!(
                "Inbox entry {entry} is a briefing; only a proposal is approved"
            )));
        }
        match s.entry.disposition {
            Disposition::Approved if s.entry.task.is_some() => {
                return Ok(Step::Done(s.entry.clone()));
            }
            Disposition::Dismissed => {
                return Err(bad(format!("Inbox entry {entry} was dismissed")));
            }
            _ => {}
        }
        if let Some(task) = existing(&s.entry) {
            s.entry.disposition = Disposition::Approved;
            s.entry.until = None;
            s.entry.task = Some(task);
            s.approving = None;
            return Ok(Step::Done(s.entry.clone()));
        }
        if s.approving
            .is_some_and(|since| at.saturating_sub(since) < APPROVAL_LIMIT)
        {
            return Err(error_body(
                ErrorCode::Conflict,
                format!("Inbox entry {entry} is being approved; wait for its Task"),
            ));
        }
        s.approving = Some(at);
        Ok(Step::Create {
            entry: s.entry.clone(),
            project: project.clone(),
        })
    })
}

/// The approved Task's goal: the proposal unchanged, its hash, and the marker.
pub fn task_goal(entry: &InboxEntry) -> Option<String> {
    let p = entry.proposal.as_ref()?;
    Some(format!(
        "Perform this approved action. Do exactly this and nothing else.\n\n\
         Action: {action}\nTarget: {target}\n\n\
         Content, approved unchanged:\n\n{content}\n\n\
         Approved in Inbox from the Routine \"{routine}\" in workspace \"{workspace}\".\n\
         Content hash: {hash}\n{marker}\n",
        action = p.action,
        target = p.target,
        content = p.content,
        routine = entry.routine,
        workspace = entry.workspace,
        hash = p.hash,
        marker = marker(entry)?,
    ))
}

async fn approve(
    cx: &Context,
    entry: InboxEntry,
    project: Option<String>,
) -> Result<InboxEntry, ErrorBody> {
    let goal = task_goal(&entry).ok_or_else(|| bad("the entry has no proposal"))?;
    let new = NewTask {
        workspace: entry.workspace.clone(),
        title: entry.title.clone(),
        goal,
        project,
        ..NewTask::default()
    };
    let (task, error) = match cx.env().await {
        Ok(env) => {
            let (record, error) = create(&env, new).await;
            (record.map(|r| r.id), error)
        }
        Err(e) => (None, Some(e)),
    };
    let (config, id) = (Arc::clone(&cx.records), entry.id.clone());
    let saved = blocking(move || {
        let store = store(&config)?;
        let state = owner(&store, &id)?;
        with_entry(&store, &state, &id, |s| {
            s.approving = None;
            // A Task that exists, also one whose agent did not start, is the approval's Task.
            if let Some(task) = &task {
                s.entry.disposition = Disposition::Approved;
                s.entry.until = None;
                s.entry.task = Some(task.clone());
            }
            Ok(s.entry.clone())
        })
    })
    .await?;
    tracing::info!(entry = %saved.id, task = ?saved.task, "inbox proposal approved");
    match (saved.task.is_some(), error) {
        (false, Some(e)) => Err(e),
        _ => Ok(saved),
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ErrorBody> + Send + 'static,
) -> Result<T, ErrorBody> {
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|_| Err(error_body(ErrorCode::Internal, "the Inbox store stopped")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::Proposal;

    fn entry(disposition: Disposition, until: Option<u64>) -> InboxEntry {
        InboxEntry {
            id: "e-00000001".into(),
            workspace: "w".into(),
            routine: "Brief".into(),
            key: "k".into(),
            kind: InboxEntryKind::Proposal,
            title: "Close ENG-12".into(),
            body: String::new(),
            links: Vec::new(),
            created_at: 1,
            run: "t-000001".into(),
            occurrence: "now-1".into(),
            disposition,
            read_at: None,
            later_at: None,
            until,
            protected_at: None,
            proposal: Some(Proposal {
                action: "Close".into(),
                target: "linear:ENG-12".into(),
                content: "Closing:\nstale.".into(),
                hash: super::super::output::proposal_hash(
                    "Close",
                    "linear:ENG-12",
                    "Closing:\nstale.",
                ),
            }),
            task: None,
        }
    }

    #[test]
    fn a_later_entry_from_before_adr_0040_is_open_and_moved_down() {
        let open = shown(&entry(Disposition::Later, Some(4_000_000_000)));
        assert_eq!(
            (open.disposition, open.later_at, open.until),
            (Disposition::Open, Some(1), None)
        );
        assert_eq!(
            shown(&entry(Disposition::Dismissed, None)).disposition,
            Disposition::Dismissed
        );
    }

    #[test]
    fn the_task_goal_holds_the_content_unchanged_with_its_hash() {
        let e = entry(Disposition::Open, None);
        let goal = task_goal(&e).unwrap();
        assert!(goal.contains("\n\nClosing:\nstale.\n\n"));
        let hash = &e.proposal.as_ref().unwrap().hash;
        assert!(goal.contains(&format!("Content hash: {hash}\n")));
        assert_eq!(
            goal.lines().next_back(),
            Some(format!("muxa-proposal: e-00000001 {hash}").as_str())
        );
        assert!(goal.len() < crate::records::MAX_GOAL);
    }
}
