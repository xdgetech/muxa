//! `capture` and `work`: the user's input and Work items (ADR 0046), behind
//! the host's `work` level 8. Saving never starts an agent.

use std::time::Duration;

use muxa_protocol::{
    Capture, CaptureSource, Message, Request, SortAction, WORK_ASSISTANT, WorkAction, WorkItem,
    WorkState, work_level,
};
use serde_json::json;

use crate::conn::Conn;

const WAIT: Duration = Duration::from_secs(10);
/// Characters of a Capture's first line in a list.
const PREVIEW: usize = 60;

pub fn require(conn: &Conn) -> Result<(), String> {
    if work_level(conn.capabilities()) >= WORK_ASSISTANT {
        Ok(())
    } else {
        Err("this host keeps no Captures or Work items; update muxa-bridge on the host".into())
    }
}

/// What `capture` sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCapture {
    pub text: String,
    pub workspace: Option<String>,
    pub source: CaptureSource,
    /// A resend with the same id returns the stored Capture.
    pub client_id: String,
}

pub fn source(name: &str) -> Option<CaptureSource> {
    serde_json::from_value(json!(name)).ok()
}

/// A fresh client id: the time, the process and a random number.
pub fn client_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(nanos);
    format!("cli-{nanos:x}-{:x}-{:016x}", std::process::id(), h.finish())
}

pub async fn capture(conn: &mut Conn, new: NewCapture, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let messages = conn
        .call(
            |id| Request::Capture {
                id,
                client_id: new.client_id,
                text: new.text,
                source: Some(new.source),
                workspace: new.workspace,
                assist: None,
            },
            WAIT,
        )
        .await?;
    let (entry, item) = saved_capture(&messages)?;
    if as_json {
        println!("{}", json!({"capture": entry, "work": item}));
    } else {
        println!("{}", capture_line(&entry, crate::when::now()));
    }
    Ok(String::new())
}

/// The Work items, newest open first, and the unsorted Captures.
pub async fn list(conn: &mut Conn, all: bool, as_json: bool) -> Result<String, String> {
    let (work, captures) = fetch(conn).await?;
    let work: Vec<WorkItem> = work.into_iter().filter(|w| all || !closed(w)).collect();
    if as_json {
        println!("{}", json!({"work": work, "captures": captures}));
        return Ok(String::new());
    }
    let now = crate::when::now();
    for w in &work {
        println!("{}", work_line(w, now));
    }
    for c in &captures {
        println!("{}", capture_line(c, now));
    }
    if work.is_empty() && captures.is_empty() {
        println!("no Work items or unsorted Captures");
    }
    Ok(String::new())
}

/// `work sort CAPTURE …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sort {
    pub capture: String,
    pub action: SortAction,
    pub work: Option<String>,
    pub title: Option<String>,
    pub workspace: Option<String>,
}

pub async fn sort(conn: &mut Conn, s: Sort, as_json: bool) -> Result<String, String> {
    let (_, captures) = fetch(conn).await?;
    let rev = captures
        .iter()
        .find(|c| c.id == s.capture)
        .map(|c| c.rev)
        .ok_or_else(|| format!("no unsorted Capture {}", s.capture))?;
    let messages = conn
        .call(
            |id| Request::SortCapture {
                id,
                capture: s.capture,
                rev,
                action: s.action,
                work: s.work,
                title: s.title,
                workspace: s.workspace,
            },
            WAIT,
        )
        .await?;
    let (entry, item) = saved_capture(&messages)?;
    if as_json {
        println!("{}", json!({"capture": entry, "work": item}));
    } else if let Some(item) = &item {
        println!("{}", work_line(item, crate::when::now()));
    } else {
        println!("{} kept as a note", entry.id);
    }
    Ok(String::new())
}

/// `work (edit | later | done | withdraw | reopen | merge) WORK …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub work: String,
    pub action: WorkAction,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub into: Option<String>,
}

pub fn action(name: &str) -> Option<WorkAction> {
    match name {
        "done" => Some(WorkAction::Complete),
        "edit" | "later" | "withdraw" | "reopen" | "merge" => {
            serde_json::from_value(json!(name)).ok()
        }
        _ => None,
    }
}

pub async fn change(conn: &mut Conn, c: Change, as_json: bool) -> Result<String, String> {
    let rev = rev_of(conn, &c.work).await?;
    let messages = conn
        .call(
            |id| Request::UpdateWork {
                id,
                work: c.work,
                rev,
                action: c.action,
                title: c.title,
                workspace: c.workspace,
                into: c.into,
            },
            WAIT,
        )
        .await?;
    let entry = messages
        .into_iter()
        .find_map(|m| match m {
            Message::WorkSaved { entry, .. } => Some(entry),
            _ => None,
        })
        .ok_or("the bridge saved no Work item")?;
    if as_json {
        println!("{}", json!(entry));
    } else {
        println!("{}", work_line(&entry, crate::when::now()));
    }
    Ok(String::new())
}

/// The `rev` of Work item `work` now, for a change or a Task start.
pub async fn rev_of(conn: &mut Conn, work: &str) -> Result<u64, String> {
    let (items, _) = fetch(conn).await?;
    items
        .iter()
        .find(|w| w.id == work)
        .map(|w| w.rev)
        .ok_or_else(|| format!("no Work item {work}"))
}

async fn fetch(conn: &mut Conn) -> Result<(Vec<WorkItem>, Vec<Capture>), String> {
    require(conn)?;
    let messages = conn.call(|id| Request::ListInbox { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxList { work, captures, .. } => Some((work, captures)),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no inbox list".into())
}

fn saved_capture(messages: &[Message]) -> Result<(Capture, Option<WorkItem>), String> {
    messages
        .iter()
        .find_map(|m| match m {
            Message::CaptureSaved { entry, work, .. } => {
                Some((entry.clone(), work.as_deref().cloned()))
            }
            _ => None,
        })
        .ok_or_else(|| "the bridge saved no Capture".into())
}

fn closed(w: &WorkItem) -> bool {
    matches!(w.state, WorkState::Done | WorkState::Withdrawn)
}

pub fn work_line(w: &WorkItem, now: u64) -> String {
    let mut state = crate::items::wire(json!(w.state));
    if w.later_at.is_some() && !closed(w) {
        state.push_str(" later");
    }
    let workspace = w
        .workspace
        .as_deref()
        .map(|s| format!("  [{s}]"))
        .unwrap_or_default();
    format!(
        "{}  {state}  {}  {}{workspace}",
        w.id,
        crate::items::age(now.saturating_sub(w.created_at)),
        w.title
    )
}

/// Only the first line, cut short: a Capture can be long.
pub fn capture_line(c: &Capture, now: u64) -> String {
    let first: String = c
        .text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .chars()
        .take(PREVIEW)
        .collect();
    format!(
        "{}  unsorted  {}  {first}",
        c.id,
        crate::items::age(now.saturating_sub(c.created_at))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(state: WorkState, later: Option<u64>) -> WorkItem {
        WorkItem {
            id: "w-0000abcd".into(),
            rev: 3,
            title: "Shorten onboarding".into(),
            state,
            created_at: 100,
            updated_at: 100,
            workspace: Some("work".into()),
            later_at: later,
            merged_into: None,
            captures: Vec::new(),
            requests: Vec::new(),
            tasks: Vec::new(),
        }
    }

    #[test]
    fn a_work_line_shows_state_later_age_title_and_workspace() {
        assert_eq!(
            work_line(&item(WorkState::InProgress, Some(150)), 7300),
            "w-0000abcd  in-progress later  2h  Shorten onboarding  [work]"
        );
        assert_eq!(
            work_line(&item(WorkState::Done, Some(150)), 160),
            "w-0000abcd  done  1m  Shorten onboarding  [work]"
        );
    }

    #[test]
    fn a_capture_line_shows_only_the_first_line_cut_short() {
        let c = Capture {
            id: "c-0000abcd".into(),
            rev: 1,
            client_id: "x".into(),
            text: format!("\n{}\nsecret second line", "a".repeat(100)),
            source: CaptureSource::Cli,
            state: muxa_protocol::CaptureState::Received,
            created_at: 0,
            updated_at: 0,
            workspace: None,
            work: Vec::new(),
        };
        let line = capture_line(&c, 30);
        assert_eq!(
            line,
            format!("c-0000abcd  unsorted  30s  {}", "a".repeat(PREVIEW))
        );
    }

    #[test]
    fn names_map_to_the_wire_and_client_ids_differ() {
        assert_eq!(action("done"), Some(WorkAction::Complete));
        assert_eq!(action("later"), Some(WorkAction::Later));
        assert_eq!(action("complete"), None);
        assert_eq!(source("tmux"), Some(CaptureSource::Tmux));
        assert_eq!(source("phone"), None);
        let (a, b) = (client_id(), client_id());
        assert_ne!(a, b);
        assert!(a.len() <= 64 && a.bytes().all(|b| b.is_ascii_graphic()));
    }
}
