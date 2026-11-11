//! Claude Code's "Do you trust this folder?" dialog, which it shows in a
//! folder that neither it nor a parent was trusted in. No hook fires and no
//! session file says so, so only the pane's screen tells
//! (`docs/findings/claude-code-trust.md`).
use muxa_tmux::{Client, PaneId};

/// The dialog's confirm option. Short enough to stay on one line of an
/// 80-column pane; the question above it is wrapped by Claude Code itself.
pub const LABEL: &str = "Yes, I trust this folder";

/// Whether the pane shows the dialog now. False when tmux cannot tell.
pub async fn asks(client: &Client, pane: PaneId) -> bool {
    match muxa_tmux::screen_shows(client, pane, LABEL).await {
        Ok(shown) => shown,
        Err(e) => {
            tracing::debug!(pane = %pane, error = %e, "cannot read the screen for the trust dialog");
            false
        }
    }
}
