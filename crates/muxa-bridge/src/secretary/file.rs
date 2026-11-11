//! `<state dir>/assistant/secretary.json`: the Claude Code session the
//! secretary's pane runs, and muxa's last start of it. Ids and times only;
//! never a transcript. The daemon and a bridge both write it, each write
//! whole; a lost observation comes back on the daemon's next look.
use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::routines::store::{StateError, read_json, write_json};

const DIR: &str = "assistant";
const NAME: &str = "secretary";
const DIR_MODE: u32 = 0o700;
const MAX_FILE: u64 = 4096;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct State {
    /// Claude Code's session id, last seen in the secretary's agent pane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<String>,
    /// muxa's last start of the agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<Started>,
    /// When a resumed start exited at once and muxa dropped the id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_failed_at: Option<u64>,
    /// When Retry on the give-up item asked for a fresh restart cap (ADR 0061).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_at: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    /// Unix seconds.
    pub at: u64,
    /// The session id it resumed; None for a fresh start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<String>,
}

impl State {
    /// A resumed start that ended within `quick` seconds: Claude Code did not
    /// find the conversation, or could not open it.
    pub fn resume_failed(&self, now: u64, quick: u64) -> bool {
        self.started
            .as_ref()
            .is_some_and(|s| s.resume.is_some() && now.saturating_sub(s.at) < quick)
    }

    /// Drops the id, so the next start is fresh, and records why.
    pub fn drop_resume(&mut self, now: u64) {
        self.agent_session = None;
        self.resume_failed_at = Some(now);
        if let Some(s) = self.started.as_mut() {
            s.resume = None;
        }
    }
}

fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join(DIR)
}

/// The file as stored; the default when it does not exist.
pub fn read(state_dir: &Path) -> Result<State, StateError> {
    let path = dir(state_dir).join(format!("{NAME}.json"));
    Ok(read_json::<State>(&path, NAME, MAX_FILE)?.unwrap_or_default())
}

pub fn write(state_dir: &Path, state: &State) -> Result<(), StateError> {
    let dir = dir(state_dir);
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(&dir)
        .map_err(|source| StateError::Io {
            action: "create",
            path: dir.clone(),
            source,
        })?;
    write_json(&dir, NAME, state, MAX_FILE)
}

/// Reads, applies `change`, and writes when it changed anything.
pub fn update(state_dir: &Path, change: impl FnOnce(&mut State)) -> Result<State, StateError> {
    let old = read(state_dir)?;
    let mut new = old.clone();
    change(&mut new);
    if new != old {
        write(state_dir, &new)?;
    }
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("muxa-secretary-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_file_is_empty_and_a_write_reads_back() {
        let dir = temp("rw");
        assert_eq!(read(&dir).unwrap(), State::default());
        let state = update(&dir, |s| {
            s.agent_session = Some("abc".into());
            s.started = Some(Started {
                at: 10,
                resume: Some("abc".into()),
            });
        })
        .unwrap();
        assert_eq!(read(&dir).unwrap(), state);
        let text = std::fs::read_to_string(dir.join("assistant/secretary.json")).unwrap();
        assert!(text.contains("\"agentSession\": \"abc\""), "{text}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn only_a_quick_end_of_a_resumed_start_drops_the_id() {
        let mut s = State {
            agent_session: Some("abc".into()),
            started: Some(Started {
                at: 100,
                resume: Some("abc".into()),
            }),
            resume_failed_at: None,
            retry_at: None,
        };
        assert!(s.resume_failed(110, 30));
        assert!(!s.resume_failed(130, 30), "it ran for a while");
        s.drop_resume(110);
        assert_eq!(s.agent_session, None);
        assert_eq!(s.resume_failed_at, Some(110));
        assert!(
            !s.resume_failed(111, 30),
            "the fresh start is not a failed resume"
        );
    }
}
