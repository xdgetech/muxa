//! The server's one Inbox: changes in order, saved before they are seen,
//! and a revision for watchers.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use muxa_protocol::v2;
use tokio::sync::watch;

use crate::book::{Book, InboxError};
use crate::store::{self, StoreError};

pub struct Inbox {
    /// None keeps the Inbox in memory only (tests).
    path: Option<PathBuf>,
    book: Mutex<Book>,
    revision: watch::Sender<u64>,
}

impl Inbox {
    pub fn open(path: PathBuf) -> Result<Self, StoreError> {
        let book = store::load(&path)?;
        Ok(Self::with(Some(path), book))
    }

    pub fn memory() -> Self {
        Self::with(None, Book::default())
    }

    fn with(path: Option<PathBuf>, book: Book) -> Self {
        let (revision, _) = watch::channel(book.revision());
        Inbox {
            path,
            book: Mutex::new(book),
            revision,
        }
    }

    fn lock(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn view(&self) -> v2::Inbox {
        self.lock().view()
    }

    /// The open item `id`, as it is now.
    pub fn item(&self, id: &str) -> Result<v2::InboxItem, InboxError> {
        self.lock().item(id).cloned()
    }

    /// Fires on each new revision.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.revision.subscribe()
    }

    /// Applies `f` to a copy, saves it, then makes it the Inbox. A failed
    /// save changes nothing.
    pub fn change<T>(
        &self,
        f: impl FnOnce(&mut Book, i64) -> Result<T, InboxError>,
    ) -> Result<T, InboxError> {
        let now = now();
        let mut book = self.lock();
        let mut next = book.clone();
        let out = f(&mut next, now)?;
        if next.revision() == book.revision() {
            return Ok(out);
        }
        if let Some(path) = &self.path {
            store::save(path, &next).map_err(|e| {
                tracing::warn!(error = %e, "the Inbox was not saved");
                InboxError::Invalid(format!("{e}. The change was not made"))
            })?;
        }
        let revision = next.revision();
        *book = next;
        drop(book);
        self.revision.send_replace(revision);
        Ok(out)
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use muxa_protocol::v2::ItemKind;

    use super::*;
    use crate::NewItem;

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

    #[tokio::test]
    async fn a_change_reaches_watchers_and_a_refused_one_does_not() {
        let inbox = Inbox::memory();
        let mut rx = inbox.subscribe();
        inbox.change(|b, now| b.add(todo("a"), now)).unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow_and_update(), 1);
        assert!(inbox.change(|b, _| b.done("nope")).is_err());
        assert!(!rx.has_changed().unwrap());
    }

    #[test]
    fn a_reopened_server_has_the_same_inbox() {
        let dir = std::env::temp_dir().join(format!("muxa-inbox-svc-{}", std::process::id()));
        let path = dir.join("inbox.json");
        let _ = std::fs::remove_dir_all(&dir);
        let inbox = Inbox::open(path.clone()).unwrap();
        inbox.change(|b, now| b.add(todo("kept"), now)).unwrap();
        assert_eq!(Inbox::open(path).unwrap().view(), inbox.view());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
