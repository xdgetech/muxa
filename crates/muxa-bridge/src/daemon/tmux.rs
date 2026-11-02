//! The daemon's tmux client (ADR 0019): attached with
//! `-f ignore-size,no-output` only while there is work, so no pane bytes
//! reach the daemon and a plugin's `client-attached` hook runs once per
//! stretch of work. It never starts a tmux server.

use std::sync::Arc;
use std::time::Duration;

use muxa_tmux::{Client, Error, Flags, Options};
use tokio::sync::Mutex;
use tokio::time::Instant;

use crate::task_records::{ClientFuture, ClientSource};

/// Without work this long, the client detaches.
const IDLE: Duration = Duration::from_secs(60);
/// After a failed attach, the next try waits this long.
const RETRY: Duration = Duration::from_secs(10);
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Lazy {
    opts: Options,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    held: Option<Held>,
    failed_at: Option<Instant>,
}

struct Held {
    client: Arc<Client>,
    /// When work last held the client.
    used: Instant,
}

impl Lazy {
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            state: Mutex::new(State::default()),
        }
    }

    /// The attached client, attaching one when there is none. Holding the
    /// returned `Arc` counts as work and keeps it attached.
    pub async fn get(&self) -> Result<Arc<Client>, Error> {
        let mut state = self.state.lock().await;
        if let Some(held) = state.held.as_mut().filter(|h| !h.client.exited()) {
            held.used = Instant::now();
            return Ok(Arc::clone(&held.client));
        }
        state.held = None;
        if let Some(at) = state.failed_at.filter(|at| at.elapsed() < RETRY) {
            let wait = RETRY.saturating_sub(at.elapsed()).as_secs();
            return Err(Error::Exited(format!(
                "tmux is not reachable; the daemon looks again in {wait} s"
            )));
        }
        match attach(&self.opts).await {
            Ok(client) => {
                tracing::info!("attached to tmux");
                state.failed_at = None;
                state.held = Some(Held {
                    client: Arc::clone(&client),
                    used: Instant::now(),
                });
                Ok(client)
            }
            Err(e) => {
                tracing::info!(error = %e, "tmux is not reachable");
                state.failed_at = Some(Instant::now());
                Err(e)
            }
        }
    }

    /// Runs on every daemon tick. `busy` is work that holds no client
    /// between ticks, such as an active loop: it gets the client, attached
    /// if need be. Otherwise this drops a client that ended, detaches one
    /// that had no work for [`IDLE`], and returns None.
    pub async fn tick(&self, busy: bool) -> Option<Arc<Client>> {
        if busy {
            return match self.get().await {
                Ok(c) => Some(c),
                Err(e) => {
                    tracing::debug!(error = %e, "no tmux for this tick");
                    None
                }
            };
        }
        self.idle().await;
        None
    }

    async fn idle(&self) {
        let mut state = self.state.lock().await;
        let Some(held) = state.held.as_mut() else {
            return;
        };
        if held.client.exited() {
            tracing::info!(reason = %held.client.exit_reason(), "the tmux client ended");
            state.held = None;
            return;
        }
        // One count is ours; any other is a prompt wait or a request.
        if Arc::strong_count(&held.client) > 1 {
            held.used = Instant::now();
        } else if held.used.elapsed() >= IDLE {
            tracing::info!("no work; detaching from tmux");
            state.held = None;
        }
    }
}

impl ClientSource for Lazy {
    fn client(&self) -> ClientFuture<'_> {
        Box::pin(self.get())
    }
}

/// Attaches to tmux's most recent session and checks that the client
/// answers: without a server, `attach-session` starts and exits at once.
async fn attach(opts: &Options) -> Result<Arc<Client>, Error> {
    let (client, _notes) = Client::attach_with(opts, None, Flags { no_output: true })?;
    match tokio::time::timeout(CHECK_TIMEOUT, client.command("display-message -p ok")).await {
        Ok(Ok(_)) => Ok(Arc::new(client)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(Error::Exited("tmux did not answer".into())),
    }
}
