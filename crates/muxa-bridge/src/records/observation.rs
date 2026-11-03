//! Managed starts and restarts need evidence from their process, not retained pane tags.

use serde::{Deserialize, Serialize};

use super::lifecycle::{Operation, ProcessIdentity};
use super::{Record, Run, Seen};

// The private v1 field name is retained for compatibility with persisted restart records.
pub const FIELD: &str = "restartObservation";

#[derive(Debug, thiserror::Error)]
#[error("restart observation is unreadable")]
pub struct Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Baseline {
    pub version: u32,
    pub operation: String,
    pub session: u32,
    pub pane: u32,
    pub replacement: ProcessIdentity,
    pub source: Option<Seen>,
}

/// Called in the same record update that finalizes the new run and pending prompt.
pub fn bind_restart(run: &mut Run, op: &Operation) {
    // Missing replacement metadata is deliberately unreadable and blocks input.
    run.extra.insert(
        FIELD.into(),
        serde_json::json!({
            "version": 1,
            "operation": op.id,
            "session": op.session,
            "pane": op.pane,
            "replacement": op.replacement,
            "source": op.seen,
        }),
    );
}

/// Store startup identity with the first run and pending prompt in one record update.
pub fn bind_start(
    run: &mut Run,
    operation: &str,
    session: u32,
    pane: u32,
    process: ProcessIdentity,
) {
    run.extra.insert(
        FIELD.into(),
        serde_json::json!({
            "version":1, "operation":operation, "session":session, "pane":pane,
            "replacement":process, "source":null,
        }),
    );
}

impl Record {
    /// Named execution needs a known requested assignment and locally stored process binding.
    pub fn worker_run_bound(&self) -> bool {
        let Some(name) = self.worker() else {
            return false;
        };
        let Some(run) = self.open_run() else {
            return false;
        };
        if run
            .extra
            .get("outcome")
            .is_some_and(|value| value == "unconfirmed")
        {
            return false;
        }
        run.assignment().is_some_and(|assignment| {
            assignment.kind == "worker" && assignment.name.as_deref() == Some(name)
        }) && run
            .restart_observation()
            .is_ok_and(|baseline| baseline.is_some())
    }
}

impl Run {
    /// Absent is a legacy run. Unknown or damaged evidence must fail closed.
    pub fn restart_observation(&self) -> Result<Option<Baseline>, Error> {
        let Some(value) = self.extra.get(FIELD) else {
            return Ok(None);
        };
        let baseline: Baseline = serde_json::from_value(value.clone()).map_err(|_| Error)?;
        if baseline.version != 1
            || baseline.operation.is_empty()
            || baseline.replacement.pid == 0
            || baseline.replacement.started.is_empty()
        {
            return Err(Error);
        }
        Ok(Some(baseline))
    }
}

impl Baseline {
    pub fn accepts(
        &self,
        session: u32,
        pane: u32,
        pid: u32,
        shell_foreground: bool,
        process_bound: bool,
        seen: Option<&Seen>,
    ) -> bool {
        if self.session != session
            || self.pane != pane
            || self.replacement.pid != pid
            || shell_foreground
            || !process_bound
        {
            return false;
        }
        let Some(seen) = seen.filter(|seen| seen.session.as_ref().is_some_and(|s| !s.is_empty()))
        else {
            return false;
        };
        !self.source.as_ref().is_some_and(|source| {
            (source.agent == seen.agent && source.session == seen.session)
                || source
                    .transcript
                    .as_ref()
                    .is_some_and(|path| seen.transcript.as_ref() == Some(path))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(session: &str) -> Seen {
        Seen {
            agent: "fixture".into(),
            session: Some(session.into()),
            transcript: Some(format!("/fixture/{session}.jsonl").into()),
        }
    }

    #[test]
    fn a_same_second_report_cannot_reuse_the_source_session_or_transcript() {
        let source = seen("old");
        let baseline = Baseline {
            version: 1,
            operation: "restart-1".into(),
            session: 1,
            pane: 2,
            replacement: ProcessIdentity {
                pid: 200,
                started: "same-second".into(),
            },
            source: Some(source.clone()),
        };
        assert!(!baseline.accepts(1, 2, 200, false, true, Some(&source)));
        let mut stale_file = seen("new");
        stale_file.transcript = source.transcript;
        assert!(!baseline.accepts(1, 2, 200, false, true, Some(&stale_file)));
        let fresh = seen("new");
        assert!(baseline.accepts(1, 2, 200, false, true, Some(&fresh)));
        assert!(!baseline.accepts(1, 2, 100, false, true, Some(&fresh)));
        assert!(!baseline.accepts(1, 3, 200, false, true, Some(&fresh)));
        assert!(!baseline.accepts(2, 2, 200, false, true, Some(&fresh)));
        assert!(!baseline.accepts(1, 2, 200, true, true, Some(&fresh)));
        assert!(!baseline.accepts(1, 2, 200, false, true, None));
        assert!(
            !baseline.accepts(1, 2, 200, false, false, Some(&fresh)),
            "cwd/mtime proximity is not process ownership"
        );
        let without_source = Baseline {
            source: None,
            ..baseline
        };
        assert!(!without_source.accepts(1, 2, 200, false, false, Some(&fresh)));
        assert!(without_source.accepts(1, 2, 200, false, true, Some(&fresh)));
    }

    #[test]
    fn a_finalized_run_keeps_its_operation_baseline_after_reopen() {
        let mut run: Run = serde_json::from_value(serde_json::json!({
            "id":2,"startedAt":100,"reason":"restart","future":"retained"
        }))
        .unwrap();
        assert!(run.restart_observation().unwrap().is_none());
        let op: Operation = serde_json::from_value(serde_json::json!({
            "version":1,"id":"restart-1","phase":"replaced","sourceRun":1,"reservedRun":2,
            "pane":2,"sourcePid":100,"worker":null,"session":1,"sourceStarted":"old-start",
            "replacement":{"pid":200,"started":"new-start"},"since":100,
            "seen":{"agent":"fixture","session":"old","transcript":"/fixture/old.jsonl"}
        }))
        .unwrap();
        bind_restart(&mut run, &op);
        let reopened: Run = serde_json::from_value(serde_json::to_value(&run).unwrap()).unwrap();
        let baseline = reopened.restart_observation().unwrap().unwrap();
        assert_eq!(baseline.operation, op.id);
        assert_eq!(baseline.source, op.seen);
        assert_eq!(Some(baseline.replacement), op.replacement);
        assert!(
            reopened.agent.is_none(),
            "requested replacement has no observed identity yet"
        );
        assert_eq!(reopened.extra["future"], "retained");
        run.extra.get_mut(FIELD).unwrap()["version"] = serde_json::json!(99);
        assert!(run.restart_observation().is_err());
        run.extra
            .insert(FIELD.into(), serde_json::json!({"version":1}));
        assert!(run.restart_observation().is_err());
    }
}
