//! `docs` and `doc`: lists a pane's Markdown documents, and follows the open one,
//! sending its whole text again whenever it changes.

mod files;

use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{ErrorBody, ErrorCode, Message};
use muxa_tmux::{Client, PaneId};
use tokio::sync::{mpsc, oneshot};

use crate::agents::Registry;
use crate::chat::pane_info;
use crate::convert::error_body;
use files::{Doc, Listing, ReadError};

const TICK: Duration = Duration::from_secs(1);

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

pub struct DocTask {
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Command>,
    agents: Arc<Registry>,
    open: Option<Open>,
}

struct Open {
    id: u64,
    doc: Doc,
    /// What was last sent, so an unchanged file is not sent again.
    sent: Option<(Option<u64>, usize)>,
}

impl DocTask {
    pub fn new(
        client: Arc<Client>,
        out: mpsc::Sender<Message>,
        rx: mpsc::Receiver<Command>,
        agents: Arc<Registry>,
    ) -> Self {
        Self {
            client,
            out,
            rx,
            agents,
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
                let error = match self.listing(pane).await {
                    Ok(listing) => {
                        let msg = Message::DocList {
                            docs: id,
                            root: listing.root,
                            entries: listing.docs.into_iter().map(|d| d.info).collect(),
                            truncated: listing.truncated,
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
                let error = match self.listing(pane).await {
                    Ok(listing) => match listing.find(&path) {
                        Ok(doc) => {
                            self.open = Some(Open {
                                id,
                                doc: doc.clone(),
                                sent: None,
                            });
                            None
                        }
                        Err(e) => Some(read_error(&e)),
                    },
                    Err(e) => Some(e),
                };
                self.result(id, error, ack).await;
                // The first content right after the result.
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

    async fn listing(&self, pane: PaneId) -> Result<Listing, ErrorBody> {
        let info = pane_info(&self.client, pane).await?;
        let agents = Arc::clone(&self.agents);
        tokio::task::spawn_blocking(move || files::list(&info, &agents))
            .await
            .map_err(|e| error_body(ErrorCode::Internal, e.to_string()))
    }

    /// Sends the open document when it changed since the last send.
    async fn refresh(&mut self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let doc = open.doc.clone();
        let read = tokio::task::spawn_blocking(move || files::read(&doc)).await;
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let (text, mtime) = match read {
            Ok(Ok(content)) => content,
            Ok(Err(e)) => {
                tracing::warn!(doc = %open.doc.info.path, error = %e, "document no longer readable");
                return self.end(e.to_string()).await;
            }
            Err(e) => return self.end(format!("the reader failed: {e}")).await,
        };
        let stamp = (mtime, text.len());
        if open.sent == Some(stamp) {
            return;
        }
        open.sent = Some(stamp);
        let msg = Message::DocContent {
            doc: open.id,
            path: open.doc.info.path.clone(),
            text,
            mtime,
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
                .send(Message::DocEnded {
                    doc: open.id,
                    reason,
                })
                .await;
        }
    }
}

fn read_error(e: &ReadError) -> ErrorBody {
    let code = match e {
        ReadError::TooLarge(_) => ErrorCode::TooLarge,
        _ => ErrorCode::BadRequest,
    };
    error_body(code, e.to_string())
}
