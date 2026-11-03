//! Runs a loop's check (ADR 0019 section 3): `/bin/sh -c` in the Task's
//! directory, in its own process group, killed at the timeout. Output goes to
//! `.muxa/checks/<attempt>.log` and a summary to `.muxa/CHECK.md`. Neither the
//! command nor its output ever reaches a log line (invariant 5).

use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt};

use super::step::CheckEnd;
use crate::records::AgentSettings;

/// `CHECK.md` relative to the task's directory, as the Reader lists it.
pub const REPORT: &str = ".muxa/CHECK.md";
const MUXA_DIR: &str = ".muxa";
const LOGS: &str = "checks";
const REPORT_FILE: &str = "CHECK.md";
/// Bytes of output kept per check.
pub const MAX_LOG: u64 = 8 << 20;
/// Logs kept per Task, newest first.
pub const KEEP_LOGS: usize = 20;
/// Lines of the log in `CHECK.md`.
const TAIL_LINES: usize = 200;
/// Bytes read from the end of the log to find those lines.
const TAIL_BYTES: u64 = 256 << 10;
const SHELL: &str = "/bin/sh";
/// After the shell exits, how long its output may still flow before the group is killed.
const DRAIN: Duration = Duration::from_secs(1);
const TRUNCATED: &str = "\n[muxa: the output reached 8 MiB; the rest was dropped]\n";
const CHUNK: usize = 64 << 10;

/// One check to run. `Debug` leaves out the command and the environment.
pub struct CheckRun {
    /// The Task's directory.
    pub dir: PathBuf,
    pub command: String,
    pub attempt: u32,
    pub timeout: Duration,
    /// The whole environment of the shell.
    pub env: Vec<(String, String)>,
}

impl std::fmt::Debug for CheckRun {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CheckRun")
            .field("dir", &self.dir)
            .field("attempt", &self.attempt)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CheckError {
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("cannot start {SHELL} in {dir}: {source}")]
    Spawn {
        dir: PathBuf,
        source: std::io::Error,
    },
}

fn io(action: &'static str, path: &Path) -> impl FnOnce(std::io::Error) -> CheckError {
    let path = path.to_owned();
    move |source| CheckError::Io {
        action,
        path,
        source,
    }
}

/// `<dir>/.muxa/checks/<attempt>.log`.
pub fn log_path(dir: &Path, attempt: u32) -> PathBuf {
    dir.join(MUXA_DIR).join(LOGS).join(format!("{attempt}.log"))
}

/// Runs the check to its end or its timeout, then writes `CHECK.md` and
/// drops the oldest logs. The process group is gone when this returns, also
/// when the future is dropped.
#[cfg(test)]
pub async fn run(spec: CheckRun) -> Result<CheckEnd, CheckError> {
    run_with_claim(spec, None).await
}

/// Keep restart excluded until the check process has actually been spawned.
pub async fn run_claimed(
    spec: CheckRun,
    claim: crate::records::store::ClaimedTask,
) -> Result<CheckEnd, CheckError> {
    run_with_claim(spec, Some(claim)).await
}

async fn run_with_claim(
    spec: CheckRun,
    claim: Option<crate::records::store::ClaimedTask>,
) -> Result<CheckEnd, CheckError> {
    let path = log_path(&spec.dir, spec.attempt);
    let folder = path.parent().unwrap_or(&spec.dir).to_owned();
    std::fs::create_dir_all(&folder).map_err(io("create", &folder))?;
    let file = std::fs::File::create(&path).map_err(io("create", &path))?;
    let log = Arc::new(Mutex::new(Log {
        file,
        written: 0,
        full: false,
    }));
    let started = Instant::now();
    let mut child = tokio::process::Command::new(SHELL)
        .arg("-c")
        .arg(&spec.command)
        .current_dir(&spec.dir)
        .env_clear()
        .envs(spec.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .map_err(|source| CheckError::Spawn {
            dir: spec.dir.clone(),
            source,
        })?;
    drop(claim);
    let group = Group(child.id());
    let mut readers: Vec<_> = [
        child
            .stdout
            .take()
            .map(|o| tokio::spawn(copy(o, Arc::clone(&log)))),
        child
            .stderr
            .take()
            .map(|e| tokio::spawn(copy(e, Arc::clone(&log)))),
    ]
    .into_iter()
    .flatten()
    .collect();
    let status = match tokio::time::timeout(spec.timeout, child.wait()).await {
        Ok(waited) => Some(waited.map_err(io("wait for", &spec.dir))?),
        Err(_) => {
            group.kill();
            // Reaps the shell; its exit after a kill says nothing.
            let _ = child.wait().await;
            None
        }
    };
    let drained = tokio::time::timeout(DRAIN, async {
        for r in &mut readers {
            let _ = r.await;
        }
    })
    .await;
    // Whatever the check left running is part of it.
    drop(group);
    if drained.is_err() {
        readers.iter().for_each(tokio::task::JoinHandle::abort);
    }
    let seconds = started.elapsed().as_secs();
    let end = match status {
        // A signal has no code: 128 + signal, as a shell reports it.
        Some(s) => CheckEnd::Exited {
            code: s.code().unwrap_or_else(|| 128 + s.signal().unwrap_or(0)),
            seconds,
        },
        None => CheckEnd::TimedOut { seconds },
    };
    let dir = spec.dir.clone();
    let attempt = spec.attempt;
    // The result stands without its summary: the log has the output.
    let summed = tokio::task::spawn_blocking(move || {
        report(&dir, attempt, end)?;
        prune(&dir.join(MUXA_DIR).join(LOGS), KEEP_LOGS)
    })
    .await;
    match summed {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::warn!(attempt, error = %e, "cannot write the check summary"),
        Err(e) => tracing::warn!(attempt, error = %e, "the check summary stopped"),
    }
    Ok(end)
}

/// A process group, killed with `kill`: the crate forbids `unsafe`, and std
/// signals only one process.
struct Group(Option<u32>);

impl Group {
    fn kill(&self) {
        let Some(pid) = self.0 else { return };
        let killed = std::process::Command::new("kill")
            .args(["-KILL", "--", &format!("-{pid}")])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if let Err(e) = killed {
            tracing::warn!(pgid = pid, error = %e, "cannot run kill for a check");
        }
    }
}

/// Also when the run is dropped, as on a clean daemon exit.
impl Drop for Group {
    fn drop(&mut self) {
        self.kill();
    }
}

struct Log {
    file: std::fs::File,
    written: u64,
    full: bool,
}

impl Log {
    /// Writes up to the cap, then one line that says the rest was dropped.
    fn take(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        if self.full {
            return Ok(());
        }
        let room = MAX_LOG.saturating_sub(self.written);
        let n = bytes.len().min(usize::try_from(room).unwrap_or(usize::MAX));
        self.file.write_all(&bytes[..n])?;
        self.written += n as u64;
        if n < bytes.len() {
            self.full = true;
            self.file.write_all(TRUNCATED.as_bytes())?;
        }
        Ok(())
    }
}

/// Copies a pipe into the log; past the cap it keeps reading, so the check
/// never blocks on a full pipe.
async fn copy(mut from: impl AsyncRead + Unpin, log: Arc<Mutex<Log>>) {
    let mut buf = vec![0; CHUNK];
    loop {
        match from.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                let mut log = log.lock().unwrap_or_else(|e| e.into_inner());
                if let Err(e) = log.take(&buf[..n]) {
                    tracing::warn!(error = %e, "cannot write a check log");
                    log.full = true;
                }
            }
        }
    }
}

/// Writes `.muxa/CHECK.md` through a temp file and a rename. Blocking.
fn report(dir: &Path, attempt: u32, end: CheckEnd) -> Result<(), CheckError> {
    let log = log_path(dir, attempt);
    let tail = read_tail(&log).map_err(io("read", &log))?;
    let text = render(attempt, end, &tail);
    let folder = dir.join(MUXA_DIR);
    let path = folder.join(REPORT_FILE);
    let tmp = folder.join(format!(".{REPORT_FILE}.tmp"));
    std::fs::write(&tmp, text).map_err(io("write", &tmp))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io("write", &path)(e)
    })
}

fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    f.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes)?;
    let text = strip_escapes(&String::from_utf8_lossy(&bytes));
    // After a seek, the first line is likely partial.
    let lines: Vec<&str> = text.lines().skip(usize::from(start > 0)).collect();
    let from = lines.len().saturating_sub(TAIL_LINES);
    Ok(lines[from..].join("\n"))
}

/// The text of `CHECK.md`.
pub fn render(attempt: u32, end: CheckEnd, tail: &str) -> String {
    let (result, seconds) = match end {
        CheckEnd::Exited { code: 0, seconds } => ("passed (exit 0)".to_owned(), seconds),
        CheckEnd::Exited { code, seconds } => (format!("failed (exit {code})"), seconds),
        CheckEnd::TimedOut { seconds } => ("timed out".to_owned(), seconds),
    };
    let fence = "`".repeat(longest_run(tail, '`').max(2) + 1);
    format!(
        "# Check\n\n- Attempt: {attempt}\n- Result: {result}\n- Duration: {seconds} s\n\
         - Full output: .muxa/checks/{attempt}.log\n\n\
         ## Last {TAIL_LINES} lines\n\n{fence}text\n{tail}\n{fence}\n"
    )
}

fn longest_run(text: &str, c: char) -> usize {
    let (mut best, mut now) = (0, 0);
    for ch in text.chars() {
        now = if ch == c { now + 1 } else { 0 };
        best = best.max(now);
    }
    best
}

/// Removes terminal escapes (CSI, OSC and two-byte ones) and every control
/// character but newline and tab. A carriage return ends a line.
pub fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    // Parameters and intermediates, up to a final byte @..~.
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    // A string, up to BEL or ST (ESC \).
                    while let Some(c) = chars.next() {
                        if c == '\u{7}' {
                            break;
                        }
                        if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                // A character set: one more byte.
                Some('(' | ')' | '*' | '+') => {
                    chars.next();
                }
                _ => {}
            },
            '\r' if chars.peek() == Some(&'\n') => {}
            '\r' => out.push('\n'),
            '\n' | '\t' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

/// Keeps the newest `keep` logs by modification time. Blocking.
fn prune(folder: &Path, keep: usize) -> Result<(), CheckError> {
    let entries = std::fs::read_dir(folder).map_err(io("read", folder))?;
    let mut logs: Vec<(std::time::SystemTime, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "log"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    logs.sort_by_key(|l| std::cmp::Reverse(l.0));
    for (_, path) in logs.into_iter().skip(keep) {
        std::fs::remove_file(&path).map_err(io("remove", &path))?;
    }
    Ok(())
}

/// `show-environment -g` output as pairs. A `-NAME` line is a variable
/// removed from the environment, and is left out.
pub fn parse_global_env(lines: &[Vec<u8>]) -> Vec<(String, String)> {
    lines
        .iter()
        .map(|l| String::from_utf8_lossy(l))
        .filter(|l| !l.starts_with('-'))
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((k.to_owned(), v.to_owned()))
        })
        .collect()
}

/// The check's environment: the tmux server's global environment, which the
/// panes get, with the workspace's `agent.env` on top. Without tmux, the
/// daemon's own environment with `fallback_path` as `PATH`.
pub fn environment(
    global: Option<Vec<(String, String)>>,
    agent: Option<&AgentSettings>,
    own: impl IntoIterator<Item = (String, String)>,
    fallback_path: Option<&str>,
) -> Vec<(String, String)> {
    let mut env: Vec<(String, String)> = match global {
        Some(g) => g,
        None => {
            let mut own: Vec<_> = own.into_iter().filter(|(k, _)| k != "PATH").collect();
            if let Some(p) = fallback_path {
                own.push(("PATH".into(), p.into()));
            }
            own
        }
    };
    for (k, v) in agent.map(|a| a.env.as_slice()).unwrap_or_default() {
        env.retain(|(have, _)| have != k);
        env.push((k.clone(), v.clone()));
    }
    env
}

/// The tmux server's global environment. The command line holds no value.
pub async fn global_env(
    client: &muxa_tmux::Client,
) -> Result<Vec<(String, String)>, muxa_tmux::Error> {
    let reply = client.command("show-environment -g").await?;
    Ok(parse_global_env(&reply.lines))
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
