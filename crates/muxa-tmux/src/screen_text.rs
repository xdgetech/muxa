//! Whether a pane's visible screen shows a fixed text. The screen's bytes
//! stay here: callers get a yes or no, never the content (invariant 5).

use crate::client::Client;
use crate::{Error, PaneId};

/// Whether `text` is on one line of the pane's visible screen, as plain
/// text (`capture-pane` without `-e`). `-J` joins lines the terminal
/// wrapped; a program that breaks its own lines is not joined.
pub async fn screen_shows(client: &Client, pane: PaneId, text: &str) -> Result<bool, Error> {
    let reply = client
        .command(&screen_line(pane))
        .await
        .map_err(|e| e.for_pane(pane))?;
    Ok(reply
        .lines
        .iter()
        .any(|line| contains(line, text.as_bytes())))
}

pub(crate) fn screen_line(pane: PaneId) -> String {
    format!("capture-pane -p -J -t {pane}")
}

fn contains(line: &[u8], text: &[u8]) -> bool {
    !text.is_empty() && line.windows(text.len()).any(|w| w == text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_reads_plain_joined_text_of_the_pane_by_id() {
        let pane: PaneId = "%4".parse().unwrap();
        assert_eq!(screen_line(pane), "capture-pane -p -J -t %4");
    }

    #[test]
    fn a_line_shows_the_text_as_bytes_anywhere_in_it() {
        assert!(contains(b"  \xe2\x9d\xaf 1. Yes, I trust", b"Yes, I trust"));
        assert!(contains(b"\xff raw Yes", b"Yes"));
        assert!(!contains(b"Yes, I", b"Yes, I trust"));
        assert!(!contains(b"anything", b""));
    }
}
