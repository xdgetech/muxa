//! The host daemon (ADR 0019): one long-lived process per user that serves
//! task records on a Unix socket, so work goes on while no client is
//! connected. It opens no network port. `daemon install` and friends manage
//! the LaunchAgent or systemd unit that starts it.

pub mod install;
mod lock;
mod log;
pub mod socket;
mod tmux;

use std::ffi::OsStr;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::{ErrorCode, Message, Request};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

use crate::Setup;
use crate::args::{self, DaemonOp};
use crate::convert::error_body;
use crate::loops;
use crate::server::MAX_REQUEST;
use crate::task_records::{self, Context, Pending};
use install::{Platform, Runner, Spec};
use socket::Control;

/// What `daemon stop` waits for the daemon to answer `shutdown`.
const SHUTDOWN_WAIT: Duration = Duration::from_secs(10);
const CONNECT_WAIT: Duration = Duration::from_millis(500);
const TICK: Duration = Duration::from_secs(2);
/// What `install` and `restart` wait for the new daemon to answer `hello`.
const START_WAIT: Duration = Duration::from_secs(5);
const START_POLL: Duration = Duration::from_millis(100);

/// `$XDG_STATE_HOME/muxa`, else `~/.local/state/muxa`: the parent of `tasks/`.
pub fn default_dir(xdg_state_home: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    crate::records::Store::default_dir(xdg_state_home, home)
        .and_then(|tasks| tasks.parent().map(Path::to_path_buf))
}

pub fn main(cmd: args::Daemon) -> ExitCode {
    let out = &mut std::io::stdout().lock();
    let result = match cmd.op {
        DaemonOp::Run => return run(cmd.args),
        DaemonOp::Install => install(cmd.args, cmd.print, out),
        DaemonOp::Stop => stop(cmd.args, false, out),
        DaemonOp::Uninstall => stop(cmd.args, true, out),
        DaemonOp::Restart => restart(cmd.args, out),
        DaemonOp::Status => status(cmd.args, out),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("muxa-bridge daemon: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The daemon itself.
struct Daemon {
    records: Context,
    /// The same client `records` gets its tmux from.
    tmux: Arc<tmux::Lazy>,
    loops: tokio::sync::Mutex<loops::Engine>,
}

impl Daemon {
    /// Called once before the daemon exits for the user's `daemon stop`:
    /// every loop stops with reason `user`, so the next daemon does not resume them.
    async fn on_user_stop(&self) {
        let config = Arc::clone(&self.records.records);
        match tokio::task::spawn_blocking(move || loops::stop_all(&config)).await {
            Ok(Ok(stopped)) => tracing::info!(loops = stopped.len(), "loops stopped by the user"),
            Ok(Err(e)) => tracing::warn!(error = %e, "cannot stop the loops"),
            Err(e) => tracing::warn!(error = %e, "stopping the loops failed"),
        }
        self.loops.lock().await.stop_checks().await;
    }

    /// Runs every [`TICK`]. The tmux client stays attached while a loop is active.
    async fn tick(&self) {
        let mut engine = self.loops.lock().await;
        let client = self.tmux.tick(engine.busy()).await;
        engine.tick(client.as_deref()).await;
        drop(engine);
        crate::routines::tick(&self.records).await;
    }
}

fn run(args: args::Args) -> ExitCode {
    let setup = match Setup::new(args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("muxa-bridge daemon: {e}");
            return ExitCode::from(2);
        }
    };
    let Some(dir) = setup.daemon_dir.clone() else {
        eprintln!("muxa-bridge daemon: HOME is not set; pass --daemon-dir");
        return ExitCode::from(2);
    };
    let path = match prepare(&dir) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("muxa-bridge daemon: {e}");
            return ExitCode::from(2);
        }
    };
    let log = match log::Rotating::open(&dir) {
        Ok(l) => l,
        Err(e) => {
            eprintln!(
                "muxa-bridge daemon: cannot open {}: {e}",
                dir.join(log::FILE).display()
            );
            return ExitCode::FAILURE;
        }
    };
    tracing_subscriber::fmt()
        .with_writer(log)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("MUXA_LOG")
                .unwrap_or_else(|_| "info".into()),
        )
        .init();
    let _lock = match lock::acquire(&dir) {
        Ok(Some(l)) => l,
        // Exit 0, so the service manager does not restart this one.
        Ok(None) => {
            tracing::info!(dir = %dir.display(), "another daemon runs; exiting");
            eprintln!(
                "muxa-bridge daemon: another daemon runs in {}",
                dir.display()
            );
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            tracing::error!(error = %e, "cannot take the daemon lock");
            eprintln!("muxa-bridge daemon: cannot take the lock: {e}");
            return ExitCode::FAILURE;
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(error = %e, "cannot start the runtime");
            return ExitCode::FAILURE;
        }
    };
    let tmux = Arc::new(tmux::Lazy::new(setup.tmux_options()));
    // Install wrote the shell's PATH into the service: a check's fallback.
    let engine = loops::Engine::new(
        Arc::clone(&setup.records),
        Arc::clone(&setup.agents),
        std::env::var("PATH").ok(),
    );
    let daemon = Daemon {
        records: Context {
            tmux: Arc::clone(&tmux) as Arc<dyn task_records::ClientSource>,
            tasks: setup.tasks,
            records: setup.records,
            agents: setup.agents,
            pending: Pending::default(),
        },
        loops: tokio::sync::Mutex::new(engine),
        tmux,
    };
    match runtime.block_on(serve(Arc::new(daemon), &path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "the daemon failed");
            ExitCode::FAILURE
        }
    }
}

/// Creates the daemon dir with mode 0700 and checks the socket path.
fn prepare(dir: &Path) -> Result<PathBuf, String> {
    let path = socket::path(dir)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("cannot set the mode of {}: {e}", dir.display()))?;
    Ok(path)
}

async fn serve(daemon: Arc<Daemon>, path: &Path) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let listener = socket::bind(path)?;
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    let (stop_tx, mut stop_rx) = mpsc::channel::<()>(1);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), socket = %path.display(), "daemon started");
    let resume = Arc::clone(&daemon);
    tokio::spawn(async move { task_records::resume_prompts(&resume.records).await });
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = tick.tick() => daemon.tick().await,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    tokio::spawn(connection(Arc::clone(&daemon), stream, stop_tx.clone()));
                }
                Err(e) => tracing::warn!(error = %e, "cannot accept a connection"),
            },
            _ = stop_rx.recv() => {
                tracing::info!("stopped by the user");
                break;
            }
            // Not the user's stop: loops stay as they are, and the next daemon resumes them.
            _ = term.recv() => {
                tracing::info!("terminated");
                break;
            }
            _ = int.recv() => {
                tracing::info!("interrupted");
                break;
            }
        }
    }
    // A check's process group does not outlive a clean exit (ADR 0019).
    daemon.loops.lock().await.stop_checks().await;
    if let Err(e) = std::fs::remove_file(path) {
        tracing::warn!(error = %e, "cannot remove the socket");
    }
    Ok(())
}

fn hello() -> Message {
    Message::Hello {
        protocol: muxa_protocol::VERSION,
        bridge: env!("CARGO_PKG_VERSION").into(),
        tmux: String::new(),
        capabilities: vec![
            "task-records".into(),
            "task-loop".into(),
            "task-restart-recovery".into(),
            muxa_protocol::work_capability(muxa_protocol::WORK_ROUTINES),
        ],
    }
}

/// One client: `hello`, then requests until it closes.
async fn connection(daemon: Arc<Daemon>, stream: UnixStream, stop: mpsc::Sender<()>) {
    if !socket::peer_allowed(&stream) {
        tracing::warn!("refused a connection from another user");
        return;
    }
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    if socket::write(&mut write, &hello()).await.is_err() {
        return;
    }
    loop {
        let line = match socket::read::<serde_json::Value, _>(&mut read, MAX_REQUEST).await {
            Ok(Some(v)) => v,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!(error = %e, "unreadable request on the socket");
                return;
            }
        };
        let answer = answer(&daemon, line).await;
        let shutdown = answer.shutdown;
        for msg in answer.messages {
            if socket::write(&mut write, &msg).await.is_err() {
                return;
            }
        }
        if shutdown {
            let _ = stop.send(()).await;
            return;
        }
    }
}

struct Answer {
    messages: Vec<Message>,
    shutdown: bool,
}

async fn answer(daemon: &Daemon, line: serde_json::Value) -> Answer {
    let id = line.get("id").and_then(serde_json::Value::as_u64);
    if let Ok(Control::Shutdown { id }) = serde_json::from_value::<Control>(line.clone()) {
        daemon.on_user_stop().await;
        return Answer {
            messages: vec![ok(id)],
            shutdown: true,
        };
    }
    let messages = match serde_json::from_value::<Request>(line) {
        Ok(req) if task_records::is_record_request(&req) => {
            tracing::info!(id = req.id(), "record request");
            task_records::handle(&daemon.records, req).await
        }
        Ok(req) => vec![failure(
            Some(req.id()),
            ErrorCode::Unsupported,
            "the muxa daemon serves task records only",
        )],
        Err(e) => vec![failure(id, ErrorCode::BadRequest, &e.to_string())],
    };
    Answer {
        messages,
        shutdown: false,
    }
}

fn ok(id: u64) -> Message {
    Message::Result {
        id: Some(id),
        error: None,
        sessions: None,
    }
}

fn failure(id: Option<u64>, code: ErrorCode, message: &str) -> Message {
    Message::Result {
        id,
        error: Some(error_body(code, message)),
        sessions: None,
    }
}

/// The daemon dir and `HOME`, which every management command needs.
fn locate(args: &args::Args) -> Result<(PathBuf, PathBuf), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("HOME is not set")?;
    let dir = args
        .daemon_dir
        .clone()
        .or_else(|| default_dir(std::env::var_os("XDG_STATE_HOME").as_deref(), Some(&home)))
        .ok_or("pass --daemon-dir")?;
    Ok((dir, home))
}

/// Writes to stdout. A reader that closed it, as `| head -1` does, is not
/// an error: the rest of the output is dropped.
fn say(out: &mut dyn Write, text: std::fmt::Arguments) -> Result<(), String> {
    match out.write_fmt(text).and_then(|()| out.flush()) {
        Err(e) if e.kind() != ErrorKind::BrokenPipe => Err(format!("cannot write: {e}")),
        _ => Ok(()),
    }
}

fn install(args: args::Args, print: bool, out: &mut dyn Write) -> Result<(), String> {
    let (dir, home) = locate(&args)?;
    socket::path(&dir)?;
    let path = std::env::var("PATH").ok();
    let tmux = args::find_tmux(args.tmux.clone(), path.as_deref())
        .ok_or("tmux not found; pass --tmux PATH")?;
    let tmux = std::path::absolute(&tmux)
        .map_err(|e| format!("cannot resolve {}: {e}", tmux.display()))?;
    let binary = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    let spec = Spec {
        binary,
        tmux,
        flags: args.service_flags(),
        daemon_dir: dir.clone(),
        env: install::COPIED_ENV
            .iter()
            .filter_map(|k| Some(((*k).to_owned(), std::env::var(k).ok()?)))
            .collect(),
    };
    let platform = Platform::current();
    let text = platform.render(&spec);
    if print {
        return say(out, format_args!("{text}"));
    }
    prepare(&dir)?;
    let file = platform.file(&home);
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::write(&file, text).map_err(|e| format!("cannot write {}: {e}", file.display()))?;
    platform.load(&install::System, &file)?;
    say(out, format_args!("Installed {}.\n", file.display()))?;
    if let Some(hint) = platform.hint() {
        say(out, format_args!("{hint}\n"))?;
    }
    report_start(&dir, out)
}

/// `stop`, and with `remove`, `uninstall`: the loops stop for the user,
/// then the service is unloaded.
fn stop(args: args::Args, remove: bool, out: &mut dyn Write) -> Result<(), String> {
    let (dir, home) = locate(&args)?;
    let answered = block_on(shutdown(&dir))??;
    if !answered {
        say(out, format_args!("The daemon was not running.\n"))?;
    }
    let platform = Platform::current();
    let file = platform.file(&home);
    let run: &dyn Runner = &install::System;
    if remove {
        platform.disable(run)?;
        match std::fs::remove_file(&file) {
            Ok(()) => say(
                out,
                format_args!("Removed {}. Task records stay.\n", file.display()),
            )?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot remove {}: {e}", file.display())),
        }
    } else {
        platform.unload(run)?;
        say(
            out,
            format_args!("Stopped. The daemon starts again at the next login.\n"),
        )?;
    }
    Ok(())
}

fn restart(args: args::Args, out: &mut dyn Write) -> Result<(), String> {
    let (dir, home) = locate(&args)?;
    let platform = Platform::current();
    let file = platform.file(&home);
    if !file.is_file() {
        return Err(format!(
            "{} does not exist. Run: muxa-bridge daemon install",
            file.display()
        ));
    }
    platform.restart(&install::System, &file)?;
    report_start(&dir, out)
}

/// Waits until the new daemon answers with this binary's version, so a
/// `status` right after sees it running.
fn report_start(dir: &Path, out: &mut dyn Write) -> Result<(), String> {
    let expected = env!("CARGO_PKG_VERSION");
    match block_on(wait_for_version(dir, expected, START_WAIT))? {
        Some(v) if v == expected => say(out, format_args!("The daemon is running, version {v}.\n")),
        found => {
            let answer = found.map_or("no answer".to_owned(), |v| format!("version {v}"));
            Err(format!(
                "the daemon did not start with version {expected} within {}s ({answer}). See {}",
                START_WAIT.as_secs(),
                dir.join(log::FILE).display()
            ))
        }
    }
}

/// Polls `hello` until it reports `expected` or `wait` ends. Returns the
/// last version seen, if any.
async fn wait_for_version(dir: &Path, expected: &str, wait: Duration) -> Option<String> {
    let deadline = tokio::time::Instant::now() + wait;
    let mut seen = None;
    loop {
        if let Some(v) = version(dir).await {
            if v == expected {
                return Some(v);
            }
            seen = Some(v);
        }
        if tokio::time::Instant::now() + START_POLL > deadline {
            return seen;
        }
        tokio::time::sleep(START_POLL).await;
    }
}

fn status(args: args::Args, out: &mut dyn Write) -> Result<(), String> {
    let (dir, _) = locate(&args)?;
    let socket = dir.join(socket::FILE);
    match block_on(version(&dir))? {
        Some(v) => say(out, format_args!("running, version {v}\n"))?,
        None => say(out, format_args!("not running\n"))?,
    }
    let platform = Platform::current();
    if matches!(platform, Platform::Launchd { .. }) && !platform.loaded(&install::System) {
        say(
            out,
            format_args!("not loaded: the LaunchAgent is not loaded in this GUI session\n"),
        )?;
    }
    say(out, format_args!("socket: {}\n", socket.display()))?;
    say(
        out,
        format_args!("log: {}\n", dir.join(log::FILE).display()),
    )
}

fn block_on<T>(f: impl Future<Output = T>) -> Result<T, String> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start: {e}"))?;
    Ok(rt.block_on(f))
}

async fn open(
    dir: &Path,
) -> Option<(
    BufReader<tokio::net::unix::OwnedReadHalf>,
    tokio::net::unix::OwnedWriteHalf,
    Message,
)> {
    let stream = tokio::time::timeout(CONNECT_WAIT, UnixStream::connect(dir.join(socket::FILE)))
        .await
        .ok()?
        .ok()?;
    let (read, write) = stream.into_split();
    let mut read = BufReader::new(read);
    let hello = tokio::time::timeout(
        CONNECT_WAIT,
        socket::read::<Message, _>(&mut read, socket::MAX_LINE),
    )
    .await
    .ok()?
    .ok()??;
    Some((read, write, hello))
}

/// The running daemon's version.
async fn version(dir: &Path) -> Option<String> {
    match open(dir).await? {
        (_, _, Message::Hello { bridge, .. }) => Some(bridge),
        _ => None,
    }
}

/// Sends `shutdown` and waits for its answer. False when no daemon runs.
async fn shutdown(dir: &Path) -> Result<bool, String> {
    let Some((mut read, mut write, _)) = open(dir).await else {
        return Ok(false);
    };
    socket::write(&mut write, &Control::Shutdown { id: 1 })
        .await
        .map_err(|e| format!("cannot send shutdown: {e}"))?;
    match tokio::time::timeout(
        SHUTDOWN_WAIT,
        socket::read::<Message, _>(&mut read, socket::MAX_LINE),
    )
    .await
    {
        Ok(Ok(Some(Message::Result { error: None, .. }))) => Ok(true),
        _ => Err("the daemon did not confirm the shutdown".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stdout whose reader has gone, as with `| head -1`.
    struct Closed(ErrorKind);

    impl Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(self.0.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_stdout_is_not_an_error() {
        let out = &mut Closed(ErrorKind::BrokenPipe);
        assert_eq!(say(out, format_args!("running\n")), Ok(()));
    }

    #[test]
    fn other_write_errors_are_reported() {
        let out = &mut Closed(ErrorKind::PermissionDenied);
        assert!(say(out, format_args!("running\n")).is_err());
    }

    #[test]
    fn waiting_for_a_daemon_that_never_answers_ends() {
        let dir = std::env::temp_dir().join(format!("muxa-wait-{}", std::process::id()));
        let started = std::time::Instant::now();
        let seen = block_on(wait_for_version(&dir, "0.0.0", Duration::from_millis(300)));
        assert_eq!(seen, Ok(None));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn waiting_returns_once_the_daemon_answers_hello() {
        let dir = std::env::temp_dir().join(format!("muxa-hello-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let seen = block_on(async {
            let listener = tokio::net::UnixListener::bind(dir.join(socket::FILE)).unwrap();
            tokio::spawn(async move {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let _ = socket::write(&mut stream, &hello()).await;
                }
            });
            wait_for_version(&dir, env!("CARGO_PKG_VERSION"), START_WAIT).await
        });
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(seen, Ok(Some(env!("CARGO_PKG_VERSION").to_owned())));
    }
}
