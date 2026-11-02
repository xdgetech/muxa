//! The event loop that owns the watched pane. It sees tmux notifications in
//! order, which is what lets it cut the output stream exactly at a snapshot:
//! output with `after_block >= screen.block` happened after the capture.

use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::Message;
use muxa_tmux::{
    Client, Error, Grid, Notification, Notifications, Options, PaneId, Screen, SessionId, WindowId,
};
use tokio::sync::{mpsc, oneshot};

use crate::sizer::{Sizer, Target};

const SYNC_TIMEOUT: Duration = Duration::from_secs(5);
/// Output held while a snapshot is in flight; beyond this, take a new one.
const HELD_MAX: usize = 1 << 20;
const TICK: Duration = Duration::from_millis(100);
/// Free writer slots needed before a resync after an overflow.
const RESYNC_ROOM: usize = 64;

/// Notifications after which a client should list sessions again.
const STRUCTURAL: [&str; 9] = [
    "sessions-changed",
    "session-renamed",
    "window-add",
    "window-close",
    "window-renamed",
    "unlinked-window-add",
    "unlinked-window-close",
    "unlinked-window-renamed",
    "layout-change",
];

pub enum Command {
    /// Watch `pane`; the loop sends the request's result itself, before any
    /// screen, then acknowledges.
    Watch {
        id: u64,
        pane: PaneId,
        ack: oneshot::Sender<()>,
    },
    Unwatch {
        id: u64,
        ack: oneshot::Sender<()>,
    },
    /// The client's grid at its default font (ADR 0020).
    Viewport(Grid),
    /// The client typed into this pane.
    Typed(PaneId),
    /// The session is ending: give the window back before the bridge exits.
    Stop(oneshot::Sender<()>),
}

struct Active {
    /// The watch request's id, carried on every screen/output/pane-gone.
    id: u64,
    pane: PaneId,
    window: Option<WindowId>,
    syncing: bool,
    /// A layout change arrived during a sync: take another when it lands.
    dirty: bool,
    retried: bool,
    /// The writer was full: output is dropped until a resync.
    overflowed: bool,
    held: Vec<(u64, Vec<u8>)>,
    held_bytes: usize,
}

struct SyncDone {
    seq: u64,
    result: Result<Synced, Error>,
}

struct Synced {
    screen: Screen,
    session: SessionId,
}

pub struct Watcher {
    client: Arc<Client>,
    out: mpsc::Sender<Message>,
    notes: Notifications,
    commands: mpsc::Receiver<Command>,
    active: Option<Active>,
    session: Option<SessionId>,
    seq: u64,
    sync_tx: mpsc::Sender<SyncDone>,
    sync_rx: mpsc::Receiver<SyncDone>,
    list_changed: bool,
    /// Sessions changed: check that the device session's target still exists.
    check_device: bool,
    sizer: Sizer,
}

impl Watcher {
    pub fn new(
        client: Arc<Client>,
        opts: Options,
        out: mpsc::Sender<Message>,
        notes: Notifications,
        commands: mpsc::Receiver<Command>,
    ) -> Self {
        let (sync_tx, sync_rx) = mpsc::channel(4);
        Self {
            client,
            out,
            notes,
            commands,
            active: None,
            session: None,
            seq: 0,
            sync_tx,
            sync_rx,
            list_changed: false,
            check_device: false,
            sizer: Sizer::new(opts),
        }
    }

    /// Runs until tmux exits or the writer is gone. Sends `bye` when tmux exits.
    pub async fn run(mut self) {
        let mut tick = tokio::time::interval(TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let alive = tokio::select! {
                note = self.notes.recv() => match note {
                    Some(n) => self.note(n),
                    None => {
                        // E.g. the session we were attached to was destroyed:
                        // with detach-on-destroy, tmux detaches its clients.
                        let reason = self.client.exit_reason();
                        self.gone(Some(reason.clone())).await;
                        self.sizer.release(&self.client).await;
                        let _ = self.out.send(Message::Bye { reason }).await;
                        return;
                    }
                },
                Some(cmd) = self.commands.recv() => self.command(cmd).await,
                Some(done) = self.sync_rx.recv() => self.landed(done).await,
                _ = tick.tick() => self.tick().await,
            };
            if !alive {
                return;
            }
        }
    }

    async fn command(&mut self, cmd: Command) -> bool {
        let (id, ack, pane) = match cmd {
            Command::Watch { id, pane, ack } => (id, ack, Some(pane)),
            Command::Unwatch { id, ack } => (id, ack, None),
            Command::Viewport(grid) => {
                self.sizer.set_viewport(&self.client, grid).await;
                return true;
            }
            Command::Typed(pane) => {
                self.sizer.typed(&self.client, pane).await;
                return true;
            }
            Command::Stop(ack) => {
                self.sizer.release(&self.client).await;
                let _ = ack.send(());
                return true;
            }
        };
        self.active = None;
        if pane.is_some() {
            self.sizer.watching();
        } else {
            self.sizer.release(&self.client).await;
        }
        let sent = self
            .out
            .send(Message::Result {
                id: Some(id),
                error: None,
                sessions: None,
            })
            .await
            .is_ok();
        if let Some(pane) = pane {
            self.active = Some(Active {
                id,
                pane,
                window: None,
                syncing: false,
                dirty: false,
                retried: false,
                overflowed: false,
                held: Vec::new(),
                held_bytes: 0,
            });
            self.start_sync();
        }
        let _ = ack.send(());
        sent
    }

    fn start_sync(&mut self) {
        let Some(active) = self.active.as_mut() else {
            return;
        };
        active.syncing = true;
        active.dirty = false;
        active.held.clear();
        active.held_bytes = 0;
        self.seq += 1;
        let (seq, pane, session) = (self.seq, active.pane, self.session);
        let (client, tx) = (Arc::clone(&self.client), self.sync_tx.clone());
        tokio::spawn(async move {
            let result = tokio::time::timeout(SYNC_TIMEOUT, sync(&client, pane, session))
                .await
                .unwrap_or_else(|_| Err(Error::Exited("snapshot timed out".into())));
            // Always delivered, so the loop never stays in "syncing".
            let _ = tx.send(SyncDone { seq, result }).await;
        });
    }

    async fn landed(&mut self, done: SyncDone) -> bool {
        if done.seq != self.seq {
            return true; // superseded by a newer sync or watch
        }
        let Some(active) = self.active.as_mut() else {
            return true;
        };
        active.syncing = false;
        let Synced { screen, session } = match done.result {
            Ok(synced) => synced,
            Err(Error::NoPane(_)) => return self.gone(None).await,
            Err(e) if !active.retried => {
                tracing::warn!(pane = %active.pane, error = %e, "snapshot failed; retrying once");
                active.retried = true;
                self.start_sync();
                return true;
            }
            Err(e) => return self.gone(Some(e.to_string())).await,
        };
        active.retried = false;
        active.overflowed = false;
        active.window = Some(screen.window);
        let held = std::mem::take(&mut active.held);
        active.held_bytes = 0;
        let (id, pane, dirty) = (active.id, active.pane.to_string(), active.dirty);
        let target = Target {
            pane: screen.pane,
            session,
            window: screen.window,
        };
        let seen = crate::sizer::seen(&screen);
        let msg = Message::Screen {
            watch: id,
            pane: pane.clone(),
            cols: screen.cols,
            rows: screen.rows,
            data: screen.data,
        };
        if self.out.send(msg).await.is_err() {
            return false;
        }
        for (after, data) in held {
            if after >= screen.block {
                self.emit(id, &pane, data);
            }
        }
        if dirty {
            self.start_sync();
        }
        self.sizer.landed(&self.client, target, seen).await;
        true
    }

    async fn gone(&mut self, reason: Option<String>) -> bool {
        let Some(active) = self.active.take() else {
            return true;
        };
        self.sizer.release(&self.client).await;
        let msg = Message::PaneGone {
            watch: active.id,
            pane: active.pane.to_string(),
            reason,
        };
        self.out.send(msg).await.is_ok()
    }

    fn note(&mut self, n: Notification) -> bool {
        if let Some((pane, data)) = n.output() {
            let Some(active) = self.active.as_mut() else {
                return true;
            };
            if pane != active.pane {
                return true;
            }
            if active.syncing {
                active.held_bytes += data.len();
                active.held.push((n.after_block, data));
                if active.held_bytes > HELD_MAX {
                    active.dirty = true;
                    active.held.clear();
                    active.held_bytes = 0;
                }
            } else if !active.overflowed {
                let (id, pane) = (active.id, active.pane.to_string());
                self.emit(id, &pane, data);
            }
            return true;
        }
        match n.name.as_str() {
            "session-changed" => self.session = n.first_arg().parse().ok(),
            "sessions-changed" => self.check_device = true,
            "layout-change" | "window-close" | "unlinked-window-close" => {
                if let Some(active) = self.active.as_mut() {
                    let ours = n.name != "layout-change"
                        || active.window.is_none_or(|w| w.to_string() == n.first_arg());
                    if ours {
                        if active.syncing {
                            active.dirty = true;
                        } else {
                            self.start_sync();
                        }
                    }
                }
            }
            _ => {}
        }
        if STRUCTURAL.contains(&n.name.as_str()) {
            self.list_changed = true;
        }
        true
    }

    /// Sends output without waiting. A full writer means the client is
    /// behind: drop output until a fresh screen catches it up.
    fn emit(&mut self, watch: u64, pane: &str, data: Vec<u8>) {
        let msg = Message::Output {
            watch,
            pane: pane.to_owned(),
            data,
        };
        if let Err(mpsc::error::TrySendError::Full(_)) = self.out.try_send(msg) {
            if let Some(active) = self.active.as_mut() {
                active.overflowed = true;
            }
        }
    }

    async fn tick(&mut self) -> bool {
        if self.list_changed {
            self.list_changed = false;
            if self.out.send(Message::SessionsChanged).await.is_err() {
                return false;
            }
        }
        let resync = self
            .active
            .as_ref()
            .is_some_and(|a| a.overflowed && !a.syncing && self.out.capacity() >= RESYNC_ROOM);
        if resync {
            self.start_sync();
        }
        if std::mem::take(&mut self.check_device) && self.sizer.target_gone(&self.client).await {
            // Only the device session holds the pane now: the next snapshot
            // finds it gone, which closes the device session.
            match self.active.as_mut() {
                Some(active) if active.syncing => active.dirty = true,
                Some(_) => self.start_sync(),
                None => self.sizer.release(&self.client).await,
            }
        }
        let syncing = self.active.as_ref().is_some_and(|a| a.syncing || a.dirty);
        self.sizer.tick(syncing).await;
        !self.out.is_closed()
    }
}

/// Moves the control client to the pane's session (tmux sends %output only
/// for that session), then snapshots the pane. This client never holds a
/// size: the device session's client does (ADR 0021).
async fn sync(client: &Client, pane: PaneId, current: Option<SessionId>) -> Result<Synced, Error> {
    let (session, _) = muxa_tmux::locate(client, pane).await?;
    if current != Some(session) {
        client
            .command(&format!("switch-client -t {session}"))
            .await?;
    }
    let screen = muxa_tmux::snapshot(client, pane).await?;
    Ok(Synced { screen, session })
}
