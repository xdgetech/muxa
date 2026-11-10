//! One file per Task in `<state dir>/items/`: the Task's Inbox items, newest
//! last (ADR 0033). A locked, atomic write per change, as the Routine store.
use std::fs::{self, DirBuilder};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use muxa_protocol::{InboxItem, ItemState};
use serde::{Deserialize, Serialize};

use crate::records::store::Lock;
use crate::routines::store::{StateError, read_json, write_json};

pub const FORMAT_VERSION: u32 = 1;
const DIR: &str = "items";
const DIR_MODE: u32 = 0o700;
/// A whole file, in bytes: 50 items with 4 KiB summaries fit.
const MAX_FILE: u64 = 1 << 20;
/// Items kept per Task. Closed items go first.
pub const KEEP_ITEMS: usize = 50;

/// An item with what only the host needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredItem {
    #[serde(flatten)]
    pub item: InboxItem,
    /// What made the item, e.g. `review:1762160400`: the same need never
    /// makes a second item.
    pub key: String,
}

/// A Task's item file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskItems {
    pub version: u32,
    pub task: String,
    #[serde(default)]
    pub items: Vec<StoredItem>,
    /// The keys of deleted items, oldest first: a need whose item was
    /// deleted is not made again (ADR 0046).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<String>,
    /// Fields written by a newer muxa; kept on rewrite.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl TaskItems {
    pub fn new(task: &str) -> Self {
        Self {
            version: FORMAT_VERSION,
            task: task.to_owned(),
            items: Vec::new(),
            deleted: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    /// Whether a need with this key already made an item, kept or deleted.
    pub fn has_key(&self, key: &str) -> bool {
        self.items.iter().any(|s| s.key == key) || self.deleted.iter().any(|k| k == key)
    }

    /// Deletes the unprotected items made before `cutoff` (epoch seconds),
    /// keeping their keys. Returns how many it deleted.
    pub fn delete_before(&mut self, cutoff: u64) -> u32 {
        let (gone, kept): (Vec<StoredItem>, Vec<StoredItem>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|s| s.item.created_at < cutoff && s.item.protected_at.is_none());
        self.items = kept;
        self.deleted.extend(gone.iter().map(|s| s.key.clone()));
        u32::try_from(gone.len()).unwrap_or(u32::MAX)
    }

    /// The one item that is open or later, if any. ADR 0033 allows one.
    pub fn current_mut(&mut self) -> Option<&mut StoredItem> {
        self.items
            .iter_mut()
            .rev()
            .find(|s| matches!(s.item.state, ItemState::Open | ItemState::Later))
    }

    /// Drops the oldest closed items, then the oldest of all, down to [`KEEP_ITEMS`].
    fn prune(&mut self) {
        while self.items.len() > KEEP_ITEMS {
            let closed = self
                .items
                .iter()
                .position(|s| matches!(s.item.state, ItemState::Answered | ItemState::Resolved));
            self.items.remove(closed.unwrap_or(0));
        }
        let extra = self.deleted.len().saturating_sub(KEEP_ITEMS);
        self.deleted.drain(..extra);
    }
}

#[derive(Debug, Clone)]
pub struct ItemStore {
    dir: PathBuf,
}

impl ItemStore {
    /// `<state dir>/items`, created with mode 0700.
    pub fn open(state_dir: &Path) -> Result<Self, StateError> {
        let dir = state_dir.join(DIR);
        DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(&dir)
            .map_err(|source| StateError::Io {
                action: "create",
                path: dir.clone(),
                source,
            })?;
        Ok(Self { dir })
    }

    /// The Task's items; an empty file when it has none.
    pub fn get(&self, task: &str) -> Result<TaskItems, StateError> {
        check_task(task)?;
        Ok(self.read(task)?.unwrap_or_else(|| TaskItems::new(task)))
    }

    /// Every readable file. An unreadable one is logged by id and skipped.
    pub fn all(&self) -> Result<Vec<TaskItems>, StateError> {
        let entries = fs::read_dir(&self.dir).map_err(|source| StateError::Io {
            action: "read",
            path: self.dir.clone(),
            source,
        })?;
        let mut out = Vec::new();
        for entry in entries {
            let name = match entry {
                Ok(e) => e.file_name(),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot read an item file entry");
                    continue;
                }
            };
            let Some(task) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if check_task(task).is_err() {
                continue;
            }
            match self.read(task) {
                Ok(Some(items)) => out.push(items),
                Ok(None) => {}
                Err(e) => tracing::warn!(task, error = %e, "skipping an item file"),
            }
        }
        Ok(out)
    }

    /// The Task that holds item `id`.
    pub fn owner(&self, id: &str) -> Result<Option<String>, StateError> {
        Ok(self
            .all()?
            .into_iter()
            .find(|t| t.items.iter().any(|s| s.item.id == id))
            .map(|t| t.task))
    }

    /// Applies `change` under the Task's lock and writes the result
    /// atomically. Nothing is written when `change` fails.
    pub fn update<T, E: From<StateError>>(
        &self,
        task: &str,
        change: impl FnOnce(&mut TaskItems) -> Result<T, E>,
    ) -> Result<T, E> {
        check_task(task)?;
        let _lock = Lock::acquire(&self.dir, task, true).map_err(StateError::from)?;
        let mut items = self.read(task)?.unwrap_or_else(|| TaskItems::new(task));
        let out = change(&mut items)?;
        items.version = FORMAT_VERSION;
        items.prune();
        write_json(&self.dir, task, &items, MAX_FILE)?;
        Ok(out)
    }

    fn read(&self, task: &str) -> Result<Option<TaskItems>, StateError> {
        let path = self.dir.join(format!("{task}.json"));
        let Some(items) = read_json::<TaskItems>(&path, task, MAX_FILE)? else {
            return Ok(None);
        };
        if items.version != FORMAT_VERSION {
            return Err(StateError::Unreadable(
                task.to_owned(),
                format!("format version {} is not {FORMAT_VERSION}", items.version),
            ));
        }
        Ok(Some(items))
    }
}

/// A Task id names the file, so only `t-` and hex digits pass.
fn check_task(task: &str) -> Result<(), StateError> {
    let ok = task.strip_prefix("t-").is_some_and(|h| {
        !h.is_empty() && h.len() <= 32 && h.bytes().all(|b| b.is_ascii_hexdigit())
    });
    if ok {
        Ok(())
    } else {
        Err(StateError::Unreadable(
            task.to_owned(),
            "not a task id".into(),
        ))
    }
}

/// `i-` and 8 hex digits.
pub fn new_id() -> Result<String, StateError> {
    Ok(format!("i-{:08x}", getrandom::u32()?))
}

/// An open-ended item for tests of this module and its parent.
#[cfg(test)]
pub(crate) fn tests_item(id: &str, state: ItemState) -> InboxItem {
    InboxItem {
        id: id.into(),
        kind: muxa_protocol::ItemKind::Review,
        state,
        question: "Ready for review: Fixture".into(),
        workspace: "w".into(),
        title: "Fixture".into(),
        created_at: 1,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary: None,
        task: Some("t-000001".into()),
        session: None,
        pane: None,
        prompt: None,
        attachments: Default::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: None,
        answers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muxa-item-store-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn items_are_kept_per_task_and_found_by_id() {
        let dir = temp();
        let store = ItemStore::open(&dir).unwrap();
        store
            .update::<_, StateError>("t-000001", |t| {
                t.items.push(StoredItem {
                    item: tests_item("i-00000001", ItemState::Open),
                    key: "review:1".into(),
                });
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store.owner("i-00000001").unwrap().as_deref(),
            Some("t-000001")
        );
        assert_eq!(store.owner("i-00000002").unwrap(), None);
        let mut got = store.get("t-000001").unwrap();
        assert_eq!(got.current_mut().unwrap().item.id, "i-00000001");
        assert!(store.get("../x").is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pruning_drops_closed_items_first() {
        let mut t = TaskItems::new("t-000001");
        t.items.push(StoredItem {
            item: tests_item("i-open", ItemState::Open),
            key: "k".into(),
        });
        for n in 0..KEEP_ITEMS {
            t.items.push(StoredItem {
                item: tests_item(&format!("i-{n}"), ItemState::Answered),
                key: format!("k{n}"),
            });
        }
        t.prune();
        assert_eq!(t.items.len(), KEEP_ITEMS);
        assert_eq!(t.items[0].item.id, "i-open");
    }

    #[test]
    fn deleting_keeps_protected_and_newer_items_and_remembers_keys() {
        let mut t = TaskItems::new("t-000001");
        for (id, created_at, protected) in [
            ("i-old", 10, false),
            ("i-kept", 10, true),
            ("i-new", 20, false),
        ] {
            let mut item = tests_item(id, ItemState::Open);
            item.created_at = created_at;
            item.protected_at = protected.then_some(15);
            t.items.push(StoredItem {
                item,
                key: format!("k-{id}"),
            });
        }
        assert_eq!(t.delete_before(20), 1);
        let ids: Vec<&str> = t.items.iter().map(|s| s.item.id.as_str()).collect();
        assert_eq!(ids, ["i-kept", "i-new"]);
        assert!(t.has_key("k-i-old"), "a deleted need is not made again");
        for n in 0..=KEEP_ITEMS {
            t.deleted.push(format!("k{n}"));
        }
        t.prune();
        assert_eq!(t.deleted.len(), KEEP_ITEMS);
        assert!(!t.has_key("k-i-old"), "the oldest keys go first");
    }
}
