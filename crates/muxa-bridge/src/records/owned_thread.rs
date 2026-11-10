//! The Codex thread muxa created for a run (ADR 0028). Written before the
//! pane starts, under the creation claim, for the run that is reserved then.
// Read by the state and delivery slices of ADR 0028.
#![cfg_attr(not(test), allow(dead_code))]
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Record;

pub const FIELD: &str = "codexThread";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OwnedThread {
    pub version: u32,
    /// The run the thread belongs to.
    pub run: u32,
    pub id: String,
    /// The rollout, by id; it appears with the first turn.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// The `codex` program that serves the App Server.
    pub program: String,
    /// The `CODEX_HOME` the thread lives under; None: Codex's default.
    #[serde(default)]
    pub home: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the run's Codex thread is unreadable")]
    Unreadable,
    #[error("the Codex thread belongs to another run")]
    OtherRun,
}

impl OwnedThread {
    pub fn new(
        run: u32,
        id: &str,
        path: Option<&Path>,
        program: &str,
        home: Option<&Path>,
    ) -> Self {
        Self {
            version: 1,
            run,
            id: id.to_owned(),
            path: path.map(Path::to_path_buf),
            program: program.to_owned(),
            home: home.map(Path::to_path_buf),
        }
    }

    fn valid(&self) -> bool {
        self.version == 1
            && self.run > 0
            && uuid_chars(&self.id)
            && !self.program.trim().is_empty()
            && !self.program.contains('\0')
            && self.path.as_deref().is_none_or(Path::is_absolute)
            && self.home.as_deref().is_none_or(Path::is_absolute)
    }
}

/// As Codex prints a thread id; the App Server client checks it again.
fn uuid_chars(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

impl Record {
    /// A record that has the field is an owned run, also when the field is
    /// damaged: it then gets no state and no input.
    pub fn has_owned_thread(&self) -> bool {
        self.extra.contains_key(FIELD)
    }

    /// The open run's thread. Unknown, damaged or another run's: an error,
    /// never None, so callers fail closed.
    pub fn owned_thread(&self) -> Result<Option<OwnedThread>, Error> {
        let Some(value) = self.extra.get(FIELD) else {
            return Ok(None);
        };
        let thread: OwnedThread =
            serde_json::from_value(value.clone()).map_err(|_| Error::Unreadable)?;
        if !thread.valid() {
            return Err(Error::Unreadable);
        }
        if self.open_run().map(|r| r.id) != Some(thread.run) {
            return Err(Error::OtherRun);
        }
        Ok(Some(thread))
    }

    /// The run a thread created now belongs to: the next one.
    pub fn reserved_run(&self) -> u32 {
        self.runs.last().map_or(1, |r| r.id.saturating_add(1))
    }

    pub fn set_owned_thread(&mut self, thread: &OwnedThread) -> Result<(), Error> {
        let value = serde_json::to_value(thread).map_err(|_| Error::Unreadable)?;
        self.extra.insert(FIELD.into(), value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::RunReason;

    fn record() -> Record {
        serde_json::from_value(serde_json::json!({
            "version":1,"id":"t-000001","rev":1,"workspace":"w","title":"fixture",
            "goal":"","status":"open","createdAt":100,"updatedAt":100
        }))
        .unwrap()
    }

    const ID: &str = "01a1c48e-945c-7353-a880-253b964832f6";

    #[test]
    fn the_thread_counts_only_for_the_run_it_was_reserved_for() {
        let mut r = record();
        assert_eq!(r.owned_thread().unwrap(), None);
        assert!(!r.has_owned_thread());
        let thread = OwnedThread::new(r.reserved_run(), ID, None, "codex", None);
        r.set_owned_thread(&thread).unwrap();
        assert!(r.has_owned_thread());
        assert!(
            matches!(r.owned_thread(), Err(Error::OtherRun)),
            "before the run exists"
        );
        assert_eq!(r.start_run(RunReason::Start, 101), thread.run);
        assert_eq!(r.owned_thread().unwrap(), Some(thread));
        r.start_run(RunReason::Restart, 102);
        assert!(matches!(r.owned_thread(), Err(Error::OtherRun)));
    }

    #[test]
    fn a_damaged_thread_is_an_error_not_absent() {
        let mut r = record();
        r.start_run(RunReason::Start, 101);
        for bad in [
            serde_json::json!({"version": 2, "run": 1, "id": ID, "program": "codex"}),
            serde_json::json!({"version": 1, "run": 1, "id": "x y", "program": "codex"}),
            serde_json::json!({"version": 1, "run": 1, "id": ID, "program": " "}),
            serde_json::json!({"version": 1, "run": 1, "id": ID, "program": "codex",
                               "path": "relative.jsonl"}),
            serde_json::json!({"version": 1, "run": 1, "id": ID, "program": "codex", "new": 1}),
            serde_json::json!("text"),
        ] {
            r.extra.insert(FIELD.into(), bad.clone());
            assert!(r.has_owned_thread());
            assert!(matches!(r.owned_thread(), Err(Error::Unreadable)), "{bad}");
        }
    }
}
