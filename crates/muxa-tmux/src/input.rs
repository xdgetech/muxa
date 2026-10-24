//! Sends input to a pane. Named keys go through tmux's own key encoding, so
//! they match the pane's modes (application cursor keys, extended keys).

use std::fmt::Write;

use crate::client::Client;
use crate::escape::quote;
use crate::{Error, PaneId};

const NAMED_KEYS: [&str; 19] = [
    "Up", "Down", "Left", "Right", "Home", "End", "PageUp", "PageDown", "PPage", "NPage", "Enter",
    "Escape", "Tab", "BTab", "BSpace", "DC", "IC", "Space", "KPEnter",
];

/// A key tmux understands: up to three `C-`/`M-`/`S-` modifiers, then one
/// printable ASCII character, F1-F12 or a named key. A whitelist, so a key
/// can never carry a command.
pub fn valid_key(key: &str) -> bool {
    let mut rest = key;
    for _ in 0..3 {
        match rest.get(..2) {
            Some("C-" | "M-" | "S-") if rest.len() > 2 => rest = &rest[2..],
            _ => break,
        }
    }
    let single = rest.len() == 1 && rest.bytes().all(|b| b.is_ascii_graphic());
    let function = rest
        .strip_prefix('F')
        .and_then(|n| n.parse::<u8>().ok())
        .is_some_and(|n| (1..=12).contains(&n));
    single || function || NAMED_KEYS.contains(&rest)
}

pub(crate) fn keys_line(pane: PaneId, keys: &[String]) -> Result<String, Error> {
    let mut line = format!("send-keys -t {pane}");
    for key in keys {
        if !valid_key(key) {
            return Err(Error::UnsupportedKey(key.clone()));
        }
        line.push(' ');
        line.push_str(&quote(key));
    }
    Ok(line)
}

pub(crate) fn text_line(pane: PaneId, text: &str) -> String {
    format!("send-keys -t {pane} -l -- {}", quote(text))
}

pub(crate) fn bytes_line(pane: PaneId, data: &[u8]) -> String {
    let mut line = format!("send-keys -t {pane} -H");
    for b in data {
        // Infallible: writing to a String.
        let _ = write!(line, " {b:02x}");
    }
    line
}

fn paste_buffer(pane: PaneId) -> String {
    quote(&format!("muxa-paste-{}", pane.number()))
}

/// Sends named keys such as `C-c`, `Up` or `M-Enter`.
pub async fn send_keys(client: &Client, pane: PaneId, keys: &[String]) -> Result<(), Error> {
    if keys.is_empty() {
        return Ok(());
    }
    client
        .command(&keys_line(pane, keys)?)
        .await
        .map(drop)
        .map_err(|e| e.for_pane(pane))
}

/// Types `text` literally.
pub async fn send_text(client: &Client, pane: PaneId, text: &str) -> Result<(), Error> {
    if text.is_empty() {
        return Ok(());
    }
    client
        .command(&text_line(pane, text))
        .await
        .map(drop)
        .map_err(|e| e.for_pane(pane))
}

/// Writes raw key bytes, for a client that encodes keys itself.
pub async fn send_bytes(client: &Client, pane: PaneId, data: &[u8]) -> Result<(), Error> {
    if data.is_empty() {
        return Ok(());
    }
    client
        .command(&bytes_line(pane, data))
        .await
        .map(drop)
        .map_err(|e| e.for_pane(pane))
}

/// Pastes `text` the way a terminal does: bracketed when the program asked
/// for it, so a shell or agent does not run each line on its own.
pub async fn paste(client: &Client, pane: PaneId, text: &str) -> Result<(), Error> {
    let buffer = paste_buffer(pane);
    let line = format!(
        "set-buffer -b {buffer} -- {} ; paste-buffer -p -d -b {buffer} -t {pane}",
        quote(text)
    );
    let results = client.run(&line, 2).await?;
    for r in results {
        if let Err(e) = r {
            // paste-buffer -d deletes the buffer only when it succeeds.
            let _ = client.command(&format!("delete-buffer -b {buffer}")).await;
            return Err(e.for_pane(pane));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_tmux_key_names() {
        for key in [
            "a", "Z", "~", "C-c", "M-x", "C-M-a", "S-Enter", "M-Enter", "Up", "BTab", "F1", "F12",
            "C-F5", "Escape", "BSpace",
        ] {
            assert!(valid_key(key), "{key}");
        }
    }

    #[test]
    fn rejects_anything_else() {
        for key in [
            "",
            "ab",
            "C-",
            "F13",
            "F0",
            "Any",
            "C-C-C-C-a",
            "Up;kill-server",
            "a b",
            "é",
            "\n",
            "C-\t",
        ] {
            assert!(!valid_key(key), "{key:?}");
        }
    }

    #[test]
    fn builds_quoted_lines() {
        let pane = PaneId::new(4);
        assert_eq!(
            keys_line(pane, &["C-c".into(), "Up".into()]).unwrap(),
            r#"send-keys -t %4 "C-c" "Up""#
        );
        assert!(keys_line(pane, &["bad key".into()]).is_err());
        assert_eq!(text_line(pane, "a\"$"), r#"send-keys -t %4 -l -- "a\"\$""#);
        assert_eq!(
            bytes_line(pane, &[0x1b, 0xc3, 0xa9]),
            "send-keys -t %4 -H 1b c3 a9"
        );
        assert_eq!(paste_buffer(pane), r#""muxa-paste-4""#);
    }
}
