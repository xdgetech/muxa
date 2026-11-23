//! The Inbox in memory: open items in manual order, Later, and recent done
//! items. Pure: every change takes the time and bumps the revision.

use std::collections::{BTreeMap, VecDeque};

use muxa_protocol::v2::{
    self, BotCount, InboxItem, ItemKind, ItemLink, ItemPlace, ItemProposal, RoutineRef,
    SettingsProposal,
};
use serde::{Deserialize, Serialize};

use crate::bots::{self, Ledger};

pub const FORMAT_VERSION: u32 = 1;
pub(crate) const MAX_TITLE: usize = 300;
pub(crate) const MAX_NOTE: usize = 4096;
pub(crate) const MAX_WORKSPACE: usize = 100;
const MAX_LINKS: usize = 10;
const MAX_URL: usize = 2048;
const MAX_PROPOSAL: usize = 8192;
const MAX_BOT: usize = 64;
/// Ops in one settings proposal.
const MAX_PROPOSAL_OPS: usize = 20;
/// Done items kept for Reopen.
pub const KEEP_DONE: usize = 100;
/// Keys remembered for [`NewItem::once`].
const KEEP_SEEN: usize = 2000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InboxError {
    /// The request is wrong; the message says what to change.
    #[error("{0}")]
    Invalid(String),
    #[error("no item {0}")]
    NotFound(String),
}

/// An item to add.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewItem {
    pub kind: ItemKind,
    pub title: String,
    pub note: Option<String>,
    pub workspace: Option<String>,
    pub due: Option<String>,
    /// `capture`, `task` or `routine`.
    pub source: String,
    /// The need that made the item, e.g. `review:<task>`: while an item with
    /// this key is open, adding it again returns that item.
    pub key: Option<String>,
    /// The key never makes a second item, also after this one is done or
    /// deleted: a Routine entry or a crash is reported once.
    pub once: bool,
    /// The Task agent terminal that made the item.
    pub terminal: Option<String>,
    /// The Routine that made the item.
    pub routine: Option<RoutineRef>,
    /// Where an older Task worked, for Resume Task.
    pub place: Option<ItemPlace>,
    /// What a Bot's run adds (ADR 0082).
    pub extra: Extra,
}

/// The parts of an item only Routine and Bot output carries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extra {
    pub links: Vec<ItemLink>,
    pub proposal: Option<ItemProposal>,
    /// The Bot in the item's Workspace.
    pub bot: Option<String>,
    /// Settings a Bot proposed in a talk (ADR 0083).
    pub settings_proposal: Option<SettingsProposal>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    #[serde(flatten)]
    item: InboxItem,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Book {
    version: u32,
    revision: u64,
    next_id: u64,
    items: Vec<Stored>,
    done: Vec<Stored>,
    /// Keys of `once` items, oldest first.
    #[serde(default, skip_serializing_if = "VecDeque::is_empty")]
    seen: VecDeque<String>,
    /// How far each source has been read, e.g. `routines` (Unix seconds).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    cursors: BTreeMap<String, i64>,
    /// Per `<workspace>/<bot>`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    bots: BTreeMap<String, Ledger>,
}

impl Default for Book {
    fn default() -> Self {
        Book {
            version: FORMAT_VERSION,
            revision: 0,
            next_id: 1,
            items: Vec::new(),
            done: Vec::new(),
            seen: VecDeque::new(),
            cursors: BTreeMap::new(),
            bots: BTreeMap::new(),
        }
    }
}

impl Book {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn version(&self) -> u32 {
        self.version
    }

    /// The wire view. Hidden items stay in the list with `later_until`.
    pub fn view(&self) -> v2::Inbox {
        v2::Inbox {
            revision: self.revision,
            items: self.items.iter().map(|s| s.item.clone()).collect(),
            done: self.done.iter().map(|s| s.item.clone()).collect(),
            bots: self
                .bots
                .iter()
                .filter_map(|(key, l)| {
                    let (workspace, bot) = key.split_once('/')?;
                    Some(BotCount {
                        workspace: workspace.to_owned(),
                        bot: bot.to_owned(),
                        done: l.done,
                        deleted: l.deleted,
                        day: l.day.clone(),
                        added: l.added,
                    })
                })
                .collect(),
        }
    }

    /// Adds `new` on top and returns its id: the open item with its key, or
    /// `None` when a `once` key already made one.
    pub fn add(&mut self, new: NewItem, now: i64) -> Result<Option<String>, InboxError> {
        let new = checked(new)?;
        if let Some(key) = &new.key {
            if let Some(open) = self.items.iter().find(|s| s.key.as_ref() == Some(key)) {
                return Ok(Some(open.item.id.clone()));
            }
            if new.once {
                if self.seen.contains(key) {
                    return Ok(None);
                }
                self.seen.push_back(key.clone());
                while self.seen.len() > KEEP_SEEN {
                    self.seen.pop_front();
                }
            }
        }
        let id = format!("i{}", self.next_id);
        self.next_id += 1;
        let item = InboxItem {
            id: id.clone(),
            kind: new.kind,
            title: new.title,
            note: new.note,
            workspace: new.workspace,
            due: new.due,
            later_until: None,
            created: now,
            source: new.source,
            terminal: new.terminal,
            routine: new.routine,
            place: new.place,
            links: new.extra.links,
            proposal: new.extra.proposal,
            bot: new.extra.bot,
            settings_proposal: new.extra.settings_proposal,
            suggestion: None,
        };
        self.items.insert(0, Stored { item, key: new.key });
        self.revision += 1;
        Ok(Some(id))
    }

    /// Adds a Bot's item while the Bot has room on `day` (the host's local
    /// date). Past `limit` the item goes into one Read item, "N more from
    /// <bot>", that lists the titles.
    pub fn add_from_bot(
        &mut self,
        new: NewItem,
        limit: u32,
        day: &str,
        now: i64,
    ) -> Result<Option<String>, InboxError> {
        let (Some(workspace), Some(bot)) = (new.workspace.clone(), new.extra.bot.clone()) else {
            return self.add(new, now);
        };
        if new.key.as_ref().is_some_and(|k| self.knows(k)) {
            return self.add(new, now);
        }
        let ledger = self.bots.entry(bots::key(&workspace, &bot)).or_default();
        if ledger.take(day, limit) {
            return self.add(new, now);
        }
        if let Some(key) = new.key.as_ref().filter(|_| new.once) {
            self.seen.push_back(key.clone());
            while self.seen.len() > KEEP_SEEN {
                self.seen.pop_front();
            }
        }
        let more = bots::more_key(&workspace, &bot, day);
        if let Some(open) = self
            .items
            .iter_mut()
            .find(|s| s.key.as_ref() == Some(&more))
        {
            let count = open
                .item
                .title
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u32>().ok());
            open.item.title = format!("{} more from {bot}", count.unwrap_or(1) + 1);
            let note = open.item.note.get_or_insert_with(String::new);
            let line = format!("- {}\n", new.title);
            if note.len() + line.len() <= MAX_NOTE {
                note.push_str(&line);
            }
            self.revision += 1;
            return Ok(Some(open.item.id.clone()));
        }
        self.add(
            NewItem {
                kind: ItemKind::Read,
                title: format!("1 more from {bot}"),
                note: Some(format!("- {}\n", new.title)),
                workspace: Some(workspace),
                due: None,
                source: new.source,
                key: Some(more),
                once: false,
                terminal: None,
                routine: new.routine,
                place: None,
                // No `bot`: how this item ends says nothing of one of its items.
                extra: Extra::default(),
            },
            now,
        )
    }

    /// An item with `key` is open, or a `once` item had it.
    pub fn knows(&self, key: &str) -> bool {
        self.seen.iter().any(|k| k == key)
            || self.items.iter().any(|s| s.key.as_deref() == Some(key))
    }

    /// The open item `id`.
    pub fn item(&self, id: &str) -> Result<&InboxItem, InboxError> {
        self.open(id).map(|at| &self.items[at].item)
    }

    pub fn cursor(&self, source: &str) -> Option<i64> {
        self.cursors.get(source).copied()
    }

    /// Moves a source's cursor forward; never back.
    pub fn advance(&mut self, source: &str, to: i64) {
        let at = self.cursors.entry(source.to_owned()).or_insert(i64::MIN);
        if to > *at {
            *at = to;
            self.revision += 1;
        }
    }

    /// Puts an open item at `index`; past the end means last.
    pub fn move_to(&mut self, id: &str, index: usize) -> Result<(), InboxError> {
        let from = self.open(id)?;
        let item = self.items.remove(from);
        self.items.insert(index.min(self.items.len()), item);
        self.revision += 1;
        Ok(())
    }

    /// Hides an open item until `until`.
    pub fn later(&mut self, id: &str, until: i64, now: i64) -> Result<(), InboxError> {
        if until <= now {
            return Err(InboxError::Invalid(
                "Later needs a time in the future".into(),
            ));
        }
        let at = self.open(id)?;
        self.items[at].item.later_until = Some(until);
        self.revision += 1;
        Ok(())
    }

    pub fn done(&mut self, id: &str) -> Result<(), InboxError> {
        let at = self.open(id)?;
        let mut s = self.items.remove(at);
        s.item.later_until = None;
        if let Some(l) = self.ledger(&s.item) {
            l.done += 1;
        }
        self.done.insert(0, s);
        self.done.truncate(KEEP_DONE);
        self.revision += 1;
        Ok(())
    }

    /// Puts a done item back on top.
    pub fn reopen(&mut self, id: &str) -> Result<(), InboxError> {
        let at = self
            .done
            .iter()
            .position(|s| s.item.id == id)
            .ok_or_else(|| InboxError::NotFound(id.to_owned()))?;
        let s = self.done.remove(at);
        if let Some(l) = self.ledger(&s.item) {
            l.done = l.done.saturating_sub(1);
        }
        self.items.insert(0, s);
        self.revision += 1;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), InboxError> {
        // A done item was already counted as done.
        if let Some(open) = self
            .items
            .iter()
            .find(|s| s.item.id == id)
            .map(|s| s.item.clone())
        {
            if let Some(l) = self.ledger(&open) {
                l.deleted += 1;
            }
        }
        let before = self.items.len() + self.done.len();
        self.items.retain(|s| s.item.id != id);
        self.done.retain(|s| s.item.id != id);
        if self.items.len() + self.done.len() == before {
            return Err(InboxError::NotFound(id.to_owned()));
        }
        self.revision += 1;
        Ok(())
    }

    /// Brings back the items whose Later time came, on top. True when any did.
    pub fn wake(&mut self, now: i64) -> bool {
        let mut woken: Vec<Stored> = Vec::new();
        self.items.retain(|s| match s.item.later_until {
            Some(t) if t <= now => {
                woken.push(s.clone());
                false
            }
            _ => true,
        });
        if woken.is_empty() {
            return false;
        }
        // The last to wake ends on top.
        woken.sort_by_key(|s| s.item.later_until);
        for mut s in woken {
            s.item.later_until = None;
            self.items.insert(0, s);
        }
        self.revision += 1;
        true
    }

    /// When the next hidden item comes back.
    pub fn next_wake(&self) -> Option<i64> {
        self.items.iter().filter_map(|s| s.item.later_until).min()
    }

    /// The ledger of the Bot that made `item`, if one did.
    fn ledger(&mut self, item: &InboxItem) -> Option<&mut Ledger> {
        let key = bots::key(item.workspace.as_deref()?, item.bot.as_deref()?);
        Some(self.bots.entry(key).or_default())
    }

    fn open(&self, id: &str) -> Result<usize, InboxError> {
        self.items
            .iter()
            .position(|s| s.item.id == id)
            .ok_or_else(|| InboxError::NotFound(id.to_owned()))
    }
}

fn checked(mut new: NewItem) -> Result<NewItem, InboxError> {
    new.title = new.title.split_whitespace().collect::<Vec<_>>().join(" ");
    if new.title.is_empty() || new.title.chars().count() > MAX_TITLE {
        return Err(InboxError::Invalid(format!(
            "a title has 1 to {MAX_TITLE} characters"
        )));
    }
    if new.note.as_ref().is_some_and(|n| n.len() > MAX_NOTE) {
        return Err(InboxError::Invalid(format!(
            "a note has at most {MAX_NOTE} bytes"
        )));
    }
    new.workspace = new.workspace.filter(|w| !w.trim().is_empty());
    if new
        .workspace
        .as_ref()
        .is_some_and(|w| w.len() > MAX_WORKSPACE)
    {
        return Err(InboxError::Invalid("the Workspace name is too long".into()));
    }
    if let Some(due) = &new.due
        && due.parse::<jiff::civil::Date>().is_err()
    {
        return Err(InboxError::Invalid(format!(
            "{due:?} is not a date (YYYY-MM-DD)"
        )));
    }
    check_extra(&new.extra)?;
    Ok(new)
}

fn check_extra(extra: &Extra) -> Result<(), InboxError> {
    let bad = |what: &str| Err(InboxError::Invalid(what.to_owned()));
    if extra.links.len() > MAX_LINKS {
        return bad("an item has at most 10 links");
    }
    for link in &extra.links {
        let web = link.url.starts_with("https://") || link.url.starts_with("http://");
        if !web || link.url.len() > MAX_URL || link.url.chars().any(char::is_whitespace) {
            return bad("a link is an http or https URL without spaces");
        }
        if link
            .title
            .as_ref()
            .is_some_and(|t| t.chars().count() > MAX_TITLE)
        {
            return bad("a link title is too long");
        }
    }
    if let Some(p) = &extra.proposal {
        if p.action.len() + p.target.len() + p.content.len() > MAX_PROPOSAL + 1024 {
            return bad("a proposal is too long");
        }
    }
    if let Some(bot) = &extra.bot {
        if bot.is_empty() || bot.len() > MAX_BOT || bot.contains('/') {
            return bad("a bot name is 1 to 64 bytes without /");
        }
    }
    if let Some(p) = &extra.settings_proposal {
        if p.ops.is_empty() || p.ops.len() > MAX_PROPOSAL_OPS {
            return bad("a settings proposal has 1 to 20 ops");
        }
    }
    Ok(())
}

#[path = "suggest.rs"]
mod suggest;

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(title: &str) -> NewItem {
        NewItem {
            kind: ItemKind::Todo,
            title: title.into(),
            note: None,
            workspace: None,
            due: None,
            source: "capture".into(),
            key: None,
            once: false,
            terminal: None,
            routine: None,
            place: None,
            extra: Default::default(),
        }
    }

    fn add(b: &mut Book, new: NewItem, now: i64) -> String {
        b.add(new, now).unwrap().unwrap()
    }

    fn titles(b: &Book) -> Vec<String> {
        b.view().items.into_iter().map(|i| i.title).collect()
    }

    #[test]
    fn new_items_go_on_top_and_the_user_orders_them() {
        let mut b = Book::default();
        let a = add(&mut b, todo("a"), 1);
        b.add(todo("b"), 2).unwrap();
        b.add(todo("c"), 3).unwrap();
        assert_eq!(titles(&b), ["c", "b", "a"]);
        b.move_to(&a, 0).unwrap();
        assert_eq!(titles(&b), ["a", "c", "b"]);
        b.move_to(&a, 99).unwrap();
        assert_eq!(titles(&b), ["c", "b", "a"]);
        assert_eq!(b.revision(), 5);
    }

    #[test]
    fn later_hides_an_item_and_it_returns_on_top() {
        let mut b = Book::default();
        let a = add(&mut b, todo("a"), 1);
        b.add(todo("b"), 2).unwrap();
        b.later(&a, 100, 10).unwrap();
        assert_eq!(b.next_wake(), Some(100));
        assert!(!b.wake(99));
        b.add(todo("c"), 50).unwrap();
        assert!(b.wake(100));
        assert_eq!(titles(&b), ["a", "c", "b"]);
        assert_eq!(b.view().items[0].later_until, None);
        assert_eq!(
            b.later(&a, 5, 10),
            Err(InboxError::Invalid(
                "Later needs a time in the future".into()
            ))
        );
    }

    #[test]
    fn done_items_can_be_reopened_or_deleted() {
        let mut b = Book::default();
        let a = add(&mut b, todo("a"), 1);
        b.add(todo("b"), 2).unwrap();
        b.done(&a).unwrap();
        assert_eq!(titles(&b), ["b"]);
        b.reopen(&a).unwrap();
        assert_eq!(titles(&b), ["a", "b"]);
        b.delete(&a).unwrap();
        assert_eq!(b.delete(&a), Err(InboxError::NotFound(a)));
    }

    #[test]
    fn an_open_need_makes_one_item() {
        let mut b = Book::default();
        let keyed = || NewItem {
            key: Some("review:t1".into()),
            kind: ItemKind::Review,
            ..todo("review fix")
        };
        let first = add(&mut b, keyed(), 1);
        assert_eq!(add(&mut b, keyed(), 2), first);
        b.done(&first).unwrap();
        assert_ne!(add(&mut b, keyed(), 3), first);
    }

    #[test]
    fn a_once_key_makes_one_item_ever() {
        let mut b = Book::default();
        let once = || NewItem {
            key: Some("routine:e-1".into()),
            once: true,
            ..todo("digest")
        };
        let id = add(&mut b, once(), 1);
        b.done(&id).unwrap();
        b.delete(&id).unwrap();
        assert_eq!(b.add(once(), 2), Ok(None));
        assert_eq!(b.view().items.len(), 0);
        assert!(b.knows("routine:e-1"));
        assert!(!b.knows("routine:e-2"));
    }

    #[test]
    fn a_cursor_only_moves_forward() {
        let mut b = Book::default();
        assert_eq!(b.cursor("routines"), None);
        b.advance("routines", 10);
        b.advance("routines", 5);
        assert_eq!((b.cursor("routines"), b.revision()), (Some(10), 1));
    }

    #[test]
    fn bad_items_are_refused() {
        let mut b = Book::default();
        assert!(b.add(todo("  "), 1).is_err());
        let late = NewItem {
            due: Some("friday".into()),
            ..todo("x")
        };
        assert!(b.add(late, 1).is_err());
        add(&mut b, todo(" spaced   out "), 1);
        assert_eq!(titles(&b)[0], "spaced out");
    }

    fn from_bot(title: &str, key: &str) -> NewItem {
        NewItem {
            workspace: Some("w".into()),
            key: Some(key.into()),
            once: true,
            extra: Extra {
                bot: Some("news".into()),
                links: vec![ItemLink {
                    url: "https://example.com/a".into(),
                    title: None,
                }],
                proposal: None,
                settings_proposal: None,
            },
            ..todo(title)
        }
    }

    #[test]
    fn a_bot_counts_its_items_per_day_and_how_they_end() {
        let mut b = Book::default();
        let first = b
            .add_from_bot(from_bot("a", "k1"), 1, "2026-11-23", 1)
            .unwrap()
            .unwrap();
        let more = b
            .add_from_bot(from_bot("b", "k2"), 1, "2026-11-23", 2)
            .unwrap()
            .unwrap();
        // A key it already sent takes no room and makes no item.
        assert_eq!(
            b.add_from_bot(from_bot("b", "k2"), 1, "2026-11-23", 3),
            Ok(None)
        );
        b.add_from_bot(from_bot("c", "k3"), 1, "2026-11-23", 4)
            .unwrap();
        assert_eq!(titles(&b), ["2 more from news", "a"]);
        assert_eq!(b.item(&first).unwrap().links.len(), 1);
        let today = &b.view().bots[0];
        assert_eq!(
            (today.day.as_str(), today.added),
            ("2026-11-23", 1),
            "the \"more\" item and its parts do not count"
        );
        b.add_from_bot(from_bot("d", "k4"), 1, "2026-11-24", 5)
            .unwrap();
        assert_eq!(titles(&b)[0], "d");
        let d = b.view().items[0].id.clone();
        b.done(&first).unwrap();
        b.delete(&d).unwrap();
        // Deleting the "more" item counts for none of its items.
        b.delete(&more).unwrap();
        let counts = b.view().bots;
        assert_eq!((counts[0].done, counts[0].deleted), (1, 1));
        b.reopen(&first).unwrap();
        assert_eq!(b.view().bots[0].done, 0);
    }

    #[test]
    fn bad_links_and_bot_names_are_refused() {
        let mut b = Book::default();
        let mut item = from_bot("a", "k");
        item.extra.links[0].url = "file:///etc/passwd".into();
        assert!(b.add(item, 1).is_err());
        let mut item = from_bot("a", "k");
        item.extra.bot = Some("a/b".into());
        assert!(b.add(item, 1).is_err());
    }
}
