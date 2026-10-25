//! `changes` and `diff`: lists the files of a pane's repo that differ from HEAD,
//! and follows the diff of the open one, sending it again whenever it changes.

mod git;

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{ChangeInfo, ErrorBody, ErrorCode, Message};
use muxa_tmux::{Client, PaneId};
use tokio::sync::{mpsc, oneshot};

use crate::chat::pane_info;
use crate::convert::error_body;
use git::{Changes, Diff, DiffError};

/// A diff is rebuilt this often while it is open; git status and diff cost
/// more than reading one file.
const TICK: Duration = Duration::from_secs(2);

pub enum Command {
    List {
        id: u64,
        pane: PaneId,
        ack: oneshot::Sender<()>,
    },
    Open {
        id: u64,
        pane: PaneId,
        path: String,
        ack: oneshot::Sender<()>,
    },
    Close {
        id: u64,
        ack: oneshot::Sender<()>,
    },
}

pub struct ChangeTask {
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Command>,
    open: Option<Open>,
}

struct Open {
    id: u64,
    root: PathBuf,
    change: ChangeInfo,
    /// A hash of what was last sent, so an unchanged diff is not sent again.
    sent: Option<u64>,
}

impl ChangeTask {
    pub fn new(
        client: Arc<Client>,
        out: mpsc::Sender<Message>,
        rx: mpsc::Receiver<Command>,
    ) -> Self {
        Self {
            client,
            out,
            rx,
            open: None,
        }
    }

    pub async fn run(mut self) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    Some(cmd) => self.command(cmd).await,
                    None => return,
                },
                _ = tick.tick() => self.refresh().await,
            }
        }
    }

    async fn command(&mut self, cmd: Command) {
        match cmd {
            Command::List { id, pane, ack } => {
                let error = match self.changes(pane).await {
                    Ok(changes) => {
                        let msg = Message::ChangeList {
                            changes: id,
                            root: changes.root_name(),
                            entries: changes.entries,
                            truncated: changes.truncated,
                        };
                        let _ = self.out.send(msg).await;
                        None
                    }
                    Err(e) => Some(e),
                };
                self.result(id, error, ack).await;
            }
            Command::Open {
                id,
                pane,
                path,
                ack,
            } => {
                self.open = None;
                let error = match self.changes(pane).await {
                    Ok(changes) => match (changes.find(&path).cloned(), changes.root) {
                        (Some(change), Some(root)) => {
                            self.open = Some(Open {
                                id,
                                root,
                                change,
                                sent: None,
                            });
                            None
                        }
                        _ => Some(error_body(
                            ErrorCode::BadRequest,
                            format!("{path} is not one of this pane's changed files"),
                        )),
                    },
                    Err(e) => Some(e),
                };
                self.result(id, error, ack).await;
                // The first diff right after the result.
                self.refresh().await;
            }
            Command::Close { id, ack } => {
                self.open = None;
                self.result(id, None, ack).await;
            }
        }
    }

    async fn result(&self, id: u64, error: Option<ErrorBody>, ack: oneshot::Sender<()>) {
        let _ = self
            .out
            .send(Message::Result {
                id: Some(id),
                error,
                sessions: None,
            })
            .await;
        let _ = ack.send(());
    }

    async fn changes(&self, pane: PaneId) -> Result<Changes, ErrorBody> {
        let info = pane_info(&self.client, pane).await?;
        tokio::task::spawn_blocking(move || git::list(std::path::Path::new(&info.path)))
            .await
            .map_err(|e| error_body(ErrorCode::Internal, e.to_string()))
    }

    /// Sends the open diff when it changed since the last send.
    async fn refresh(&mut self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let (root, change) = (open.root.clone(), open.change.clone());
        let built = tokio::task::spawn_blocking(move || git::diff(&root, &change)).await;
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let diff = match built {
            Ok(Ok(diff)) => diff,
            Ok(Err(e)) => {
                if !matches!(e, DiffError::Unchanged(_)) {
                    tracing::warn!(error = %e, "diff no longer available");
                }
                return self.end(e.to_string()).await;
            }
            Err(e) => return self.end(format!("the diff failed: {e}")).await,
        };
        let stamp = stamp(&diff);
        if open.sent == Some(stamp) {
            return;
        }
        open.sent = Some(stamp);
        let msg = Message::DiffContent {
            diff: open.id,
            path: open.change.path.clone(),
            binary: diff.text.is_none(),
            text: diff.text,
        };
        if let Err(mpsc::error::TrySendError::Full(_)) = self.out.try_send(msg) {
            // The client is slow; send it again on the next tick.
            if let Some(o) = self.open.as_mut() {
                o.sent = None;
            }
        }
    }

    async fn end(&mut self, reason: String) {
        if let Some(open) = self.open.take() {
            let _ = self
                .out
                .send(Message::DiffEnded {
                    diff: open.id,
                    reason,
                })
                .await;
        }
    }
}

fn stamp(diff: &Diff) -> u64 {
    let mut h = DefaultHasher::new();
    diff.text.hash(&mut h);
    h.finish()
}
