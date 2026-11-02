//! Which client size gives the watched pane the device's grid (ADR 0020).
//! tmux sizes windows, not panes, and spreads a change over a split, so the
//! size is found in a few measured steps. Pure: the watcher measures.

use std::ops::RangeInclusive;

use muxa_tmux::Grid;

/// What a client may ask for (`protocol/PROTOCOL.md`).
const COLS: RangeInclusive<u32> = 10..=1000;
const ROWS: RangeInclusive<u32> = 5..=1000;
/// Corrections after the first size.
const MAX_STEPS: u8 = 3;
/// The window never grows past this many viewports, per dimension.
const MAX_FACTOR: i64 = 4;

/// A client's viewport, when it is within the protocol's limits.
pub fn valid(cols: u32, rows: u32) -> Option<Grid> {
    (COLS.contains(&cols) && ROWS.contains(&rows)).then_some(Grid { cols, rows })
}

/// The watched pane and its window, as the last snapshot saw them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seen {
    pub window: Grid,
    pub pane: Grid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    /// Send this size, and look again when the layout has settled.
    Resize(Grid),
    /// The pane has the viewport, or no step can bring it closer.
    Done,
    /// The window is not at the size sent: another client is latest.
    Stop,
}

/// One fit: from the first size to the pane at the viewport.
#[derive(Debug, Clone)]
pub struct Fit {
    viewport: Grid,
    sent: Grid,
    last: Seen,
    /// Pane cells per window cell, measured on the last step that moved them.
    ratio: (f64, f64),
    steps: u8,
}

impl Fit {
    /// The window grows or shrinks by what the pane lacks.
    pub fn start(viewport: Grid, seen: Seen) -> Self {
        let sent = Grid {
            cols: bound(
                viewport.cols,
                i64::from(seen.window.cols) + i64::from(viewport.cols) - i64::from(seen.pane.cols),
            ),
            rows: bound(
                viewport.rows,
                i64::from(seen.window.rows) + i64::from(viewport.rows) - i64::from(seen.pane.rows),
            ),
        };
        Self {
            viewport,
            sent,
            last: seen,
            ratio: (1.0, 1.0),
            steps: 0,
        }
    }

    /// The size the client was given last.
    pub fn sent(&self) -> Grid {
        self.sent
    }

    /// The layout has settled at `seen`.
    pub fn settled(&mut self, seen: Seen) -> Next {
        if seen.window != self.sent {
            return Next::Stop;
        }
        if seen.pane == self.viewport || self.steps >= MAX_STEPS {
            return Next::Done;
        }
        let before = std::mem::replace(&mut self.last, seen);
        let (cols, ratio_cols) = step(
            self.viewport.cols,
            (before.window.cols, before.pane.cols),
            (self.sent.cols, seen.pane.cols),
            self.ratio.0,
        );
        let (rows, ratio_rows) = step(
            self.viewport.rows,
            (before.window.rows, before.pane.rows),
            (self.sent.rows, seen.pane.rows),
            self.ratio.1,
        );
        self.ratio = (ratio_cols, ratio_rows);
        let next = Grid { cols, rows };
        if next == self.sent {
            return Next::Done;
        }
        self.sent = next;
        self.steps += 1;
        Next::Resize(next)
    }
}

/// One dimension: the next window size and the ratio it used. `before` and
/// `now` are (window, pane).
fn step(want: u32, before: (u32, u32), now: (u32, u32), ratio: f64) -> (u32, f64) {
    let (window, pane) = now;
    let moved = (
        f64::from(window) - f64::from(before.0),
        f64::from(pane) - f64::from(before.1),
    );
    let ratio = if moved.0 != 0.0 {
        moved.1 / moved.0
    } else {
        ratio
    };
    if pane == want || ratio <= 0.0 {
        return (window, ratio);
    }
    let delta = ((f64::from(want) - f64::from(pane)) / ratio).round() as i64;
    (bound(want, i64::from(window) + delta), ratio)
}

/// At least the viewport, at most `MAX_FACTOR` viewports.
fn bound(want: u32, n: i64) -> u32 {
    let want = i64::from(want);
    n.clamp(want, want * MAX_FACTOR) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(cols: u32, rows: u32) -> Grid {
        Grid { cols, rows }
    }

    /// tmux-agents: a 28-column sidebar, then two work panes of equal share,
    /// each with a border (`scripts/repro/sized-client-layout.sh`).
    fn agents(window: Grid) -> Seen {
        Seen {
            window,
            pane: grid((window.cols - 28 - 2) / 2, window.rows),
        }
    }

    /// Plain tmux: three side-by-side panes share each new column in turn.
    fn plain(window: Grid) -> Seen {
        let extra = window.cols - 80;
        Seen {
            window,
            pane: grid(25 + extra / 3 + u32::from(extra % 3 >= 2), window.rows),
        }
    }

    fn converge(viewport: Grid, layout: fn(Grid) -> Seen, start: Grid) -> (Grid, Seen, u8) {
        let mut fit = Fit::start(viewport, layout(start));
        let mut sent = fit.sent();
        let mut steps = 0;
        loop {
            match fit.settled(layout(sent)) {
                Next::Resize(g) => {
                    sent = g;
                    steps += 1;
                }
                Next::Done => return (sent, layout(sent), steps),
                Next::Stop => panic!("stopped"),
            }
        }
    }

    #[test]
    fn a_single_pane_window_takes_the_viewport_at_once() {
        let mut fit = Fit::start(
            grid(120, 40),
            Seen {
                window: grid(80, 24),
                pane: grid(80, 24),
            },
        );
        assert_eq!(fit.sent(), grid(120, 40));
        let at = Seen {
            window: grid(120, 40),
            pane: grid(120, 40),
        };
        assert_eq!(fit.settled(at), Next::Done);
    }

    #[test]
    fn a_tmux_agents_pane_reaches_the_viewport() {
        let (sent, seen, steps) = converge(grid(180, 50), agents, grid(80, 24));
        assert_eq!(seen.pane, grid(180, 50), "window {sent:?}");
        assert_eq!(sent, grid(390, 50));
        assert!(steps <= MAX_STEPS, "{steps} steps");
    }

    #[test]
    fn a_plain_split_pane_gets_close_to_the_viewport() {
        let (_, seen, _) = converge(grid(100, 30), plain, grid(80, 24));
        assert!(seen.pane.cols.abs_diff(100) <= 1, "{seen:?}");
        assert_eq!(seen.pane.rows, 30);
    }

    #[test]
    fn another_latest_client_stops_the_fit() {
        let mut fit = Fit::start(grid(120, 40), agents(grid(80, 24)));
        assert_eq!(fit.settled(agents(grid(160, 39))), Next::Stop);
    }

    #[test]
    fn a_pane_that_does_not_grow_ends_the_fit() {
        let fixed = |window: Grid| Seen {
            window,
            pane: grid(28, window.rows),
        };
        let mut fit = Fit::start(grid(60, 30), fixed(grid(80, 24)));
        let first = fit.sent();
        assert_eq!(fit.settled(fixed(first)), Next::Done);
    }

    #[test]
    fn the_window_stays_within_four_viewports() {
        let tiny = |window: Grid| Seen {
            window,
            pane: grid(window.cols / 10, window.rows / 10),
        };
        let (sent, _, _) = converge(grid(100, 30), tiny, grid(80, 24));
        assert!(sent.cols <= 400 && sent.rows <= 120, "{sent:?}");
    }

    #[test]
    fn viewports_outside_the_protocol_limits_are_refused() {
        assert_eq!(valid(10, 5), Some(grid(10, 5)));
        assert_eq!(valid(1000, 1000), Some(grid(1000, 1000)));
        assert_eq!(valid(9, 40), None);
        assert_eq!(valid(80, 4), None);
        assert_eq!(valid(1001, 40), None);
    }
}
