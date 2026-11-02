//! Sizes the bridge's tmux client for the device in use (ADR 0020). The
//! watcher tells it what happened; it decides when to take, resize or give
//! back the window, and sends nothing until the client has sent a viewport.

use std::time::{Duration, Instant};

use muxa_tmux::{Client, Grid, PaneId, Screen, SessionId};

use crate::viewport::{Fit, Next, Seen};

/// The layout counts as settled after this long without a new snapshot.
const SETTLE: Duration = Duration::from_millis(300);
const SETTLE_MAX: Duration = Duration::from_secs(2);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct Sizer {
    viewport: Option<Grid>,
    /// The size the tmux client holds while it is not `ignore-size`.
    held: Option<Grid>,
    /// The watched pane and its session, once its first snapshot landed.
    target: Option<(PaneId, SessionId)>,
    seen: Option<Seen>,
    fit: Option<Fit>,
    /// (settled at, at the latest)
    settle: Option<(Instant, Instant)>,
    begin_on_land: bool,
}

impl Sizer {
    /// The size to give back before the client moves to another session.
    pub fn held(&self) -> Option<Grid> {
        self.held
    }

    /// A new watch: the fit starts when its first snapshot lands.
    pub fn watching(&mut self) {
        self.target = None;
        self.seen = None;
        self.fit = None;
        self.settle = None;
        self.begin_on_land = true;
    }

    /// The client moved to another session after giving the size back.
    pub fn released(&mut self) {
        self.held = None;
    }

    pub async fn set_viewport(&mut self, client: &Client, viewport: Grid) {
        self.viewport = Some(viewport);
        // Like a terminal resize, a new viewport makes this client latest.
        self.begin(client).await;
    }

    /// A snapshot of the watched pane landed.
    pub async fn landed(&mut self, client: &Client, pane: PaneId, session: SessionId, seen: Seen) {
        self.target = Some((pane, session));
        self.seen = Some(seen);
        if let Some((_, latest)) = self.settle {
            self.settle = Some(((Instant::now() + SETTLE).min(latest), latest));
        }
        if std::mem::take(&mut self.begin_on_land) {
            self.begin(client).await;
        }
    }

    /// The client typed into `pane`. When another client took the window,
    /// this one takes it back, as a key press does for a terminal.
    pub async fn typed(&mut self, client: &Client, pane: PaneId) {
        let (Some(held), Some((target, _))) = (self.held, self.target) else {
            return;
        };
        if target != pane {
            return;
        }
        // Asked, not taken from the last snapshot: the other client's resize
        // may not have reached this client's stream yet.
        match timed(muxa_tmux::window_size(client, pane)).await {
            Ok(window) if window != held => {
                if let Some(seen) = self.seen.as_mut() {
                    // The pane's share is unknown until the next snapshot.
                    seen.pane = scaled(seen.pane, seen.window, window);
                    seen.window = window;
                }
                self.begin(client).await;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(%pane, error = %e, "reading the window size failed"),
        }
    }

    /// Called on every tick; `syncing` while a snapshot is in flight.
    pub async fn tick(&mut self, client: &Client, syncing: bool) {
        let Some((at, latest)) = self.settle else {
            return;
        };
        let now = Instant::now();
        if now < at || (syncing && now < latest) {
            return;
        }
        self.settle = None;
        let (Some(fit), Some(seen)) = (self.fit.as_mut(), self.seen) else {
            return;
        };
        match fit.settled(seen) {
            Next::Resize(size) => {
                if let Err(e) = timed(muxa_tmux::resize_client(client, size)).await {
                    tracing::warn!(error = %e, "resizing the client failed");
                    self.fit = None;
                    return;
                }
                self.held = Some(size);
                self.settle_from(now);
            }
            Next::Done | Next::Stop => {
                if let Some((pane, _)) = self.target {
                    tracing::info!(%pane, window = ?seen.window, pane_size = ?seen.pane, "sized for the device");
                }
                self.fit = None;
            }
        }
    }

    /// The watch ended: the windows go back to the other clients.
    pub async fn release(&mut self, client: &Client) {
        self.target = None;
        self.fit = None;
        self.settle = None;
        self.begin_on_land = false;
        let Some(held) = self.held.take() else {
            return;
        };
        if let Err(e) = timed(muxa_tmux::release_size(client, held)).await {
            tracing::warn!(error = %e, "giving the window size back failed");
        }
    }

    async fn begin(&mut self, client: &Client) {
        let (Some(viewport), Some((pane, session)), Some(seen)) =
            (self.viewport, self.target, self.seen)
        else {
            return;
        };
        match timed(muxa_tmux::follows_device(client, pane)).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::debug!(%pane, "the window's window-size is not latest; not sizing it");
                self.release(client).await;
                self.target = Some((pane, session));
                return;
            }
            Err(e) => {
                tracing::warn!(%pane, error = %e, "reading window-size failed");
                return;
            }
        }
        let fit = Fit::start(viewport, seen);
        let size = fit.sent();
        if let Err(e) = timed(muxa_tmux::take_size(client, session, size)).await {
            tracing::warn!(%pane, error = %e, "sizing the client failed");
            return;
        }
        let now = Instant::now();
        self.held = Some(size);
        self.fit = Some(fit);
        self.settle_from(now);
    }

    fn settle_from(&mut self, now: Instant) {
        self.settle = Some((now + SETTLE, now + SETTLE_MAX));
    }
}

/// The pane's size scaled with its window, for a first guess.
fn scaled(pane: Grid, from: Grid, to: Grid) -> Grid {
    let dim = |p: u32, f: u32, t: u32| {
        if f == 0 {
            p
        } else {
            (u64::from(p) * u64::from(t) / u64::from(f)) as u32
        }
    };
    Grid {
        cols: dim(pane.cols, from.cols, to.cols),
        rows: dim(pane.rows, from.rows, to.rows),
    }
}

/// The pane and its window, as a snapshot saw them.
pub fn seen(screen: &Screen) -> Seen {
    Seen {
        window: grid(screen.window_cols, screen.window_rows),
        pane: grid(screen.cols, screen.rows),
    }
}

fn grid(cols: u16, rows: u16) -> Grid {
    Grid {
        cols: u32::from(cols),
        rows: u32::from(rows),
    }
}

async fn timed<T>(
    f: impl Future<Output = Result<T, muxa_tmux::Error>>,
) -> Result<T, muxa_tmux::Error> {
    tokio::time::timeout(COMMAND_TIMEOUT, f)
        .await
        .unwrap_or_else(|_| {
            Err(muxa_tmux::Error::Exited(
                "tmux did not answer in time".into(),
            ))
        })
}
