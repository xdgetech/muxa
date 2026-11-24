//! Typing a message into an agent's terminal and sending it with Return.
//! Claude Code reads one write of about 65 bytes or more as a paste, and a
//! Return inside a paste is a new line (docs/findings/claude-code-input.md).

use std::time::Duration;

use muxa_backend::{BackendError, TerminalStream};

/// Bracketed paste: a message of several lines arrives as one paste, so its
/// new lines do not send it line by line.
const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
/// Time for typed bytes to leave before the next write or the detach. The
/// apps wait as long between a message and its Return.
pub const SETTLE: Duration = Duration::from_millis(200);

/// The writes of `text` as a message: its text, then Return alone.
pub fn message_writes(text: &str) -> [Vec<u8>; 2] {
    let body = if text.contains('\n') {
        [PASTE_START, text.as_bytes(), PASTE_END].concat()
    } else {
        text.as_bytes().to_vec()
    };
    [body, b"\r".to_vec()]
}

/// Types `text` on `stream`, waits, then sends Return as a write of its own.
/// It waits again so the bytes leave, then detaches the stream.
pub async fn type_message<S: TerminalStream>(stream: S, text: &str) -> Result<(), BackendError> {
    let [body, submit] = message_writes(text);
    // Each write ends before a wait: the stream is Send, not Sync.
    let typed = stream.input(&body);
    tokio::time::sleep(SETTLE).await;
    let typed = typed.and_then(|()| stream.input(&submit));
    tokio::time::sleep(SETTLE).await;
    let _ = stream.detach();
    typed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_line_has_its_return_apart() {
        let long = "x".repeat(300);
        assert_eq!(message_writes(&long), [long.into_bytes(), b"\r".to_vec()]);
    }

    #[test]
    fn several_lines_are_one_paste_then_return() {
        assert_eq!(
            message_writes("Fix it.\nAnd add a test."),
            [
                b"\x1b[200~Fix it.\nAnd add a test.\x1b[201~".to_vec(),
                b"\r".to_vec()
            ]
        );
    }
}
