//! Client sessions across connections: each live session by token, and the
//! attachments of a client that went away, kept for its resume.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use muxa_backend::TerminalId;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::time::Instant;

use crate::attachment::Cmd;

/// How long a resume waits for the connection it replaces to let go.
const TAKEOVER_WAIT: Duration = Duration::from_secs(2);

pub(crate) struct Handle {
    pub terminal: TerminalId,
    pub cmds: mpsc::UnboundedSender<Cmd>,
}

struct Parked {
    at: Instant,
    channels: HashMap<u32, Handle>,
}

#[derive(Default)]
struct Inner {
    /// Live sessions; the sender asks one to end.
    live: HashMap<String, Option<oneshot::Sender<()>>>,
    parked: HashMap<String, Parked>,
}

/// One per server; shared by every connection.
#[derive(Default)]
pub struct Sessions {
    inner: Mutex<Inner>,
    changed: Notify,
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Registers a live session. The receiver fires when a resume takes over.
    pub(crate) fn begin(&self, token: &str) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        self.lock().live.insert(token.to_owned(), Some(tx));
        rx
    }

    /// The parked attachments of `token`. A connection still live with that
    /// token (the client moved to another network) is ended first.
    pub(crate) async fn claim(&self, token: &str) -> Option<HashMap<u32, Handle>> {
        let deadline = Instant::now() + TAKEOVER_WAIT;
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut inner = self.lock();
                if let Some(p) = inner.parked.remove(token) {
                    return Some(p.channels);
                }
                match inner.live.get_mut(token) {
                    None => return None,
                    Some(kick) => {
                        if let Some(tx) = kick.take() {
                            let _ = tx.send(());
                        }
                    }
                }
            }
            if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return None;
            }
        }
    }

    /// Ends a live session and keeps its attachments for `park_for`.
    pub(crate) fn park(&self, token: &str, channels: HashMap<u32, Handle>, park_for: Duration) {
        let mut inner = self.lock();
        inner.live.remove(token);
        let now = Instant::now();
        inner
            .parked
            .retain(|_, p| now.duration_since(p.at) < park_for);
        if !channels.is_empty() {
            inner
                .parked
                .insert(token.to_owned(), Parked { at: now, channels });
        }
        drop(inner);
        self.changed.notify_waiters();
    }
}
