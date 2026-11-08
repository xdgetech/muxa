//! A pane's scrollback and screen, read once with `capture-pane`. Reading it
//! changes nothing in the pane: no copy mode, no scroll position.

use crate::client::Client;
use crate::escape::quote;
use crate::parser::Reply;
use crate::{Error, PaneId};

/// Lines `cols` wide, oldest first, each ending in `\n`, with their colors as
/// escape sequences (`capture-pane -e`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    pub pane: PaneId,
    pub cols: u16,
    pub data: Vec<u8>,
    /// The pane has older lines than were read.
    pub truncated: bool,
}

/// The pane's last `lines` scrollback lines, then its visible screen. The
/// alternate screen, when shown, has no scrollback of its own.
pub async fn history(client: &Client, pane: PaneId, lines: u32) -> Result<History, Error> {
    let results = client.run(&history_line(pane, lines), 2).await?;
    let mut replies = Vec::with_capacity(2);
    for r in results {
        replies.push(r.map_err(|e| e.for_pane(pane))?);
    }
    let [size, capture] = <[Reply; 2]>::try_from(replies)
        .map_err(|_| Error::Unexpected("history reply count".into()))?;
    let (cols, kept) = parse_size(size.lines.first().ok_or(Error::NoPane(pane))?)?;
    let (data, cut) = newest_lines(&capture.lines, MAX_BYTES);
    Ok(History {
        pane,
        cols,
        data,
        truncated: cut || kept > u64::from(lines),
    })
}

/// A history message must stay well under a client's 32 MiB line limit after base64.
const MAX_BYTES: usize = 8 << 20;

/// The newest lines that fit in `max` bytes, each ending in `\n`; true when older ones were left out.
fn newest_lines(lines: &[Vec<u8>], max: usize) -> (Vec<u8>, bool) {
    let mut size = 0;
    let mut first = lines.len();
    while first > 0 && size + lines[first - 1].len() < max {
        first -= 1;
        size += lines[first].len() + 1;
    }
    let mut data = Vec::with_capacity(size);
    for line in &lines[first..] {
        data.extend_from_slice(line);
        data.push(b'\n');
    }
    (data, first > 0)
}

pub(crate) fn history_line(pane: PaneId, lines: u32) -> String {
    format!(
        "display-message -p -t {pane} {} ; capture-pane -p -e -t {pane} -S -{lines}",
        quote("#{pane_width} #{history_size}")
    )
}

fn parse_size(line: &[u8]) -> Result<(u16, u64), Error> {
    let text = String::from_utf8_lossy(line);
    let mut parts = text.split(' ');
    let bad = || Error::Unexpected(format!("pane size {text:?}"));
    let cols = parts.next().and_then(|c| c.parse().ok()).ok_or_else(bad)?;
    let kept = parts.next().and_then(|h| h.parse().ok()).ok_or_else(bad)?;
    Ok((cols, kept))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_starts_lines_above_the_screen_and_targets_the_pane_by_id() {
        assert_eq!(
            history_line(PaneId::new(4), 2000),
            r##"display-message -p -t %4 "#{pane_width} #{history_size}" ; capture-pane -p -e -t %4 -S -2000"##
        );
    }

    #[test]
    fn a_history_over_the_byte_limit_keeps_its_newest_lines() {
        let lines = vec![b"old".to_vec(), b"mid".to_vec(), b"new".to_vec()];
        assert_eq!(newest_lines(&lines, 8), (b"mid\nnew\n".to_vec(), true));
        assert_eq!(
            newest_lines(&lines, 100),
            (b"old\nmid\nnew\n".to_vec(), false)
        );
    }

    #[test]
    fn the_size_line_gives_the_width_and_the_lines_kept() {
        assert_eq!(parse_size(b"80 1234").unwrap(), (80, 1234));
        assert!(parse_size(b"80").is_err());
    }
}
