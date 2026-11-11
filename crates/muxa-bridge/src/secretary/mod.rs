//! The secretary stays open (ADR 0056): the overall workspace's root Ask
//! session is resident. Its agent continues the same Claude Code
//! conversation when it starts again, and the daemon starts it again when it
//! ends, within a cap. It never touches a session it did not start.
mod command;
pub mod file;
mod keeper;
mod tick;

use std::path::Path;

use crate::records::workspaces::Workspace;

pub use command::resume_command;
pub use tick::{Resident, tick};

/// Marks the secretary's session, for muxa for tmux's tree.
pub const TAG: &str = "@muxa_secretary";

/// The command a workspace's agents start with: its own, else `default`
/// (`--task-command`).
pub fn base_command<'a>(ws: &'a Workspace, default: &'a str) -> &'a str {
    ws.agent
        .as_ref()
        .and_then(|a| a.command.as_deref())
        .unwrap_or(default)
}

/// The command for the next start of the secretary's agent: `base` with
/// `--resume <id>` when muxa knows its Claude Code conversation, else
/// `base`. Records the start, so the daemon can tell a failed resume.
pub fn prepare(state_dir: Option<&Path>, base: &str, now: u64) -> String {
    let Some(dir) = state_dir else {
        return base.to_owned();
    };
    let state = match file::read(dir) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read the secretary's state; it starts fresh");
            file::State::default()
        }
    };
    let resumed = state
        .agent_session
        .as_deref()
        .and_then(|id| Some((id, resume_command(base, id)?)));
    let started = file::Started {
        at: now,
        resume: resumed.as_ref().map(|(id, _)| (*id).to_owned()),
    };
    if let Err(e) = file::update(dir, |s| s.started = Some(started)) {
        tracing::warn!(error = %e, "cannot record the secretary's start");
    }
    tracing::info!(resume = resumed.is_some(), "secretary agent starting");
    resumed.map_or_else(|| base.to_owned(), |(_, command)| command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_known_conversation_is_resumed_and_the_start_is_recorded() {
        let dir = std::env::temp_dir().join(format!("muxa-secretary-prep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(prepare(Some(&dir), "claude", 5), "claude");
        let state = file::read(&dir).unwrap();
        assert_eq!(state.started.unwrap().resume, None);

        let id = "11111111-2222-4333-8444-555555555555";
        file::update(&dir, |s| s.agent_session = Some(id.into())).unwrap();
        assert_eq!(
            prepare(Some(&dir), "claude", 9),
            format!("claude --resume {id}")
        );
        let started = file::read(&dir).unwrap().started.unwrap();
        assert_eq!((started.at, started.resume.as_deref()), (9, Some(id)));

        assert_eq!(
            prepare(Some(&dir), "codex", 12),
            "codex",
            "Codex starts fresh"
        );
        assert_eq!(file::read(&dir).unwrap().started.unwrap().resume, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
