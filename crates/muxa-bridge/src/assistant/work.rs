//! `sort-capture` and `update-work`: the user's changes to Captures and
//! Work items. Each checks the `rev` the client saw, under the record's lock.
use muxa_protocol::{
    Capture, CaptureState, Due, ErrorBody, ErrorCode, SortAction, WorkAction, WorkItem, WorkState,
};

use super::store::Shelf;
use crate::convert::error_body;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// Characters.
pub const MAX_TITLE: usize = 200;
/// Bytes of an IANA time zone name, as a Routine's `timezone` allows.
const MAX_ZONE: usize = 64;

/// A failed change: the store's error, or one for the client.
pub(super) enum Fail {
    State(StateError),
    Client(ErrorBody),
}

impl From<StateError> for Fail {
    fn from(e: StateError) -> Self {
        Fail::State(e)
    }
}

impl From<ErrorBody> for Fail {
    fn from(e: ErrorBody) -> Self {
        Fail::Client(e)
    }
}

impl From<Fail> for ErrorBody {
    fn from(f: Fail) -> Self {
        match f {
            Fail::State(e) => super::state_error(&e),
            Fail::Client(e) => e,
        }
    }
}

pub struct Sort {
    pub capture: String,
    pub rev: u64,
    pub action: SortAction,
    pub work: Option<String>,
    pub title: Option<String>,
    pub workspace: Option<String>,
}

/// Blocking. The sorted Capture, and the Work item it went to.
pub fn sort(config: &Config, s: Sort) -> Result<(Capture, Option<WorkItem>), ErrorBody> {
    let title = check_sort(config, &s)?;
    let captures = super::shelf::<Capture>(config)?;
    let works = super::shelf::<WorkItem>(config)?;
    let at = crate::records::now();
    let sorted = captures.update(&s.capture, |c| -> Result<_, Fail> {
        check_rev("Capture", &c.id, c.rev, s.rev)?;
        if c.state != CaptureState::Received {
            return Err(bad(format!("Capture {} is already sorted", c.id)).into());
        }
        let item = match s.action {
            SortAction::Work => Some(works.create(|id| {
                Ok::<_, Fail>(WorkItem {
                    id,
                    rev: 1,
                    title: title.clone().unwrap_or_else(|| title_from(&c.text)),
                    state: WorkState::Open,
                    created_at: at,
                    updated_at: at,
                    workspace: s.workspace.clone().or_else(|| c.workspace.clone()),
                    later_at: None,
                    merged_into: None,
                    pinned_at: None,
                    due: None,
                    captures: vec![c.id.clone()],
                    requests: Vec::new(),
                    tasks: Vec::new(),
                })
            })?),
            SortAction::Link => {
                let work = s.work.as_deref().unwrap_or_default();
                Some(link_capture(&works, work, &c.id, at)?)
            }
            SortAction::Note => None,
        };
        if let Some(item) = &item {
            c.work.push(item.id.clone());
        }
        c.state = CaptureState::Sorted;
        c.rev += 1;
        c.updated_at = at;
        Ok((c.clone(), item))
    })?;
    let (capture, item) = sorted.ok_or_else(|| bad(format!("no Capture {}", s.capture)))?;
    tracing::info!(capture = %capture.id, action = ?s.action, work = ?item.as_ref().map(|w| &w.id), "capture sorted");
    Ok((capture, item))
}

/// The fields each action takes, and the new item's title.
fn check_sort(config: &Config, s: &Sort) -> Result<Option<String>, ErrorBody> {
    match s.action {
        SortAction::Work if s.work.is_some() => Err(bad("sort-capture: work takes no work id")),
        SortAction::Work => {
            if let Some(name) = &s.workspace {
                check_workspace(config, name)?;
            }
            s.title.as_deref().map(check_title).transpose()
        }
        SortAction::Link if s.work.is_none() => Err(bad("sort-capture: link needs work")),
        SortAction::Link | SortAction::Note if s.title.is_some() || s.workspace.is_some() => {
            Err(bad("sort-capture: only work takes a title or a workspace"))
        }
        SortAction::Note if s.work.is_some() => Err(bad("sort-capture: note takes no work id")),
        SortAction::Link | SortAction::Note => Ok(None),
    }
}

fn link_capture(
    works: &Shelf<WorkItem>,
    work: &str,
    capture: &str,
    at: u64,
) -> Result<WorkItem, Fail> {
    works
        .update(work, |w| -> Result<_, Fail> {
            if !w.captures.iter().any(|c| c == capture) {
                w.captures.push(capture.to_owned());
                w.rev += 1;
                w.updated_at = at;
            }
            Ok(w.clone())
        })?
        .ok_or_else(|| bad(format!("no Work item {work}")).into())
}

pub struct Update {
    pub work: String,
    pub rev: u64,
    pub action: WorkAction,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub into: Option<String>,
    pub due: Option<Due>,
}

/// Blocking. The changed Work item. A `pin` of a pinned item or an `unpin`
/// of an unpinned one returns it unchanged, with its `rev`.
pub fn update(config: &Config, u: Update) -> Result<WorkItem, ErrorBody> {
    let title = check_update(config, &u)?;
    let works = super::shelf::<WorkItem>(config)?;
    let at = crate::records::now();
    let changed = works.update(&u.work, |w| -> Result<_, Fail> {
        check_rev("Work item", &w.id, w.rev, u.rev)?;
        let open = !closed(w.state);
        match u.action {
            WorkAction::Edit => {
                if let Some(title) = &title {
                    w.title = title.clone();
                }
                if let Some(name) = &u.workspace {
                    w.workspace = Some(name.clone());
                }
                if let Some(due) = &u.due {
                    w.due = Some(due.clone());
                }
            }
            WorkAction::Later if !open => return Err(is_closed(w).into()),
            // Moving an item down ends its pin; pinning it moves it up again.
            WorkAction::Later => {
                w.later_at = Some(at);
                w.pinned_at = None;
            }
            WorkAction::Complete => w.state = WorkState::Done,
            WorkAction::Withdraw => w.state = WorkState::Withdrawn,
            WorkAction::Reopen if open => {
                return Err(bad(format!("Work item {} is open", w.id)).into());
            }
            WorkAction::Reopen => {
                w.state = WorkState::Open;
                w.later_at = None;
                w.merged_into = None;
            }
            WorkAction::Pin if !open => return Err(is_closed(w).into()),
            // Already pinned, or not pinned: nothing changes, not even `rev`.
            WorkAction::Pin if w.pinned_at.is_some() => return Ok(w.clone()),
            WorkAction::Pin => {
                w.pinned_at = Some(at);
                w.later_at = None;
            }
            WorkAction::Unpin if w.pinned_at.is_none() => return Ok(w.clone()),
            WorkAction::Unpin => w.pinned_at = None,
            WorkAction::Merge if !open => return Err(is_closed(w).into()),
            WorkAction::Merge => {
                let into = u.into.as_deref().unwrap_or_default();
                merge_into(&works, w, into, at)?;
                w.state = WorkState::Withdrawn;
                w.merged_into = Some(into.to_owned());
            }
        }
        if closed(w.state) {
            w.pinned_at = None;
        } else {
            // A derived state is never stored.
            w.state = WorkState::Open;
        }
        w.rev += 1;
        w.updated_at = at;
        Ok(w.clone())
    })?;
    let item = changed.ok_or_else(|| bad(format!("no Work item {}", u.work)))?;
    tracing::info!(work = %item.id, action = ?u.action, rev = item.rev, "work item saved");
    if let Some(into) = &item.merged_into {
        if u.action == WorkAction::Merge {
            add_work_to_captures(config, &item.captures, into, at);
        }
    }
    Ok(item)
}

/// The fields each action takes, and the edited title.
fn check_update(config: &Config, u: &Update) -> Result<Option<String>, ErrorBody> {
    let edits = u.title.is_some() || u.workspace.is_some() || u.due.is_some();
    match u.action {
        WorkAction::Edit if u.into.is_some() => Err(bad("update-work: edit takes no into")),
        WorkAction::Edit if !edits => {
            Err(bad("update-work: edit needs a title, a workspace or a due"))
        }
        WorkAction::Edit => {
            if let Some(name) = &u.workspace {
                check_workspace(config, name)?;
            }
            if let Some(due) = &u.due {
                check_due(due)?;
            }
            u.title.as_deref().map(check_title).transpose()
        }
        _ if edits => Err(bad(
            "update-work: only edit takes a title, a workspace or a due",
        )),
        WorkAction::Merge => match &u.into {
            None => Err(bad("update-work: merge needs into")),
            Some(into) if *into == u.work => {
                Err(bad("update-work: a Work item cannot merge into itself"))
            }
            Some(_) => Ok(None),
        },
        _ if u.into.is_some() => Err(bad("update-work: only merge takes into")),
        _ => Ok(None),
    }
}

/// Adds `from`'s Captures and requests to the open item `into`.
fn merge_into(works: &Shelf<WorkItem>, from: &WorkItem, into: &str, at: u64) -> Result<(), Fail> {
    works
        .update(into, |t| -> Result<(), Fail> {
            if closed(t.state) {
                return Err(is_closed(t).into());
            }
            for c in &from.captures {
                if !t.captures.contains(c) {
                    t.captures.push(c.clone());
                }
            }
            for r in &from.requests {
                if !t.requests.contains(r) {
                    t.requests.push(r.clone());
                }
            }
            t.rev += 1;
            t.updated_at = at;
            Ok(())
        })?
        .ok_or_else(|| bad(format!("no Work item {into}")).into())
}

/// After a merge, each Capture also lists the item it went to. A Capture
/// that cannot be written keeps its old link; the item still lists it.
fn add_work_to_captures(config: &Config, captures: &[String], into: &str, at: u64) {
    let Ok(shelf) = super::shelf::<Capture>(config) else {
        return;
    };
    for id in captures {
        let added = shelf.update::<_, StateError>(id, |c| {
            if !c.work.iter().any(|w| w == into) {
                c.work.push(into.to_owned());
                c.rev += 1;
                c.updated_at = at;
            }
            Ok(())
        });
        if let Err(e) = added {
            tracing::warn!(capture = %id, error = %e, "cannot link a merged capture");
        }
    }
}

pub fn closed(state: WorkState) -> bool {
    matches!(state, WorkState::Done | WorkState::Withdrawn)
}

fn is_closed(w: &WorkItem) -> ErrorBody {
    bad(format!(
        "Work item {} is done or withdrawn; reopen it first",
        w.id
    ))
}

pub(super) fn check_rev(what: &str, id: &str, stored: u64, seen: u64) -> Result<(), ErrorBody> {
    if stored == seen {
        return Ok(());
    }
    Err(error_body(
        ErrorCode::Conflict,
        format!("{what} {id} changed (rev {stored}, not {seen}); list it again"),
    ))
}

/// One line, 1 to [`MAX_TITLE`] characters. The error never quotes it.
fn check_title(title: &str) -> Result<String, ErrorBody> {
    let t = title.trim();
    if t.is_empty() || t.contains(['\n', '\r', '\0']) || t.chars().count() > MAX_TITLE {
        return Err(bad(format!(
            "a Work item title is one line of 1 to {MAX_TITLE} characters"
        )));
    }
    Ok(t.to_owned())
}

/// `at` a time jiff can hold, after 1970; `timezone` a known IANA name.
/// The error never quotes either.
fn check_due(due: &Due) -> Result<(), ErrorBody> {
    let zone = due.timezone.as_str();
    let shaped = !zone.is_empty() && zone.len() <= MAX_ZONE && !zone.chars().any(char::is_control);
    let at = i64::try_from(due.at)
        .ok()
        .filter(|s| *s > 0)
        .and_then(|s| jiff::Timestamp::from_second(s).ok());
    if at.is_none() || !shaped || jiff::tz::TimeZone::get(zone).is_err() {
        return Err(bad(
            "update-work: due needs at (epoch seconds above 0) and timezone, a known IANA \
             name such as Asia/Tokyo",
        ));
    }
    Ok(())
}

/// The text's first non-blank line, cut to [`MAX_TITLE`] characters.
fn title_from(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    line.chars()
        .filter(|c| *c != '\0')
        .take(MAX_TITLE)
        .collect()
}

fn check_workspace(config: &Config, name: &str) -> Result<(), ErrorBody> {
    if config.load_workspaces()?.get(name).is_none() {
        return Err(bad(format!("no workspace {name}")));
    }
    Ok(())
}

fn bad(message: impl Into<String>) -> ErrorBody {
    error_body(ErrorCode::BadRequest, message.into())
}

#[cfg(test)]
mod tests;
