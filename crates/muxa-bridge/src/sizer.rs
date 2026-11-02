//! Sizes the watched window for the device in use (ADR 0020), through a
//! device session per window (ADR 0021). The watcher tells it what happened;
//! it decides when to take, resize or give back the window, and sends nothing
//! until the client has sent a viewport.

use std::time::{Duration, Instant};

use muxa_tmux::{Client, Device, Grid, Options, PaneId, Screen, SessionId, WindowId};

use crate::viewport::{Fit, Next, Seen};

/// The layout counts as settled after this long without a new snapshot.
const SETTLE: Duration = Duration::from_millis(300);
const SETTLE_MAX: Duration = Duration::from_secs(2);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

/// The watched pane and where it is, once its first snapshot landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub pane: PaneId,
    pub session: SessionId,
    pub window: WindowId,
}

pub struct Sizer {
    opts: Options,
    /// This process: the owner of its device sessions.
    owner: u32,
    opened: u32,
    viewport: Option<Grid>,
    /// The device session of the watched window, while it sizes it.
    device: Option<Device>,
    /// The size the device client holds while it is not `ignore-size`.
    held: Option<Grid>,
    target: Option<Target>,
    seen: Option<Seen>,
    fit: Option<Fit>,
    /// (settled at, at the latest)
    settle: Option<(Instant, Instant)>,
    begin_on_land: bool,
}

impl Sizer {
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            owner: std::process::id(),
            opened: 0,
            viewport: None,
            device: None,
            held: None,
            target: None,
            seen: None,
            fit: None,
            settle: None,
            begin_on_land: false,
        }
    }

    /// A new watch: the fit starts when its first snapshot lands.
    pub fn watching(&mut self) {
        self.target = None;
        self.seen = None;
        self.fit = None;
        self.settle = None;
        self.begin_on_land = true;
    }

    pub async fn set_viewport(&mut self, client: &Client, viewport: Grid) {
        self.viewport = Some(viewport);
        // Like a terminal resize, a new viewport makes this client latest.
        self.begin(client).await;
    }

    /// A snapshot of the watched pane landed.
    pub async fn landed(&mut self, client: &Client, target: Target, seen: Seen) {
        self.target = Some(target);
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
        let (Some(held), Some(target)) = (self.held, self.target) else {
            return;
        };
        if target.pane != pane {
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
    pub async fn tick(&mut self, syncing: bool) {
        let Some((at, latest)) = self.settle else {
            return;
        };
        let now = Instant::now();
        if now < at || (syncing && now < latest) {
            return;
        }
        self.settle = None;
        let (Some(fit), Some(seen), Some(device)) =
            (self.fit.as_mut(), self.seen, self.device.as_ref())
        else {
            return;
        };
        match fit.settled(seen) {
            Next::Resize(size) => {
                if let Err(e) = timed(device.resize(size)).await {
                    tracing::warn!(error = %e, "resizing the device client failed");
                    self.fit = None;
                    return;
                }
                self.held = Some(size);
                self.settle_from(now);
            }
            Next::Done | Next::Stop => {
                if let Some(target) = self.target {
                    tracing::info!(pane = %target.pane, window = ?seen.window, pane_size = ?seen.pane, "sized for the device");
                }
                self.fit = None;
            }
        }
    }

    /// Whether the user session that the device session is grouped with is
    /// gone. The device session then holds its windows open.
    pub async fn target_gone(&self, client: &Client) -> bool {
        let Some(device) = &self.device else {
            return false;
        };
        let line = format!("has-session -t {}", device.target());
        matches!(
            timed(client.command(&line)).await,
            Err(muxa_tmux::Error::Tmux { .. })
        )
    }

    /// The watch ended: the window goes back to the other clients, and the
    /// device session ends.
    pub async fn release(&mut self, client: &Client) {
        self.target = None;
        self.fit = None;
        self.settle = None;
        self.begin_on_land = false;
        self.close_device(client).await;
    }

    async fn close_device(&mut self, client: &Client) {
        let held = self.held.take();
        let Some(device) = self.device.take() else {
            return;
        };
        let (session, window) = (device.session(), device.window());
        match timed(device.close(client, held)).await {
            Ok(()) => tracing::info!(%session, %window, "device session closed"),
            Err(e) => tracing::warn!(%session, error = %e, "closing the device session failed"),
        }
    }

    async fn begin(&mut self, client: &Client) {
        let (Some(viewport), Some(target), Some(seen)) = (self.viewport, self.target, self.seen)
        else {
            return;
        };
        match timed(muxa_tmux::follows_device(client, target.pane)).await {
            Ok(true) => {}
            Ok(false) => {
                tracing::debug!(pane = %target.pane, "the window's window-size is not latest; not sizing it");
                self.close_device(client).await;
                self.fit = None;
                self.settle = None;
                return;
            }
            Err(e) => {
                tracing::warn!(pane = %target.pane, error = %e, "reading window-size failed");
                return;
            }
        }
        let Some(device) = self.device_for(client, target).await else {
            return;
        };
        let fit = Fit::start(viewport, seen);
        let size = fit.sent();
        if let Err(e) = timed(device.take(client, size)).await {
            tracing::warn!(pane = %target.pane, error = %e, "sizing the device client failed");
            return;
        }
        self.held = Some(size);
        self.fit = Some(fit);
        self.settle_from(Instant::now());
    }

    /// The device session for the target's window. A client stays latest of
    /// every window it was on, so another window gets a new one.
    async fn device_for(&mut self, client: &Client, target: Target) -> Option<&Device> {
        let fits = self.device.as_ref().is_some_and(|d| {
            !d.exited() && d.target() == target.session && d.window() == target.window
        });
        if !fits {
            self.close_device(client).await;
            remove_leftovers(client).await;
            self.opened = self.opened.wrapping_add(1);
            let opened = Device::open(
                client,
                &self.opts,
                target.session,
                target.window,
                self.owner,
                self.opened,
            );
            match timed(opened).await {
                Ok(device) => {
                    tracing::info!(session = %device.session(), target = %target.session, window = %target.window, "device session opened");
                    self.device = Some(device);
                }
                Err(e) => {
                    tracing::warn!(target = %target.session, window = %target.window, error = %e, "opening a device session failed");
                    return None;
                }
            }
        }
        self.device.as_ref()
    }

    fn settle_from(&mut self, now: Instant) {
        self.settle = Some((now + SETTLE, now + SETTLE_MAX));
    }
}

/// Removes device sessions that a bridge left behind: no client, and their
/// owner is not running, e.g. after a `kill -9`. Runs when a bridge starts and
/// before it opens a device session.
pub async fn remove_leftovers(client: &Client) {
    let leftovers = match timed(muxa_tmux::unattached_devices(client)).await {
        Ok(found) => found,
        Err(e) => {
            tracing::warn!(error = %e, "listing device sessions failed");
            return;
        }
    };
    for (session, owner) in leftovers {
        if owner.is_some_and(running) {
            continue;
        }
        match timed(client.command(&format!("kill-session -t {session}"))).await {
            Ok(_) => tracing::info!(%session, "removed a leftover device session"),
            Err(e) => {
                tracing::warn!(%session, error = %e, "removing a leftover device session failed");
            }
        }
    }
}

fn running(pid: u32) -> bool {
    let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    else {
        return false;
    };
    // EPERM still means it runs, as another user.
    !matches!(
        rustix::process::test_kill_process(pid),
        Err(rustix::io::Errno::SRCH)
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_runs_and_a_reaped_child_does_not() {
        assert!(running(std::process::id()));
        let mut child = std::process::Command::new("true").spawn().expect("spawn");
        let pid = child.id();
        child.wait().expect("wait");
        assert!(!running(pid));
        assert!(!running(0));
    }
}
