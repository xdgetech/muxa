//! Typing into an agent pane with no user watching (ADR 0019, "How a prompt
//! is typed"): only on a finished turn, which `step` decides, and only when
//! no person typed on the Task's session in the last few seconds. Callers persist
//! a delivery claim before typing, then record the result after sending.

use std::time::Duration;

use muxa_tmux::{Client, PaneId, SessionId, quote};

/// A client active this recently is a person typing. Seconds.
pub const ACTIVE_WINDOW: u64 = 5;
/// Between the text and Enter, so a TUI does not take both as one paste.
const KEY_GAP: Duration = Duration::from_millis(200);
/// The session as an id, never a name (invariant 3).
const FORMAT: &str = "#{client_activity} #{session_id} #{client_flags}";
const CONTROL_FLAG: &str = "control-mode";

/// The fail prompt: fixed text, only the attempt changes (ADR 0019).
pub fn fail_prompt(attempt: u32) -> String {
    format!(
        "The check failed. Its output is in .muxa/checks/{attempt}.log. \
         Fix the cause, then end your turn."
    )
}

/// True when a client that is not a control client, on `session`, was
/// active within [`ACTIVE_WINDOW`] seconds of `now`. Lines are
/// `list-clients` output in [`FORMAT`]; an unreadable line counts as a person,
/// so a doubt never types.
pub fn person_typing(lines: &[Vec<u8>], session: SessionId, now: u64) -> bool {
    let want = session.to_string();
    lines.iter().any(|line| {
        let line = String::from_utf8_lossy(line);
        let mut parts = line.splitn(3, ' ');
        let (Some(activity), Some(id)) = (parts.next(), parts.next()) else {
            return true;
        };
        let flags = parts.next().unwrap_or("");
        if id != want || flags.split(',').any(|f| f == CONTROL_FLAG) {
            return false;
        }
        match activity.parse::<u64>() {
            Ok(at) => now.saturating_sub(at) < ACTIVE_WINDOW,
            Err(_) => true,
        }
    })
}

/// No person is typing on `session` now.
pub async fn quiet(
    client: &Client,
    session: SessionId,
    now: u64,
) -> Result<bool, muxa_tmux::Error> {
    let reply = client
        .command(&format!("list-clients -F {}", quote(FORMAT)))
        .await?;
    Ok(!person_typing(&reply.lines, session, now))
}

/// Types `text` literally, then Enter.
pub async fn type_prompt(
    client: &Client,
    pane: PaneId,
    text: &str,
) -> Result<(), muxa_tmux::Error> {
    muxa_tmux::send_text(client, pane, text).await?;
    tokio::time::sleep(KEY_GAP).await;
    muxa_tmux::send_keys(client, pane, &["Enter".to_owned()]).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(n: u32) -> SessionId {
        format!("${n}").parse().unwrap()
    }

    fn lines(list: &[&str]) -> Vec<Vec<u8>> {
        list.iter().map(|l| l.as_bytes().to_vec()).collect()
    }

    #[test]
    fn a_recent_desktop_client_on_the_session_is_a_person_typing() {
        let now = 1_000;
        assert!(person_typing(
            &lines(&["998 $1 attached,focused,UTF-8"]),
            session(1),
            now
        ));
        assert!(
            !person_typing(&lines(&["995 $1 attached,focused"]), session(1), now),
            "5 s ago"
        );
        assert!(
            !person_typing(&lines(&["999 $2 attached"]), session(1), now),
            "another session"
        );
        assert!(
            !person_typing(
                &lines(&["999 $1 attached,control-mode,no-output"]),
                session(1),
                now
            ),
            "a control client, the daemon or a bridge"
        );
        assert!(!person_typing(&[], session(1), now));
    }

    #[test]
    fn a_line_in_doubt_counts_as_a_person() {
        assert!(person_typing(&lines(&["garbage"]), session(1), 1_000));
        assert!(person_typing(&lines(&["x $1 attached"]), session(1), 1_000));
    }

    #[test]
    fn the_fail_prompt_is_fixed() {
        assert_eq!(
            fail_prompt(2),
            "The check failed. Its output is in .muxa/checks/2.log. Fix the cause, then end your turn."
        );
    }
}
