//! Inbox items from Routines (ADR 0066 decision 5) while Routines still run
//! in the v1 engine: new entries and failures in the Routine store are copied
//! into the Inbox. Entries from before the first copy are not (no migration).

use std::sync::Arc;
use std::time::Duration;

use muxa_inbox::{Inbox, NewItem};
use muxa_protocol::v2::{ItemKind, RoutineRef};
use muxa_protocol::{InboxEntryKind, RoutineFailure};

use crate::routines::store::{RoutineState, RoutineStore};

const EVERY: Duration = Duration::from_secs(15);
const SOURCE: &str = "routines";
/// [`muxa_inbox`]'s note limit, in bytes.
const MAX_NOTE: usize = 4096;

/// Copies Routine output into `inbox` every 15 s, for as long as the daemon runs.
pub fn spawn(inbox: Arc<Inbox>, records: Arc<crate::config::Config>) {
    tokio::spawn(async move {
        loop {
            let config = Arc::clone(&records);
            let read = tokio::task::spawn_blocking(move || {
                let dir = config.state_dir.as_ref().filter(|d| d.is_dir())?;
                Some(RoutineStore::open(dir).and_then(|s| s.all()))
            })
            .await;
            match read {
                Ok(Some(Ok(states))) => copy(&inbox, &states),
                Ok(Some(Err(e))) => tracing::warn!(error = %e, "cannot read Routine output"),
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "the Routine feed failed"),
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

fn copy(inbox: &Inbox, states: &[RoutineState]) {
    let result = inbox.change(|b, now| {
        let Some(since) = b.cursor(SOURCE) else {
            // The first copy: start now, and leave older entries behind.
            b.advance(SOURCE, now);
            return Ok(());
        };
        let (items, newest) = collect(states, since);
        for item in items {
            b.add(item, now)?;
        }
        if let Some(t) = newest {
            b.advance(SOURCE, t);
        }
        Ok(())
    });
    if let Err(e) = result {
        tracing::warn!(error = %e, "Routine output not copied to the Inbox");
    }
}

/// Items for the entries and failures at or after `since`, and the newest
/// time among them. A key per entry or failure makes each one item at most.
fn collect(states: &[RoutineState], since: i64) -> (Vec<NewItem>, Option<i64>) {
    let at = |t: u64| i64::try_from(t).unwrap_or(i64::MAX);
    let mut items = Vec::new();
    let mut newest = None;
    for s in states {
        for e in s.entries.iter().map(|st| &st.entry) {
            let kind = match e.kind {
                InboxEntryKind::Briefing => ItemKind::Read,
                InboxEntryKind::Proposal => ItemKind::Decide,
                InboxEntryKind::Unknown => continue,
            };
            if at(e.created_at) < since {
                continue;
            }
            newest = newest.max(Some(at(e.created_at)));
            items.push(NewItem {
                kind,
                title: e.title.clone(),
                note: Some(clip(&e.body)).filter(|b| !b.is_empty()),
                workspace: Some(s.workspace.clone()),
                due: None,
                source: "routine".into(),
                key: Some(format!("routine:{}", e.id)),
                once: true,
                terminal: None,
                routine: Some(routine_of(s)),
                place: None,
            });
        }
        if let Some(f) = s.last_failure.as_ref().filter(|f| at(f.at) >= since) {
            newest = newest.max(Some(at(f.at)));
            items.push(failed(s, f));
        }
    }
    (items, newest)
}

fn failed(s: &RoutineState, f: &RoutineFailure) -> NewItem {
    NewItem {
        kind: ItemKind::Failed,
        title: format!("{} failed", s.name),
        note: Some(clip(&f.reason)),
        workspace: Some(s.workspace.clone()),
        due: None,
        source: "routine".into(),
        key: Some(format!(
            "routine-failed:{}/{}:{}",
            s.workspace, s.name, f.at
        )),
        once: true,
        terminal: None,
        routine: Some(routine_of(s)),
        place: None,
    }
}

fn routine_of(s: &RoutineState) -> RoutineRef {
    RoutineRef {
        workspace: s.workspace.clone(),
        name: s.name.clone(),
    }
}

fn clip(text: &str) -> String {
    let mut end = text.len().min(MAX_NOTE);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn state(entries: serde_json::Value, failure: serde_json::Value) -> RoutineState {
        serde_json::from_value(json!({"version": 1, "workspace": "lisk", "name": "digest",
            "entries": entries, "lastFailure": failure}))
        .unwrap()
    }

    fn entry(id: &str, kind: &str, at: u64) -> serde_json::Value {
        json!({"id": id, "workspace": "lisk", "routine": "digest", "key": id, "kind": kind,
               "title": format!("t {id}"), "body": "b", "createdAt": at, "run": "r",
               "occurrence": "o", "disposition": "open"})
    }

    #[test]
    fn new_entries_and_failures_become_items() {
        let s = state(
            json!([
                entry("e-1", "briefing", 90),
                entry("e-2", "proposal", 100),
                entry("e-3", "briefing", 120)
            ]),
            json!({"at": 130, "occurrence": "o", "reason": "gh login expired"}),
        );
        let (items, newest) = collect(&[s], 100);
        let got: Vec<_> = items.iter().map(|i| (i.kind, i.title.as_str())).collect();
        assert_eq!(
            got,
            [
                (ItemKind::Decide, "t e-2"),
                (ItemKind::Read, "t e-3"),
                (ItemKind::Failed, "digest failed")
            ]
        );
        assert_eq!(newest, Some(130));
        assert_eq!(items[2].note.as_deref(), Some("gh login expired"));
        assert_eq!(items[0].workspace.as_deref(), Some("lisk"));
        let digest = RoutineRef {
            workspace: "lisk".into(),
            name: "digest".into(),
        };
        assert!(items.iter().all(|i| i.routine.as_ref() == Some(&digest)));
    }

    #[test]
    fn the_first_copy_takes_nothing_and_later_ones_take_each_entry_once() {
        let inbox = Inbox::memory();
        let old = state(json!([entry("e-1", "briefing", 1)]), json!(null));
        copy(&inbox, std::slice::from_ref(&old));
        assert!(inbox.view().items.is_empty());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let fresh = state(
            json!([
                entry("e-1", "briefing", 1),
                entry("e-2", "briefing", now + 5)
            ]),
            json!(null),
        );
        copy(&inbox, std::slice::from_ref(&fresh));
        copy(&inbox, &[fresh]);
        let titles: Vec<_> = inbox.view().items.into_iter().map(|i| i.title).collect();
        assert_eq!(titles, ["t e-2"]);
    }

    #[test]
    fn a_long_note_is_cut_at_a_character() {
        let text = "é".repeat(3000);
        assert!(clip(&text).len() <= MAX_NOTE);
        assert!(clip(&text).chars().all(|c| c == 'é'));
    }
}
