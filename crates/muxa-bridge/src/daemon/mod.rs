//! The host daemon: one long-lived process per user that serves protocol v2
//! on `server.sock` over herdr-core (ADR 0067), so Tasks, Routines and Task
//! Loops go on while no client is connected. It opens no network port.
//! `daemon install` and friends manage the LaunchAgent or systemd unit.

pub mod install;
mod lock;
mod log;
pub mod socket;

use std::ffi::OsStr;
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use muxa_protocol::v2::{ClientMsg, Frame, Kind, ServerMsg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

use crate::Setup;
use crate::args::{self, DaemonOp};
use install::{Platform, Runner, Spec};

const CONNECT_WAIT: Duration = Duration::from_millis(500);
/// What `install` and `restart` wait for the new daemon to answer `hello`.
const START_WAIT: Duration = Duration::from_secs(5);
const START_POLL: Duration = Duration::from_millis(100);

/// `$XDG_STATE_HOME/muxa`, else `~/.local/state/muxa`.
pub fn default_dir(xdg_state_home: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
    let base = xdg_state_home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| h.join(".local/state")))?;
    Some(base.join("muxa"))
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

fn run(args: args::Args) -> ExitCode {
    let setup = Setup::new(args);
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
    match runtime.block_on(serve(setup, &path)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(error = %e, "the daemon failed");
            ExitCode::FAILURE
        }
    }
}

/// Serves `server.sock` and runs the watches until SIGTERM or SIGINT.
async fn serve(setup: Setup, path: &Path) -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};
    let listener = socket::bind(path)?;
    let program = std::env::var("MUXA_HC").unwrap_or_else(|_| "hc".to_owned());
    let config = Arc::clone(&setup.config);
    let inbox = crate::v2::inbox(config.state_dir.as_deref());
    let runner = Arc::new(crate::v2::runner::Runner::new(
        Arc::clone(&config),
        setup.task_command.clone(),
    ));
    let loops = Arc::new(crate::v2::task_loop::TaskLoops::new(
        Arc::clone(&config),
        Arc::clone(&inbox),
    ));
    let chats = Arc::new(crate::v2::chat::Chats::new(Arc::clone(&setup.agents)));
    let host = crate::v2::BridgeHost::new(setup.task_command, Arc::clone(&config))
        .with_claude_dirs(setup.claude_dirs)
        .with_runner(Arc::clone(&runner))
        .with_loops(Arc::clone(&loops))
        .with_chats(chats)
        .with_settings(Arc::new(crate::settings::SettingsStore::new(
            config.workspaces.clone(),
            config.home.clone(),
            setup.settings_read_only,
            Box::new(crate::settings::secrets::Keychain),
        )));
    crate::v2::feed::spawn(Arc::clone(&inbox), Arc::clone(&config));
    let herdr = Arc::new(crate::v2::Herdr::new(program, host, inbox));
    crate::v2::spawn_task_watch(Arc::clone(&herdr));
    crate::v2::spawn_runner(Arc::clone(&herdr), runner);
    crate::v2::spawn_loops(Arc::clone(&herdr), loops);
    tokio::spawn(crate::v2::accept(listener, herdr));
    tracing::info!(version = env!("CARGO_PKG_VERSION"), socket = %path.display(), "daemon started");
    let mut term = signal(SignalKind::terminate())?;
    let mut int = signal(SignalKind::interrupt())?;
    tokio::select! {
        _ = term.recv() => tracing::info!("SIGTERM; exiting"),
        _ = int.recv() => tracing::info!("SIGINT; exiting"),
    }
    // Loops and Routine runs live in files: the next daemon goes on with them.
    let _ = std::fs::remove_file(path);
    Ok(())
}

/// Creates the daemon dir with mode 0700 and checks the socket path.
fn prepare(dir: &Path) -> Result<PathBuf, String> {
    let path = crate::v2::path(dir)?;
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("cannot set the mode of {}: {e}", dir.display()))?;
    Ok(path)
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
    crate::v2::path(&dir)?;
    let binary = std::env::current_exe().map_err(|e| format!("cannot find this binary: {e}"))?;
    let spec = Spec {
        binary,
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

/// `stop`, and with `remove`, `uninstall`. The service manager ends the
/// daemon; its loops and Routine runs stay in their files.
fn stop(args: args::Args, remove: bool, out: &mut dyn Write) -> Result<(), String> {
    let (dir, home) = locate(&args)?;
    if block_on(version(&dir))?.is_none() {
        say(out, format_args!("The daemon was not running.\n"))?;
    }
    let platform = Platform::current();
    let file = platform.file(&home);
    let run: &dyn Runner = &install::System;
    if remove {
        platform.disable(run)?;
        match std::fs::remove_file(&file) {
            Ok(()) => say(out, format_args!("Removed {}.\n", file.display()))?,
            Err(e) if e.kind() == ErrorKind::NotFound => {}
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
    say(
        out,
        format_args!("socket: {}\n", dir.join(crate::v2::FILE).display()),
    )?;
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

/// The running daemon's version, from protocol v2's `welcome`
/// (`"server": "muxa 0.1.0"`).
async fn version(dir: &Path) -> Option<String> {
    tokio::time::timeout(CONNECT_WAIT, welcome(dir))
        .await
        .ok()
        .flatten()
}

async fn welcome(dir: &Path) -> Option<String> {
    let mut stream = UnixStream::connect(dir.join(crate::v2::FILE)).await.ok()?;
    let hello = ClientMsg::Hello {
        version: muxa_protocol::v2::VERSION,
        client: format!("muxa-bridge {}", env!("CARGO_PKG_VERSION")),
        resume: None,
    };
    let frame = Frame::control(&hello).ok()?.encode().ok()?;
    stream.write_all(&frame).await.ok()?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(f) = Frame::decode(&mut buf).ok()? {
            if f.channel != 0 || f.kind != Kind::Data {
                return None;
            }
            return match serde_json::from_slice(&f.payload).ok()? {
                ServerMsg::Welcome { server, .. } => {
                    server.strip_prefix("muxa ").map(str::to_owned)
                }
                _ => None,
            };
        }
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
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
    fn the_version_comes_from_the_welcome() {
        let dir = std::env::temp_dir().join(format!("muxa-hello-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let seen = block_on(async {
            let listener = tokio::net::UnixListener::bind(dir.join(crate::v2::FILE)).unwrap();
            tokio::spawn(async move {
                while let Ok((mut stream, _)) = listener.accept().await {
                    let welcome = ServerMsg::Welcome {
                        version: 2,
                        server: format!("muxa {}", env!("CARGO_PKG_VERSION")),
                        backend: "fake".into(),
                        token: "t".into(),
                        resumed: vec![],
                    };
                    let mut hello = [0u8; 256];
                    let _ = stream.read(&mut hello).await;
                    let frame = Frame::control(&welcome).unwrap().encode().unwrap();
                    let _ = stream.write_all(&frame).await;
                }
            });
            wait_for_version(&dir, env!("CARGO_PKG_VERSION"), START_WAIT).await
        });
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(seen, Ok(Some(env!("CARGO_PKG_VERSION").to_owned())));
    }
}
