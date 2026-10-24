//! A tmux control-mode client process. It attaches with `-f ignore-size` and
//! never sends a size, so it never sizes a window (docs/adr/0001).

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::mpsc;

use crate::parser::{Event, Notification, Parser, Reply};
use crate::pending::Pending;
use crate::{Error, SessionId};

/// The longest control-mode line accepted. %output lines are one pane read,
/// so real lines stay far below this.
pub const MAX_LINE: usize = 8 << 20;

/// Which tmux binary and server to use.
#[derive(Debug, Clone)]
pub struct Options {
    pub tmux: PathBuf,
    /// The `-L` socket name. `None` is the user's default server.
    pub socket: Option<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            tmux: PathBuf::from("tmux"),
            socket: None,
        }
    }
}

impl Options {
    pub(crate) fn command(&self) -> Command {
        let mut cmd = Command::new(&self.tmux);
        // -u: an SSH exec channel has no locale, and tmux would print "_" for
        // every multibyte character in formats.
        cmd.arg("-u");
        if let Some(socket) = &self.socket {
            cmd.arg("-L").arg(socket);
        }
        // Inside tmux, TMUX is set and control mode refuses to nest.
        cmd.env_remove("TMUX").kill_on_drop(true);
        cmd
    }
}

pub type Notifications = mpsc::UnboundedReceiver<Notification>;

/// A control-mode client. Dropping it detaches; sessions and panes stay.
#[derive(Debug)]
pub struct Client {
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending: Arc<Mutex<Pending>>,
    exit: Arc<Mutex<Option<String>>>,
}

impl Client {
    /// Attaches to `session`, or to tmux's most recent session when `None`.
    /// Notifications arrive in tmux's order and end when the client exits.
    pub fn attach(
        opts: &Options,
        session: Option<SessionId>,
    ) -> Result<(Client, Notifications), Error> {
        let mut cmd = opts.command();
        cmd.args(["-C", "attach-session", "-f", "ignore-size"]);
        if let Some(s) = session {
            cmd.arg("-t").arg(s.to_string());
        }
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::Exited("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::Exited("no stdout".into()))?;

        let pending = Arc::new(Mutex::new(Pending::default()));
        let exit = Arc::new(Mutex::new(None));
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(read_loop(
            child,
            stdout,
            Arc::clone(&pending),
            Arc::clone(&exit),
            tx,
        ));
        Ok((
            Client {
                stdin: tokio::sync::Mutex::new(stdin),
                pending,
                exit,
            },
            rx,
        ))
    }

    /// Sends one command line with `n` commands joined by ` ; ` and returns
    /// one result per command. tmux runs them back to back, with no pane
    /// output in between.
    pub async fn run(&self, line: &str, n: usize) -> Result<Vec<Result<Reply, Error>>, Error> {
        if line.contains(['\n', '\r']) {
            return Err(Error::MultiLineCommand);
        }
        let slots = {
            let mut stdin = self.stdin.lock().await;
            // Slots go in before the write, so the reader finds them.
            let slots = lock(&self.pending).push_line(n)?;
            let written = async {
                stdin.write_all(line.as_bytes()).await?;
                stdin.write_all(b"\n").await?;
                stdin.flush().await
            }
            .await;
            if let Err(e) = written {
                lock(&self.pending).pop_line(n);
                return Err(e.into());
            }
            slots
        };
        let mut results = Vec::with_capacity(n);
        for slot in slots {
            results.push(
                slot.await
                    .unwrap_or_else(|_| Err(Error::Exited(self.exit_reason()))),
            );
        }
        Ok(results)
    }

    /// Runs one command and returns its reply.
    pub async fn command(&self, line: &str) -> Result<Reply, Error> {
        self.run(line, 1)
            .await?
            .pop()
            .unwrap_or_else(|| Err(Error::Unexpected("no reply".into())))
    }

    /// Why the client exited, once it has.
    pub fn exit_reason(&self) -> String {
        lock(&self.exit)
            .clone()
            .unwrap_or_else(|| "tmux exited".into())
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // The guarded state stays consistent even if a holder panicked.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

async fn read_loop(
    mut child: Child,
    stdout: ChildStdout,
    pending: Arc<Mutex<Pending>>,
    exit: Arc<Mutex<Option<String>>>,
    notes: mpsc::UnboundedSender<Notification>,
) {
    let mut stderr = child.stderr.take();
    let mut reader = BufReader::with_capacity(64 * 1024, stdout);
    let mut parser = Parser::new();
    let mut reason: Option<String> = None;
    loop {
        match read_line_capped(&mut reader, MAX_LINE).await {
            Ok(Some(line)) => match parser.feed(line) {
                Some(Event::Reply(r)) => lock(&pending).deliver(r),
                Some(Event::Notification(n)) => {
                    if n.name == "exit" && !n.args.is_empty() {
                        reason = Some(String::from_utf8_lossy(&n.args).into_owned());
                    }
                    // A dropped receiver only means nobody listens any more.
                    let _ = notes.send(n);
                }
                None => {}
            },
            Ok(None) => break,
            Err(e) => {
                reason = Some(e.to_string());
                break;
            }
        }
    }
    if reason.is_none() {
        let mut err = String::new();
        if let Some(stderr) = stderr.as_mut() {
            // tmux has closed stdout, so stderr ends too; the timeout guards a hung child.
            let _ =
                tokio::time::timeout(Duration::from_secs(1), stderr.read_to_string(&mut err)).await;
        }
        let err = err.trim();
        reason = Some(if err.is_empty() {
            "tmux exited".into()
        } else {
            err.to_owned()
        });
    }
    let _ = child.kill().await;
    let reason = reason.unwrap_or_default();
    tracing::debug!(%reason, "tmux control client exited");
    lock(&pending).fail_all(&reason);
    *lock(&exit) = Some(reason);
}

/// Reads one line without its newline. `None` at end of input.
async fn read_line_capped<R: AsyncBufRead + Unpin>(
    r: &mut R,
    max: usize,
) -> Result<Option<Vec<u8>>, Error> {
    let mut line = Vec::new();
    loop {
        let buf = r.fill_buf().await?;
        if buf.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        if let Some(i) = buf.iter().position(|b| *b == b'\n') {
            line.extend_from_slice(&buf[..i]);
            r.consume(i + 1);
            return Ok(Some(line));
        }
        let n = buf.len();
        line.extend_from_slice(buf);
        r.consume(n);
        if line.len() > max {
            return Err(Error::LineTooLong(max));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_lines_as_bytes_and_caps_their_length() {
        let mut r = BufReader::with_capacity(4, &b"ab\xff\ncd\nlast"[..]);
        assert_eq!(
            read_line_capped(&mut r, 100).await.unwrap(),
            Some(b"ab\xff".to_vec())
        );
        assert_eq!(
            read_line_capped(&mut r, 100).await.unwrap(),
            Some(b"cd".to_vec())
        );
        assert_eq!(
            read_line_capped(&mut r, 100).await.unwrap(),
            Some(b"last".to_vec())
        );
        assert_eq!(read_line_capped(&mut r, 100).await.unwrap(), None);

        let mut r = BufReader::with_capacity(4, &b"0123456789\n"[..]);
        assert!(matches!(
            read_line_capped(&mut r, 5).await,
            Err(Error::LineTooLong(5))
        ));
    }
}
