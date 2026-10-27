//! `images` and `image`: lists a pane's images, and follows the open one, sending
//! the whole file again whenever it changes, e.g. when a test run rewrites it.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{ErrorBody, ErrorCode, Message};
use muxa_tmux::{Client, PaneId};
use tokio::sync::{mpsc, oneshot};

use super::images::{self, Image, Listing};
use super::{Command, read_error};
use crate::chat::pane_info;
use crate::convert::error_body;

/// Images are larger than documents, so they are checked less often.
const TICK: Duration = Duration::from_secs(2);

pub struct ImageTask {
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Command>,
    open: Option<Open>,
}

struct Open {
    id: u64,
    image: Image,
    /// The (mtime, size) last sent, so an unchanged file is not sent again.
    sent: Option<(Option<u64>, usize)>,
}

impl ImageTask {
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
                let error = match self.listing(pane).await {
                    Ok(listing) => {
                        let msg = Message::ImageList {
                            images: id,
                            root: listing.root,
                            entries: listing.images.into_iter().map(|i| i.info).collect(),
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
                        Ok(image) => {
                            self.open = Some(Open {
                                id,
                                image: image.clone(),
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
        let path = PathBuf::from(info.path);
        tokio::task::spawn_blocking(move || images::list(&path))
            .await
            .map_err(|e| error_body(ErrorCode::Internal, e.to_string()))
    }

    /// Sends the open image when it changed since the last send.
    async fn refresh(&mut self) {
        let Some(open) = self.open.as_ref() else {
            return;
        };
        let image = open.image.clone();
        let read = tokio::task::spawn_blocking(move || images::read(&image)).await;
        let Some(open) = self.open.as_mut() else {
            return;
        };
        let (data, mtime) = match read {
            Ok(Ok(content)) => content,
            Ok(Err(e)) => {
                tracing::warn!(image = %open.image.info.path, error = %e, "image no longer readable");
                return self.end(e.to_string()).await;
            }
            Err(e) => return self.end(format!("the image reader failed: {e}")).await,
        };
        let stamp = (mtime, data.len());
        if open.sent == Some(stamp) {
            return;
        }
        open.sent = Some(stamp);
        let msg = Message::ImageContent {
            image: open.id,
            path: open.image.info.path.clone(),
            data,
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
                .send(Message::ImageEnded {
                    image: open.id,
                    reason,
                })
                .await;
        }
    }
}
