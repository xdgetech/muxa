//! Inbox items from Routines (ADR 0066 decision 5) while Routines still run
//! in the v1 engine: new entries and failures in the Routine store are copied
//! into the Inbox. Entries from before the first copy are not (no migration).
//! A Bot's entries count against its daily limit (ADR 0082).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use muxa_inbox::{Extra, Inbox, NewItem};
use muxa_protocol::v2::{ItemKind, ItemLink, ItemProposal, RoutineRef};
use muxa_protocol::{InboxEntryKind, RoutineFailure};

use crate::routines::store::{RoutineState, RoutineStore};

/// `(workspace, bot)` to its daily limit.
type Limits = BTreeMap<(String, String), u32>;

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
                let states = RoutineStore::open(dir).and_then(|s| s.all());
                Some(states.map(|s| (s, limits(&config))))
            })
            .await;
            match read {
                Ok(Some(Ok((states, limits)))) => copy(&inbox, &states, &limits),
                Ok(Some(Err(e))) => tracing::warn!(error = %e, "cannot read Routine output"),
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "the Routine feed failed"),
            }
            tokio::time::sleep(EVERY).await;
        }
    });
}

/// Blocking. The configured Bots' limits; none when the config cannot load.
fn limits(config: &crate::config::Config) -> Limits {
    let Ok(workspaces) = crate::routines::runs::load(config) else {
        return Limits::new();
    };
    workspaces
        .bots()
        .iter()
        .map(|b| ((b.workspace.clone(), b.name.clone()), b.daily_items))
        .collect()
}

fn copy(inbox: &Inbox, states: &[RoutineState], limits: &Limits) {
    let day = jiff::Zoned::now().date().to_string();
    let result = inbox.change(|b, now| {
        let Some(since) = b.cursor(SOURCE) else {
            // The first copy: start now, and leave older entries behind.
            b.advance(SOURCE, now);
            return Ok(());
        };
        let (items, newest) = collect(states, since);
        for item in items {
            let limit = match (&item.workspace, &item.extra.bot) {
                (Some(w), Some(bot)) => limits.get(&(w.clone(), bot.clone())).copied(),
                _ => None,
            };
            match limit {
                Some(limit) => b.add_from_bot(item, limit, &day, now)?,
                None => b.add(item, now)?,
            };
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
                InboxEntryKind::Reply => ItemKind::Reply,
                InboxEntryKind::Todo => ItemKind::Todo,
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
                due: e.due.clone(),
                source: "routine".into(),
                key: Some(format!("routine:{}", e.id)),
                once: true,
                terminal: None,
                routine: Some(routine_of(s)),
                place: None,
                extra: Extra {
                    links: e
                        .links
                        .iter()
                        .map(|l| ItemLink {
                            url: l.url.clone(),
                            title: l.title.clone(),
                        })
                        .collect(),
                    proposal: e.proposal.as_ref().map(|p| ItemProposal {
                        action: p.action.clone(),
                        target: p.target.clone(),
                        content: p.content.clone(),
                    }),
                    bot: e.bot.clone(),
                    settings_proposal: None,
                },
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
        extra: Default::default(),
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
        copy(&inbox, std::slice::from_ref(&old), &Limits::new());
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
        copy(&inbox, std::slice::from_ref(&fresh), &Limits::new());
        copy(&inbox, &[fresh], &Limits::new());
        let titles: Vec<_> = inbox.view().items.into_iter().map(|i| i.title).collect();
        assert_eq!(titles, ["t e-2"]);
    }

    #[test]
    fn a_long_note_is_cut_at_a_character() {
        let text = "é".repeat(3000);
        assert!(clip(&text).len() <= MAX_NOTE);
        assert!(clip(&text).chars().all(|c| c == 'é'));
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    #[test]
    fn a_bot_entry_keeps_its_links_proposal_and_due() {
        let mut e = entry("e-1", "proposal", 100);
        e["links"] = json!([{"url": "https://github.com/o/r/pull/7", "title": "PR 7"}]);
        e["proposal"] =
            json!({"action": "comment", "target": "o/r#7", "content": "LGTM", "hash": "h"});
        e["bot"] = json!("pr-watch");
        let mut r = entry("e-2", "reply", 100);
        r["due"] = json!("2026-11-24");
        let (items, _) = collect(&[state(json!([e, r]), json!(null))], 0);
        assert_eq!(items[0].extra.links[0].url, "https://github.com/o/r/pull/7");
        assert_eq!(items[0].extra.proposal.as_ref().unwrap().content, "LGTM");
        assert_eq!(items[0].extra.bot.as_deref(), Some("pr-watch"));
        assert_eq!(
            (items[1].kind, items[1].due.as_deref()),
            (ItemKind::Reply, Some("2026-11-24"))
        );
    }

    #[test]
    fn a_bot_past_its_daily_limit_adds_one_more_item() {
        let inbox = Inbox::memory();
        copy(&inbox, &[], &Limits::new());
        let t = now() + 5;
        let entries: Vec<_> = (1..=4)
            .map(|n| {
                let mut e = entry(&format!("e-{n}"), "briefing", t);
                e["bot"] = json!("news");
                e
            })
            .collect();
        let limits = Limits::from([(("lisk".to_owned(), "news".to_owned()), 2)]);
        copy(&inbox, &[state(json!(entries), json!(null))], &limits);
        let view = inbox.view();
        let titles: Vec<_> = view.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["2 more from news", "t e-2", "t e-1"]);
        assert!(view.items[0].note.as_deref().unwrap().contains("t e-4"));
    }
}
