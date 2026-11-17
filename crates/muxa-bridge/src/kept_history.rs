//! Each pane's scrollback as it was before this bridge resized its window.
//! A resize redraws agents that clear tmux's history with CSI 3J
//! (docs/findings/tmux-control-mode.md), so `history` adds back what the clear removed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use muxa_tmux::{Client, PaneId, WindowId};

/// Kept lines stay well under a client's line limit after base64, like `history`.
const MAX_BYTES: usize = 8 << 20;
/// Lines read before a resize: the protocol's most.
const KEEP_LINES: u32 = muxa_protocol::MAX_HISTORY_LINES;
/// The last lines of what was kept, found again in a later read, mark where it goes on.
const ANCHOR: usize = 3;

#[derive(Clone, Default)]
pub struct KeptHistories(Arc<Mutex<HashMap<PaneId, Vec<Vec<u8>>>>>);

impl KeptHistories {
    /// Reads the scrollback of every pane in `window`, before the bridge resizes it.
    pub async fn keep_window(&self, client: &Client, window: WindowId) {
        let panes = match muxa_tmux::window_panes(client, window).await {
            Ok(panes) => panes,
            Err(e) => {
                tracing::warn!(%window, error = %e, "listing panes to keep their history failed");
                return;
            }
        };
        for pane in panes {
            match muxa_tmux::history(client, pane, KEEP_LINES).await {
                Ok(read) => {
                    let mut kept = self.0.lock().unwrap_or_else(|e| e.into_inner());
                    let old = kept.remove(&pane).unwrap_or_default();
                    let joined = join(old, lines(&read.data));
                    tracing::debug!(%pane, lines = joined.len(), "kept history before a resize");
                    kept.insert(pane, joined);
                }
                Err(e) => tracing::warn!(%pane, error = %e, "keeping a pane's history failed"),
            }
        }
    }

    /// `read` (a `history` reply's data) after what was kept from before a clear.
    pub fn complete(&self, pane: PaneId, read: &[u8]) -> (Vec<u8>, bool) {
        let kept = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(old) = kept.get(&pane) else {
            return (read.to_vec(), false);
        };
        let joined = join(old.clone(), lines(read));
        let first = newest_within(&joined, MAX_BYTES);
        let mut data = Vec::new();
        for line in &joined[first..] {
            data.extend_from_slice(line);
            data.push(b'\n');
        }
        (data, first > 0)
    }
}

fn lines(data: &[u8]) -> Vec<Vec<u8>> {
    let mut lines: Vec<Vec<u8>> = data.split(|b| *b == b'\n').map(<[u8]>::to_vec).collect();
    if data.ends_with(b"\n") {
        lines.pop();
    }
    lines
}

/// `old`, then the lines of `new` after where `old` ends. When `new` does not
/// hold `old`'s last lines, the history was cleared between: all of `new` follows.
fn join(mut old: Vec<Vec<u8>>, new: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    // The screen's empty rows below its last line are not history.
    while old.last().is_some_and(|l| is_blank(l)) {
        old.pop();
    }
    let anchor: Vec<&Vec<u8>> = old
        .iter()
        .rev()
        .filter(|l| !is_blank(l))
        .take(ANCHOR)
        .collect();
    if anchor.is_empty() {
        return new;
    }
    let anchor: Vec<&Vec<u8>> = anchor.into_iter().rev().collect();
    let found = (0..new.len()).rev().find(|&end| {
        let mut i = end + 1;
        anchor.iter().rev().all(|want| {
            while i > 0 && is_blank(&new[i - 1]) {
                i -= 1;
            }
            if i == 0 || new[i - 1] != **want {
                return false;
            }
            i -= 1;
            true
        })
    });
    match found {
        Some(end) => old.extend(new.into_iter().skip(end + 1)),
        None => old.extend(new),
    }
    old
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(u8::is_ascii_whitespace)
}

/// The index of the oldest line such that the lines from it fit in `max` bytes.
fn newest_within(lines: &[Vec<u8>], max: usize) -> usize {
    let mut size = 0;
    let mut first = lines.len();
    while first > 0 && size + lines[first - 1].len() < max {
        first -= 1;
        size += lines[first].len() + 1;
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[&str]) -> Vec<Vec<u8>> {
        lines.iter().map(|l| l.as_bytes().to_vec()).collect()
    }

    #[test]
    fn a_cleared_history_follows_what_was_kept() {
        let old = text(&["agent 001", "agent 002", "agent 003", ""]);
        let new = text(&["agent redrawn", ""]);
        assert_eq!(
            join(old, new),
            text(&["agent 001", "agent 002", "agent 003", "agent redrawn", ""])
        );
    }

    #[test]
    fn a_kept_history_goes_on_without_repeating_lines() {
        let old = text(&["row 1", "row 2", "$ ", ""]);
        let new = text(&["row 1", "row 2", "$ ", "$ ls", "a b", "$ "]);
        assert_eq!(
            join(old, new),
            text(&["row 1", "row 2", "$ ", "$ ls", "a b", "$ "])
        );
    }

    #[test]
    fn nothing_kept_is_the_new_read() {
        assert_eq!(join(vec![], text(&["a"])), text(&["a"]));
    }

    #[test]
    fn the_newest_lines_fit_the_limit() {
        let all = text(&["aaaa", "bb", "c"]);
        assert_eq!(newest_within(&all, 5), 1);
        assert_eq!(newest_within(&all, 100), 0);
    }

    #[test]
    fn complete_without_a_kept_history_is_the_read() {
        let kept = KeptHistories::default();
        let pane: PaneId = "%1".parse().unwrap();
        assert_eq!(kept.complete(pane, b"x\n"), (b"x\n".to_vec(), false));
    }
}
