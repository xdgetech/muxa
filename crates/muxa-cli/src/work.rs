//! `capture` and `work`: the user's input and Work items (ADR 0046), behind
//! the host's `work` level 8. Saving starts an agent only with `--assist`.

use std::time::Duration;

use muxa_protocol::{
    AssistSpec, AssistantRequest, Capture, CaptureSource, Correction, Due, Hypothesis, InboxItem,
    Message, Plan, Policy, Request, SortAction, WORK_ASSISTANT, WorkAction, WorkItem, WorkState,
    work_level,
};
use serde_json::json;

use crate::conn::Conn;

pub const WAIT: Duration = Duration::from_secs(10);
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
    /// Also make a Work item in `workspace` and ask its assistant.
    pub assist: Option<AssistSpec>,
}

pub fn source(name: &str) -> Option<CaptureSource> {
    // A name this build does not know decodes as `Unknown`; the host refuses it.
    serde_json::from_value(json!(name))
        .ok()
        .filter(muxa_protocol::Known::is_known)
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
                assist: new.assist,
            },
            WAIT,
        )
        .await?;
    let (entry, item) = saved_capture(&messages)?;
    let request = messages.iter().find_map(|m| match m {
        Message::CaptureSaved { request, .. } => request.as_deref().cloned(),
        _ => None,
    });
    if as_json {
        println!(
            "{}",
            json!({"capture": entry, "work": item, "request": request})
        );
        return Ok(String::new());
    }
    let now = crate::when::now();
    match &item {
        Some(item) => println!("{}", work_line(item, now)),
        None => println!("{}", capture_line(&entry, now)),
    }
    if let Some(request) = &request {
        println!("{}", crate::assist::request_line(request));
    }
    Ok(String::new())
}

/// The Work items, newest open first, and the unsorted Captures.
pub async fn list(conn: &mut Conn, all: bool, as_json: bool) -> Result<String, String> {
    let Listing {
        work,
        captures,
        requests,
        ..
    } = fetch(conn).await?;
    let work: Vec<WorkItem> = work.into_iter().filter(|w| all || !closed(w)).collect();
    if as_json {
        println!(
            "{}",
            json!({"work": work, "captures": captures, "requests": requests})
        );
        return Ok(String::new());
    }
    let now = crate::when::now();
    for w in &work {
        let asked = latest_request(w, &requests)
            .map(|q| format!("  asked: {}", crate::items::wire(json!(q.delivery))))
            .unwrap_or_default();
        println!("{}{asked}", work_line(w, now));
    }
    for c in &captures {
        println!("{}", capture_line(c, now));
    }
    if work.is_empty() && captures.is_empty() {
        println!("no Work items or unsorted Captures");
    }
    Ok(String::new())
}

/// `work show WORK`: the item with every Capture's full text, its Tasks and
/// its newest request. Capture texts go to stdout only.
pub async fn show(conn: &mut Conn, work: &str, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let messages = conn
        .call(
            |id| Request::GetWork {
                id,
                work: work.to_owned(),
            },
            WAIT,
        )
        .await?;
    let (entry, captures, requests) = messages
        .into_iter()
        .find_map(|m| match m {
            Message::WorkDetail {
                entry,
                captures,
                requests,
                ..
            } => Some((entry, captures, requests)),
            _ => None,
        })
        .ok_or("the bridge sent no Work item")?;
    if as_json {
        println!(
            "{}",
            json!({"work": entry, "captures": captures, "requests": requests})
        );
    } else {
        print!(
            "{}",
            detail_text(&entry, &captures, &requests, crate::when::now())
        );
    }
    Ok(String::new())
}

/// The item's line, each Capture's full text indented, then its Tasks and newest request.
fn detail_text(
    w: &WorkItem,
    captures: &[Capture],
    requests: &[AssistantRequest],
    now: u64,
) -> String {
    let mut out = format!("{}\n", crate::assist::plain(&work_line(w, now)));
    for c in captures {
        out.push_str(&format!(
            "\n{}  {}\n",
            c.id,
            crate::items::age(now.saturating_sub(c.created_at))
        ));
        for line in crate::assist::plain(&c.text).lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }
    if !w.tasks.is_empty() || !w.requests.is_empty() {
        out.push('\n');
    }
    if !w.tasks.is_empty() {
        out.push_str(&format!("Tasks  {}\n", w.tasks.join(" ")));
    }
    if let Some(q) = latest_request(w, requests) {
        out.push_str(&format!("Asked  {}\n", crate::assist::request_line(q)));
    }
    out
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
    let rev = fetch(conn)
        .await?
        .captures
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

/// `work (edit | later | done | withdraw | reopen | merge | pin | unpin | due) WORK …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub work: String,
    pub action: WorkAction,
    pub title: Option<String>,
    pub workspace: Option<String>,
    pub into: Option<String>,
    /// With `edit`, from `work due`.
    pub due: Option<Due>,
}

pub fn action(name: &str) -> Option<WorkAction> {
    match name {
        "done" => Some(WorkAction::Complete),
        "due" => Some(WorkAction::Edit),
        "edit" | "later" | "withdraw" | "reopen" | "merge" | "pin" | "unpin" => {
            serde_json::from_value(json!(name)).ok()
        }
        _ => None,
    }
}

/// `work due --at`: an RFC 3339 time, or `YYYY-MM-DD HH:MM` in `timezone`.
/// The zone defaults to the system's, which must have an IANA name.
pub fn due(at: &str, timezone: Option<&str>) -> Result<Due, String> {
    let zone = zone_name(timezone)?;
    let at = epoch("--at", at, &zone)?;
    Ok(Due { at, timezone: zone })
}

/// The IANA name of `timezone`, or of this system's zone.
pub fn zone_name(timezone: Option<&str>) -> Result<String, String> {
    match timezone {
        Some(name) => Ok(name.to_owned()),
        None => jiff::tz::TimeZone::system()
            .iana_name()
            .map(str::to_owned)
            .ok_or_else(|| {
                "this system's time zone has no IANA name; give --timezone, e.g. Asia/Tokyo".into()
            }),
    }
}

/// Epoch seconds of `at`: RFC 3339, or `YYYY-MM-DD HH:MM` in `zone`. `flag`
/// names the option in the error.
pub fn epoch(flag: &str, at: &str, zone: &str) -> Result<u64, String> {
    let tz = time_zone(zone)?;
    let stamp = match at.parse::<jiff::Timestamp>() {
        Ok(stamp) => stamp,
        Err(_) => at
            .replacen(' ', "T", 1)
            .parse::<jiff::civil::DateTime>()
            .and_then(|local| local.to_zoned(tz))
            .map(|zoned| zoned.timestamp())
            .map_err(|_| format!("{flag} {at:?}: give an RFC 3339 time or YYYY-MM-DD HH:MM"))?,
    };
    match u64::try_from(stamp.as_second()) {
        Ok(at) if at > 0 => Ok(at),
        _ => Err(format!("{flag} {at:?} is before 1970")),
    }
}

pub fn time_zone(zone: &str) -> Result<jiff::tz::TimeZone, String> {
    jiff::tz::TimeZone::get(zone)
        .map_err(|_| format!("unknown time zone {zone:?}; give an IANA name, e.g. Asia/Tokyo"))
}

pub async fn change(conn: &mut Conn, c: Change, as_json: bool) -> Result<String, String> {
    if matches!(c.action, WorkAction::Pin | WorkAction::Unpin) || c.due.is_some() {
        crate::plan::require(conn)?;
    }
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
                due: c.due,
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
    fetch(conn)
        .await?
        .work
        .iter()
        .find(|w| w.id == work)
        .map(|w| w.rev)
        .ok_or_else(|| format!("no Work item {work}"))
}

/// The level 8 to 10 parts of `inbox-list`, and its items.
#[derive(Debug, Default)]
pub struct Listing {
    pub work: Vec<WorkItem>,
    pub captures: Vec<Capture>,
    pub requests: Vec<AssistantRequest>,
    pub items: Vec<InboxItem>,
    /// Newest first.
    pub plans: Vec<Plan>,
    /// Active, then expired.
    pub policies: Vec<Policy>,
    /// `proposed` and `rejected`, newest first.
    pub hypotheses: Vec<Hypothesis>,
    /// Newest first.
    pub corrections: Vec<Correction>,
}

pub async fn fetch(conn: &mut Conn) -> Result<Listing, String> {
    require(conn)?;
    let messages = conn.call(|id| Request::ListInbox { id }, WAIT).await?;
    messages
        .into_iter()
        .find_map(|m| match m {
            Message::InboxList {
                work,
                captures,
                requests,
                items,
                plans,
                policies,
                hypotheses,
                corrections,
                ..
            } => Some(Listing {
                work,
                captures,
                requests,
                items,
                plans,
                policies,
                hypotheses,
                corrections,
            }),
            _ => None,
        })
        .ok_or_else(|| "the bridge sent no inbox list".into())
}

/// The item's newest request: the last one it lists.
pub fn latest_request<'a>(
    w: &WorkItem,
    requests: &'a [AssistantRequest],
) -> Option<&'a AssistantRequest> {
    let id = w.requests.last()?;
    requests.iter().find(|q| &q.id == id)
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
    if w.pinned_at.is_some() {
        state.push_str(" pinned");
    }
    let workspace = w
        .workspace
        .as_deref()
        .map(|s| format!("  [{s}]"))
        .unwrap_or_default();
    let due = w
        .due
        .as_ref()
        .map(|d| format!("  due {}", crate::when::format(d.at)))
        .unwrap_or_default();
    format!(
        "{}  {state}  {}  {}{due}{workspace}",
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
            pinned_at: None,
            due: None,
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
    fn detail_shows_every_capture_in_full_without_control_characters() {
        let mut w = item(WorkState::InProgress, None);
        w.captures = vec!["c-0000abcd".into()];
        w.tasks = vec!["t-0000aa".into(), "t-0000bb".into()];
        let c = Capture {
            id: "c-0000abcd".into(),
            rev: 2,
            client_id: "x".into(),
            text: "Ring the bank\n\tabout \u{1b}[31mfees\u{1b}[0m".into(),
            source: CaptureSource::App,
            state: muxa_protocol::CaptureState::Sorted,
            created_at: 40,
            updated_at: 40,
            workspace: None,
            work: vec![w.id.clone()],
        };
        assert_eq!(
            detail_text(&w, &[c], &[], 100),
            "w-0000abcd  in-progress  0s  Shorten onboarding  [work]\n\
             \nc-0000abcd  1m\n  Ring the bank\n  \tabout [31mfees[0m\n\
             \nTasks  t-0000aa t-0000bb\n"
        );
    }

    #[test]
    fn due_takes_rfc3339_or_a_local_time_in_the_zone() {
        let tokyo = Some("Asia/Tokyo");
        let want = Due {
            at: 1_762_930_800,
            timezone: "Asia/Tokyo".into(),
        };
        assert_eq!(due("2025-11-12T16:00:00+09:00", tokyo), Ok(want.clone()));
        assert_eq!(due("2025-11-12 16:00", tokyo), Ok(want.clone()));
        assert_eq!(due("2025-11-12T16:00", tokyo), Ok(want));
        assert!(due("2025-11-12 16:00", Some("Mars/Base")).is_err());
        assert!(due("Wednesday", tokyo).is_err());
        assert!(due("1960-01-01 00:00", tokyo).is_err());
    }

    #[test]
    fn a_pinned_item_with_a_due_says_so() {
        let mut w = item(WorkState::Open, None);
        w.pinned_at = Some(150);
        w.due = Some(Due {
            at: 1_762_930_800,
            timezone: "Asia/Tokyo".into(),
        });
        assert_eq!(
            work_line(&w, 160),
            "w-0000abcd  open pinned  1m  Shorten onboarding  due 2025-11-12T07:00Z  [work]"
        );
    }

    #[test]
    fn names_map_to_the_wire_and_client_ids_differ() {
        assert_eq!(action("done"), Some(WorkAction::Complete));
        assert_eq!(action("later"), Some(WorkAction::Later));
        assert_eq!(action("complete"), None);
        assert_eq!(action("pin"), Some(WorkAction::Pin));
        assert_eq!(action("due"), Some(WorkAction::Edit));
        assert_eq!(source("tmux"), Some(CaptureSource::Tmux));
        assert_eq!(source("phone"), None);
        let (a, b) = (client_id(), client_id());
        assert_ne!(a, b);
        assert!(a.len() <= 64 && a.bytes().all(|b| b.is_ascii_graphic()));
    }
}
