//! The server's one Inbox: changes in order, saved before they are seen,
//! a revision for watchers, and the timer that brings Later items back.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use muxa_protocol::v2;
use tokio::sync::watch;

use crate::book::{Book, InboxError};
use crate::store::{self, StoreError};

/// The longest the timer sleeps: a clock change or a sleeping Mac is caught up.
const MAX_SLEEP: Duration = Duration::from_secs(60);

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
        next.wake(now);
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

    /// Brings Later items back when their time comes, for as long as the
    /// server runs.
    pub fn spawn_timer(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let inbox = Arc::clone(self);
        tokio::spawn(async move {
            let mut changes = inbox.subscribe();
            loop {
                let wait = inbox
                    .lock()
                    .next_wake()
                    .map(|t| Duration::from_secs(u64::try_from(t - now()).unwrap_or(0)))
                    .unwrap_or(MAX_SLEEP)
                    .min(MAX_SLEEP);
                tokio::select! {
                    () = tokio::time::sleep(wait) => {
                        let _ = inbox.change(|b, now| {
                            b.wake(now);
                            Ok(())
                        });
                    }
                    changed = changes.changed() => {
                        if changed.is_err() {
                            return;
                        }
                    }
                }
            }
        })
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

    #[tokio::test]
    async fn a_later_item_comes_back_on_time() {
        let inbox = Arc::new(Inbox::memory());
        let id = inbox.change(|b, now| b.add(todo("a"), now)).unwrap();
        inbox.change(|b, now| b.later(&id, now + 1, now)).unwrap();
        let _timer = inbox.spawn_timer();
        let mut rx = inbox.subscribe();
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                rx.changed().await.unwrap();
                if inbox.view().items[0].later_until.is_none() {
                    return;
                }
            }
        })
        .await
        .expect("woken within 3 s");
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
