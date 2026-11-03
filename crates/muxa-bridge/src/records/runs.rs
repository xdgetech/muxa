//! Agent runs (ADR 0018): which agents worked on a Task, as the bridge
//! started or restarted them. Without a daemon, nothing else is recorded.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Record;

/// The newest runs a record keeps.
pub const MAX_RUNS: usize = 50;

/// One agent on a Task. Times are Unix epoch seconds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    /// From 1; never reused, also after old runs are dropped.
    pub id: u32,
    /// The adapter id; absent until an adapter reported the agent (invariant 6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub started_at: u64,
    /// Set when the bridge restarted the agent, not when the process ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<PathBuf>,
    pub reason: RunReason,
    /// Fields written by a newer muxa; kept on rewrite.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunReason {
    Start,
    Restart,
}

/// What an adapter reported about the agent in the Task's pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seen {
    pub agent: String,
    pub session: Option<String>,
    pub transcript: Option<PathBuf>,
}

impl Run {
    pub fn assignment(&self) -> Option<muxa_protocol::WorkerAssignment> {
        self.extra
            .get("assignment")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    /// Takes what the adapter saw. A known session id is replaced only by
    /// another one: the agent may have switched sessions (`/clear`).
    pub fn fill(&mut self, seen: &Seen) {
        self.agent = Some(seen.agent.clone());
        if seen.session.is_some() {
            self.agent_session = seen.session.clone();
            self.transcript = seen.transcript.clone();
        }
    }
}

impl Record {
    /// Ends the last run if it is still open, adds a new one, and keeps the
    /// newest [`MAX_RUNS`]. Returns the new run's id.
    pub fn start_run(&mut self, reason: RunReason, now: u64) -> u32 {
        if let Some(last) = self.runs.last_mut().filter(|r| r.ended_at.is_none()) {
            last.ended_at = Some(now.max(last.started_at));
        }
        let id = self.runs.last().map_or(1, |r| r.id.saturating_add(1));
        self.runs.push(Run {
            id,
            agent: None,
            started_at: now,
            ended_at: None,
            agent_session: None,
            transcript: None,
            reason,
            extra: serde_json::Map::new(),
        });
        let excess = self.runs.len().saturating_sub(MAX_RUNS);
        self.runs.drain(..excess);
        id
    }

    pub fn run_mut(&mut self, id: u32) -> Option<&mut Run> {
        self.runs.iter_mut().find(|r| r.id == id)
    }

    /// The last run, when no restart has ended it yet.
    pub fn open_run(&self) -> Option<&Run> {
        self.runs.last().filter(|r| r.ended_at.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::Status;

    fn record() -> Record {
        Record {
            version: 1,
            id: "t-000001".into(),
            rev: 1,
            workspace: "w".into(),
            title: "T".into(),
            goal: String::new(),
            status: Status::Open,
            created_at: 0,
            updated_at: 0,
            project: None,
            repo: None,
            branch: None,
            dir: None,
            session: None,
            note: None,
            runs: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    #[test]
    fn a_new_run_ends_the_open_one() {
        let mut r = record();
        assert_eq!(r.start_run(RunReason::Start, 100), 1);
        assert_eq!(r.open_run().map(|r| r.id), Some(1));
        assert_eq!(r.start_run(RunReason::Restart, 200), 2);
        assert_eq!(r.runs[0].ended_at, Some(200));
        assert_eq!(r.runs[1].ended_at, None);
        assert_eq!(r.runs[1].reason, RunReason::Restart);
    }

    #[test]
    fn runs_are_bounded_and_ids_never_reused() {
        let mut r = record();
        for t in 0..(MAX_RUNS as u64 + 7) {
            r.start_run(RunReason::Restart, t);
        }
        assert_eq!(r.runs.len(), MAX_RUNS);
        assert_eq!(r.runs[0].id, 8, "the oldest are dropped");
        assert_eq!(r.runs.last().map(|r| r.id), Some(MAX_RUNS as u32 + 7));
    }

    #[test]
    fn filling_keeps_a_known_session_until_another_is_seen() {
        let mut r = record();
        let id = r.start_run(RunReason::Start, 1);
        let run = r.run_mut(id).unwrap();
        run.fill(&Seen {
            agent: "claude-code".into(),
            session: Some("a1".into()),
            transcript: Some("/t/a1.jsonl".into()),
        });
        run.fill(&Seen {
            agent: "claude-code".into(),
            session: None,
            transcript: None,
        });
        assert_eq!(run.agent_session.as_deref(), Some("a1"));
        run.fill(&Seen {
            agent: "claude-code".into(),
            session: Some("b2".into()),
            transcript: None,
        });
        assert_eq!(
            (run.agent_session.as_deref(), &run.transcript),
            (Some("b2"), &None)
        );
    }

    #[test]
    fn unknown_run_fields_survive() {
        let json = r#"{"id":3,"startedAt":5,"reason":"start","later":true}"#;
        let run: Run = serde_json::from_str(json).unwrap();
        assert_eq!(serde_json::to_string(&run).unwrap(), json);
    }
}
