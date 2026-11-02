//! Sizing the control client for the device in use (ADR 0020). This is the
//! only code that sends `refresh-client -C` or changes `ignore-size`.
//! The facts it relies on: `scripts/repro/sized-client-latest.sh`.

use crate::client::Client;
use crate::escape::quote;
use crate::{Error, PaneId, SessionId};

/// tmux's `WINDOW_MAXIMUM`: `refresh-client -C` refuses larger sizes.
const MAX_CELLS: u32 = 10_000;

/// A size in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
}

impl Grid {
    fn arg(self) -> Result<String, Error> {
        let ok = |n: u32| (1..=MAX_CELLS).contains(&n);
        if !ok(self.cols) || !ok(self.rows) {
            return Err(Error::Unexpected(format!(
                "size {}x{} is out of range",
                self.cols, self.rows
            )));
        }
        Ok(format!("{}x{}", self.cols, self.rows))
    }
}

/// Whether the pane's window follows the latest client (`window-size latest`).
/// Under any other value muxa never sizes it.
pub async fn follows_device(client: &Client, pane: PaneId) -> Result<bool, Error> {
    let reply = client
        .command(&format!(
            "display-message -p -t {pane} {}",
            quote("#{window-size}")
        ))
        .await
        .map_err(|e| e.for_pane(pane))?;
    Ok(reply.lines.first().is_some_and(|l| l == b"latest"))
}

/// The size of the pane's window now.
pub async fn window_size(client: &Client, pane: PaneId) -> Result<Grid, Error> {
    let reply = client
        .command(&format!(
            "display-message -p -t {pane} {}",
            quote("#{window_width} #{window_height}")
        ))
        .await
        .map_err(|e| e.for_pane(pane))?;
    let line = reply
        .lines
        .first()
        .map(|l| String::from_utf8_lossy(l).into_owned());
    let parsed = line.as_deref().and_then(|l| {
        let (cols, rows) = l.split_once(' ')?;
        Some(Grid {
            cols: cols.parse().ok()?,
            rows: rows.parse().ok()?,
        })
    });
    parsed.ok_or_else(|| Error::Unexpected(format!("window size {line:?}")))
}

pub(crate) fn take_line(session: SessionId, size: Grid) -> Result<String, Error> {
    // -E: the session's environment stays as it is.
    Ok(format!(
        "refresh-client -f {} ; switch-client -E -t {session} ; refresh-client -C {}",
        quote("!ignore-size"),
        size.arg()?
    ))
}

/// Drops `ignore-size`, makes this client the latest one of `session`'s
/// current window, and sizes it, as a terminal of `size` would.
pub async fn take_size(client: &Client, session: SessionId, size: Grid) -> Result<(), Error> {
    all_ok(client.run(&take_line(session, size)?, 3).await?)
}

/// A new size for a client that already holds one.
pub async fn resize_client(client: &Client, size: Grid) -> Result<(), Error> {
    client
        .command(&format!("refresh-client -C {}", size.arg()?))
        .await
        .map(drop)
}

pub(crate) fn release_line(size: Grid) -> Result<String, Error> {
    // Setting the flag recalculates nothing; the repeated size does.
    Ok(format!(
        "refresh-client -f ignore-size ; refresh-client -C {}",
        size.arg()?
    ))
}

/// Sets `ignore-size` again: the windows go back to the other clients.
/// `size` is the one the client holds.
pub async fn release_size(client: &Client, size: Grid) -> Result<(), Error> {
    all_ok(client.run(&release_line(size)?, 2).await?)
}

fn all_ok(results: Vec<Result<crate::Reply, Error>>) -> Result<(), Error> {
    for r in results {
        r?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_target_by_id_and_keep_the_environment() {
        let line = take_line(
            SessionId::new(4),
            Grid {
                cols: 120,
                rows: 40,
            },
        )
        .unwrap();
        assert_eq!(
            line,
            "refresh-client -f \"!ignore-size\" ; switch-client -E -t $4 ; refresh-client -C 120x40"
        );
        assert_eq!(
            release_line(Grid {
                cols: 120,
                rows: 40
            })
            .unwrap(),
            "refresh-client -f ignore-size ; refresh-client -C 120x40"
        );
    }

    #[test]
    fn sizes_beyond_tmux_limits_are_refused() {
        for bad in [(0, 10), (10, 0), (10_001, 10)] {
            let grid = Grid {
                cols: bad.0,
                rows: bad.1,
            };
            assert!(release_line(grid).is_err(), "{bad:?}");
        }
    }
}
