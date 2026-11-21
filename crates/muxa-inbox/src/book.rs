//! The Inbox in memory: open items in manual order, Later, and recent done
//! items. Pure: every change takes the time and bumps the revision.

use muxa_protocol::v2::{self, InboxItem, ItemKind};
use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
const MAX_TITLE: usize = 300;
const MAX_NOTE: usize = 4096;
const MAX_WORKSPACE: usize = 100;
/// Done items kept for Reopen.
pub const KEEP_DONE: usize = 100;

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
}

impl Default for Book {
    fn default() -> Self {
        Book {
            version: FORMAT_VERSION,
            revision: 0,
            next_id: 1,
            items: Vec::new(),
            done: Vec::new(),
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
        }
    }

    /// Adds `new` on top and returns its id.
    pub fn add(&mut self, new: NewItem, now: i64) -> Result<String, InboxError> {
        let new = checked(new)?;
        if let Some(key) = &new.key
            && let Some(open) = self.items.iter().find(|s| s.key.as_ref() == Some(key))
        {
            return Ok(open.item.id.clone());
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
        };
        self.items.insert(0, Stored { item, key: new.key });
        self.revision += 1;
        Ok(id)
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
        self.items.insert(0, s);
        self.revision += 1;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), InboxError> {
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
    Ok(new)
}

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
        }
    }

    fn titles(b: &Book) -> Vec<String> {
        b.view().items.into_iter().map(|i| i.title).collect()
    }

    #[test]
    fn new_items_go_on_top_and_the_user_orders_them() {
        let mut b = Book::default();
        let a = b.add(todo("a"), 1).unwrap();
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
        let a = b.add(todo("a"), 1).unwrap();
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
        let a = b.add(todo("a"), 1).unwrap();
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
        let first = b.add(keyed(), 1).unwrap();
        assert_eq!(b.add(keyed(), 2).unwrap(), first);
        b.done(&first).unwrap();
        assert_ne!(b.add(keyed(), 3).unwrap(), first);
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
        assert_eq!(
            b.add(todo(" spaced   out "), 1)
                .map(|_| titles(&b)[0].clone()),
            Ok("spaced out".into())
        );
    }
}
