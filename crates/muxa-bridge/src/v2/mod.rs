//! Protocol v2 in the daemon (ADR 0067): `server.sock` beside `daemon.sock`,
//! served by `muxa-server` on herdr-core, and `muxa-bridge connect`, which
//! relays an SSH client's stdin and stdout to it.

pub mod chat;
pub mod feed;
mod git;
mod places;
pub mod reader;
mod routines;
pub mod runner;
pub mod task_loop;
mod tasks;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muxa_backend::SessionBackend;
use muxa_herdr::{Client, HerdrBackend};
use muxa_server::{Config, Host, Inbox, PARK, Services, Sessions};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;

use crate::daemon::socket;

pub const FILE: &str = "server.sock";

pub fn path(dir: &Path) -> Result<PathBuf, String> {
    let path = dir.join(FILE);
    if path.as_os_str().len() > socket::MAX_PATH {
        return Err(format!(
            "the socket path {} is too long for a Unix socket. Pass a shorter --daemon-dir",
            path.display()
        ));
    }
    Ok(path)
}

pub use tasks::BridgeHost;

/// One herdr-core connection, made on first use and again after it is lost,
/// the client sessions a resume returns to, and how Tasks start.
pub struct Herdr {
    program: String,
    current: Mutex<Option<HerdrBackend>>,
    sessions: Sessions,
    host: BridgeHost,
    inbox: Arc<Inbox>,
}

impl Herdr {
    /// `program` runs as `<program> connect`: `hc`, or a path to it.
    pub fn new(program: String, host: BridgeHost, inbox: Arc<Inbox>) -> Self {
        Herdr {
            program,
            current: Mutex::new(None),
            sessions: Sessions::new(),
            host,
            inbox,
        }
    }

    async fn backend(&self) -> Result<HerdrBackend, String> {
        let mut current = self.current.lock().await;
        if let Some(b) = current.as_ref().filter(|b| !b.client().is_closed()) {
            return Ok(b.clone());
        }
        let client = Client::spawn(&self.program, &["connect"])
            .await
            .map_err(|e| e.to_string())?;
        tracing::info!(version = client.version(), "connected to herdr-core");
        let backend = HerdrBackend::new(client);
        *current = Some(backend.clone());
        Ok(backend)
    }
}

/// `<state dir>/inbox.json` (ADR 0066), with its Later timer running. A file
/// that cannot be read leaves an Inbox in memory only, and says so in the log.
pub fn inbox(state_dir: Option<&Path>) -> Arc<Inbox> {
    let opened = match state_dir {
        Some(dir) => Inbox::open(dir.join("inbox.json")).unwrap_or_else(|e| {
            tracing::error!(error = %e, "the Inbox is in memory only: changes are not kept");
            Inbox::memory()
        }),
        None => {
            tracing::warn!("no state dir: the Inbox is in memory only");
            Inbox::memory()
        }
    };
    let inbox = Arc::new(opened);
    inbox.spawn_timer();
    inbox
}

/// Keeps a Failed item coming for each failing Task agent: watches
/// herdr-core, and connects again when it is lost or not running yet.
pub fn spawn_task_watch(herdr: Arc<Herdr>) {
    const FIRST: Duration = Duration::from_secs(2);
    const MOST: Duration = Duration::from_secs(60);
    tokio::spawn(async move {
        let mut wait = FIRST;
        loop {
            match herdr.backend().await {
                Ok(backend) => {
                    wait = FIRST;
                    if let Err(e) = muxa_server::watch_tasks(&backend, &herdr.inbox).await {
                        tracing::info!(error = %e, "the Task watch ended");
                    }
                }
                Err(e) => tracing::info!(error = %e, "no herdr-core for the Task watch yet"),
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(MOST);
        }
    });
}

/// Runs `watch` on herdr-core again each time it ends, waiting longer while
/// herdr-core is not there.
fn keep_watching<F, Fut>(herdr: Arc<Herdr>, what: &'static str, watch: F)
where
    F: Fn(HerdrBackend) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), String>> + Send,
{
    const FIRST: Duration = Duration::from_secs(2);
    const MOST: Duration = Duration::from_secs(60);
    tokio::spawn(async move {
        let mut wait = FIRST;
        loop {
            match herdr.backend().await {
                Ok(backend) => {
                    wait = FIRST;
                    if let Err(e) = watch(backend).await {
                        tracing::info!(error = %e, what, "a herdr-core watch ended");
                    }
                }
                Err(e) => tracing::info!(error = %e, what, "no herdr-core yet"),
            }
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(MOST);
        }
    });
}

/// Runs Task Loops on herdr-core.
pub fn spawn_loops(herdr: Arc<Herdr>, loops: Arc<task_loop::TaskLoops>) {
    keep_watching(herdr, "task loops", move |backend| {
        let loops = Arc::clone(&loops);
        async move { loops.watch(&backend).await }
    });
}

/// Runs Routines on herdr-core: a watch follows the run terminals, and every
/// [`RUNNER_TICK`] due runs start and lost ones close.
pub fn spawn_runner(herdr: Arc<Herdr>, runner: Arc<runner::Runner>) {
    let r = Arc::clone(&runner);
    keep_watching(Arc::clone(&herdr), "routine runs", move |backend| {
        let r = Arc::clone(&r);
        async move { r.watch(&backend).await }
    });
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(RUNNER_TICK);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            if let Ok(backend) = herdr.backend().await {
                runner.tick(&backend).await;
            }
        }
    });
}

/// How often due Routine runs start.
const RUNNER_TICK: Duration = Duration::from_secs(30);

/// Accepts v2 clients until the listener fails.
pub async fn accept(listener: UnixListener, herdr: Arc<Herdr>) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let herdr = Arc::clone(&herdr);
                tokio::spawn(async move {
                    if !socket::peer_allowed(&stream) {
                        return;
                    }
                    let backend = match herdr.backend().await {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!(error = %e, "no herdr-core for a v2 client");
                            return;
                        }
                    };
                    let name = format!("herdr-core {}", backend.client().version());
                    let services = Services {
                        backend: &backend,
                        host: &herdr.host,
                        sessions: &herdr.sessions,
                        inbox: &herdr.inbox,
                    };
                    serve(stream, &services, name).await;
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "cannot accept a v2 connection");
                return;
            }
        }
    }
}

/// Serves one v2 client on `stream`.
pub async fn serve<B: SessionBackend, H: Host>(
    stream: UnixStream,
    services: &Services<'_, B, H>,
    backend_name: String,
) {
    let (r, w) = stream.into_split();
    let config = Config {
        server: format!("muxa {}", env!("CARGO_PKG_VERSION")),
        backend: backend_name,
        park: PARK,
    };
    match muxa_server::serve(services, r, w, &config).await {
        Ok(()) => tracing::info!("a v2 client left"),
        Err(e) => tracing::info!(error = %e, "a v2 client ended"),
    }
}

/// `muxa-bridge connect`: relays stdin and stdout to the server's socket.
pub async fn connect(dir: &Path) -> Result<(), String> {
    relay(tokio::io::stdin(), tokio::io::stdout(), &path(dir)?).await
}

/// Copies `input` to the socket and the socket to `output` until either side
/// ends. It never looks at the bytes.
pub async fn relay<R, W>(mut input: R, mut output: W, socket: &Path) -> Result<(), String>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let stream = UnixStream::connect(socket).await.map_err(|e| {
        format!(
            "cannot reach the muxa server at {}: {e}. Is the daemon running?",
            socket.display()
        )
    })?;
    let (mut from_server, mut to_server) = stream.into_split();
    let up = async {
        let r = tokio::io::copy(&mut input, &mut to_server).await;
        let _ = to_server.shutdown().await;
        r
    };
    let down = async {
        let r = tokio::io::copy(&mut from_server, &mut output).await;
        let _ = output.flush().await;
        r
    };
    tokio::select! {
        r = up => r.map(drop).map_err(|e| format!("relay to the server: {e}")),
        r = down => r.map(drop).map_err(|e| format!("relay from the server: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use muxa_backend::fake::FakeBackend;
    use muxa_protocol::v2::{Frame, Kind};
    use serde_json::{Value, json};
    use tokio::io::AsyncReadExt;

    use super::*;

    fn no_tasks() -> BridgeHost {
        let records = crate::config::Config {
            state_dir: None,
            workspaces: None,
            home: None,
        };
        BridgeHost::new("true".into(), Arc::new(records))
    }

    fn short_dir(name: &str) -> PathBuf {
        let dir = PathBuf::from(format!("/tmp/muxa-v2-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn a_relayed_client_gets_a_welcome_from_the_server() {
        let dir = short_dir("relay");
        let sock = path(&dir).unwrap();
        let listener = UnixListener::bind(&sock).unwrap();
        let backend = FakeBackend::new();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (host, sessions, inbox) = (no_tasks(), Sessions::new(), Inbox::memory());
            let services = Services {
                backend: &backend,
                host: &host,
                sessions: &sessions,
                inbox: &inbox,
            };
            serve(stream, &services, "fake".into()).await;
        });

        // The client's side of SSH: what it writes, and what it reads back.
        let (mut client, relay_end) = tokio::io::duplex(1 << 16);
        let (relay_in, relay_out) = tokio::io::split(relay_end);
        let relayed = tokio::spawn(async move { relay(relay_in, relay_out, &sock).await });

        let hello = json!({"type": "hello", "version": 2, "client": "test", "resume": null});
        let frame = Frame::new(0, Kind::Data, hello.to_string().into_bytes());
        client.write_all(&frame.encode().unwrap()).await.unwrap();
        let mut buf = Vec::new();
        let welcome = loop {
            if let Some(f) = Frame::decode(&mut buf).unwrap() {
                break f;
            }
            let mut chunk = [0u8; 4096];
            let n = client.read(&mut chunk).await.unwrap();
            assert!(n > 0, "the relay closed");
            buf.extend_from_slice(&chunk[..n]);
        };
        let msg: Value = serde_json::from_slice(&welcome.payload).unwrap();
        assert_eq!(
            (msg["type"].as_str(), msg["backend"].as_str()),
            (Some("welcome"), Some("fake"))
        );

        drop(client);
        relayed.await.unwrap().unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(feature = "herdr-integration")]
    #[tokio::test]
    async fn the_v2_socket_lists_herdr_core_terminals() {
        if std::env::var("MUXA_TEST_HERDR").as_deref() != Ok("1") {
            eprintln!("SKIP  set MUXA_TEST_HERDR=1 to use the running herdr-core");
            return;
        }
        let dir = short_dir("herdr");
        let sock = path(&dir).unwrap();
        let listener = UnixListener::bind(&sock).unwrap();
        tokio::spawn(accept(
            listener,
            Arc::new(Herdr::new(
                "hc".into(),
                no_tasks(),
                Arc::new(Inbox::memory()),
            )),
        ));
        let mut stream = UnixStream::connect(&sock).await.unwrap();
        for msg in [
            json!({"type": "hello", "version": 2, "client": "test", "resume": null}),
            json!({"type": "terminals", "id": 1}),
        ] {
            let frame = Frame::new(0, Kind::Data, msg.to_string().into_bytes());
            stream.write_all(&frame.encode().unwrap()).await.unwrap();
        }
        let mut buf = Vec::new();
        let mut msgs = Vec::new();
        while msgs.len() < 2 {
            if let Some(f) = Frame::decode(&mut buf).unwrap() {
                msgs.push(serde_json::from_slice::<Value>(&f.payload).unwrap());
                continue;
            }
            let mut chunk = [0u8; 65536];
            let n = stream.read(&mut chunk).await.unwrap();
            assert!(n > 0, "the server closed");
            buf.extend_from_slice(&chunk[..n]);
        }
        assert!(
            msgs[0]["backend"]
                .as_str()
                .unwrap()
                .starts_with("herdr-core ")
        );
        assert!(msgs[1]["terminals"].is_array(), "{}", msgs[1]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[tokio::test]
    async fn connect_without_a_server_says_what_to_check() {
        let dir = short_dir("none");
        let (r, w) = tokio::io::duplex(64);
        let err = relay(r, w, &path(&dir).unwrap()).await.unwrap_err();
        assert!(err.contains("Is the daemon running?"), "{err}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
