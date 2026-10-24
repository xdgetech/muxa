//! Shows one pane in the local terminal: the bridge's bytes go to stdout,
//! keystrokes go back as raw bytes. Ctrl-] leaves.

use std::io::Write;
use std::time::Duration;

use crossterm::terminal;
use muxa_protocol::{Message, Request};
use tokio::io::AsyncReadExt;

use crate::conn::Conn;

const QUIT: u8 = 0x1d; // Ctrl-]
const PING_EVERY: Duration = Duration::from_secs(15);

pub struct Options {
    pub read_only: bool,
    /// Leave after this long; `None` waits for Ctrl-].
    pub seconds: Option<u64>,
}

/// Restores the local terminal however the view ends.
struct Raw;

impl Raw {
    fn enter() -> Result<Raw, String> {
        terminal::enable_raw_mode().map_err(|e| format!("cannot use this terminal: {e}"))?;
        print!("\x1b[?1049h");
        let _ = std::io::stdout().flush();
        Ok(Raw)
    }
}

impl Drop for Raw {
    fn drop(&mut self) {
        // Soft reset first: the pane may have left modes on (mouse, keypad).
        print!("\x1b[!p\x1b[?1049l\x1b[?25h");
        let _ = std::io::stdout().flush();
        let _ = terminal::disable_raw_mode();
    }
}

pub async fn run(conn: &mut Conn, pane: &str, opts: Options) -> Result<String, String> {
    let watch = conn.id();
    conn.send(&Request::Watch {
        id: watch,
        pane: pane.to_owned(),
    })
    .await?;
    // Without a terminal (a script, a pipe), only stream: no raw mode, no keys.
    let interactive = std::io::IsTerminal::is_terminal(&std::io::stdin());
    let _raw = if interactive {
        Some(Raw::enter()?)
    } else {
        None
    };
    let mut stdin = tokio::io::stdin();
    let mut buf = [0u8; 4096];
    let mut ping = tokio::time::interval(PING_EVERY);
    let deadline = tokio::time::sleep(Duration::from_secs(opts.seconds.unwrap_or(u64::MAX / 4)));
    tokio::pin!(deadline);
    let mut errors = 0usize;
    loop {
        tokio::select! {
            msg = conn.recv_forever() => match msg? {
                Message::Screen { watch: w, cols, rows, data, .. } if w == watch => {
                    warn_if_small(cols, rows);
                    write_out(&strip_reset(&data));
                }
                Message::Output { watch: w, data, .. } if w == watch => write_out(&data),
                Message::PaneGone { watch: w, reason, .. } if w == watch => {
                    return Ok(format!("pane {pane} is gone{}", reason.map(|r| format!(": {r}")).unwrap_or_default()));
                }
                Message::Bye { reason } => return Ok(format!("bridge exited: {reason}")),
                Message::Result { error: Some(_), .. } => errors += 1,
                _ => {}
            },
            n = stdin.read(&mut buf), if interactive => {
                let n = n.map_err(|e| e.to_string())?;
                if n == 0 || buf[..n].contains(&QUIT) {
                    break;
                }
                if !opts.read_only {
                    let id = conn.id();
                    conn.send(&Request::Bytes { id, pane: pane.to_owned(), data: buf[..n].to_vec() }).await?;
                }
            }
            _ = ping.tick() => {
                let id = conn.id();
                conn.send(&Request::Ping { id }).await?;
            }
            _ = &mut deadline => break,
        }
    }
    let id = conn.id();
    conn.send(&Request::Bye { id }).await?;
    Ok(if errors == 0 {
        "left".into()
    } else {
        format!("left; {errors} requests failed")
    })
}

/// The screen starts with a full reset (ESC c). On a real terminal that
/// would also leave the alternate screen this view runs in, so use a soft
/// reset and a clear instead.
fn strip_reset(data: &[u8]) -> Vec<u8> {
    match data.strip_prefix(b"\x1bc") {
        Some(rest) => [b"\x1b[!p\x1b[H\x1b[2J".as_slice(), rest].concat(),
        None => data.to_vec(),
    }
}

fn write_out(data: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(data);
    let _ = out.flush();
}

fn warn_if_small(cols: u16, rows: u16) {
    if let Ok((w, h)) = terminal::size()
        && (w < cols || h < rows)
    {
        // Shown in the title, so it does not disturb the pane's screen.
        print!("\x1b]0;muxa: terminal {w}x{h} is smaller than the pane {cols}x{rows}\x07");
    }
}
