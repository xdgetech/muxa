//! Durable restart and input-delivery phases (ADR 0024).
use serde::{Deserialize, Serialize};

use super::store::ClaimedTask;
use super::{Record, StoreError};

pub const FIELD: &str = "restartOperation";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub version: u32,
    pub id: String,
    pub phase: Phase,
    pub source_run: Option<u32>,
    pub reserved_run: u32,
    pub pane: u32,
    pub source_pid: u32,
    pub worker: Option<String>,
    #[serde(default)]
    pub session: Option<u32>,
    #[serde(default)]
    pub source_started: Option<String>,
    #[serde(default)]
    pub replacement: Option<ProcessIdentity>,
    #[serde(default)]
    pub files: Option<super::restart_files::Manifest>,
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub seen: Option<super::Seen>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProcessIdentity {
    pub pid: u32,
    pub started: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Prepared,
    Dispatching,
    Replaced,
    Delivering,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("task lifecycle operation requires recovery")]
    RecoveryRequired,
    #[error("task lifecycle transition is not permitted")]
    InvalidTransition,
    #[error(transparent)]
    Store(#[from] StoreError),
}

impl Record {
    /// Unknown or malformed intent blocks callers; it is never treated as absent.
    pub fn restart_operation(&self) -> Result<Option<Operation>, Error> {
        let Some(value) = self.extra.get(FIELD) else {
            return Ok(None);
        };
        let op: Operation =
            serde_json::from_value(value.clone()).map_err(|_| Error::RecoveryRequired)?;
        if op.version != 1
            || op.id.is_empty()
            || (op.reserved_run == 0 && !(op.phase == Phase::Delivering && op.source_run.is_none()))
            || (op.phase == Phase::Delivering && op.source_run.unwrap_or(0) != op.reserved_run)
            || op
                .files
                .as_ref()
                .is_some_and(|f| f.version != 1 || f.operation != op.id)
        {
            return Err(Error::RecoveryRequired);
        }
        Ok(Some(op))
    }
}

impl ClaimedTask {
    /// Persist prepared intent before authorizing an external command.
    pub fn prepare_restart(&mut self, op: Operation) -> Result<(), Error> {
        if self.record().extra.contains_key(FIELD) {
            return Err(Error::RecoveryRequired);
        }
        let record = self.record();
        let next = record
            .runs
            .last()
            .map_or(Some(1), |run| run.id.checked_add(1));
        if op.version != 1
            || op.id.is_empty()
            || op.phase != Phase::Prepared
            || Some(op.reserved_run) != next
            || op.source_run != record.open_run().map(|run| run.id)
        {
            return Err(Error::InvalidTransition);
        }
        self.write_operation(Some(op))
    }

    pub fn advance_restart(&mut self, operation: &str, phase: Phase) -> Result<(), Error> {
        let mut op = self
            .record()
            .restart_operation()?
            .ok_or(Error::InvalidTransition)?;
        if op.id != operation
            || !matches!(
                (op.phase, phase),
                (Phase::Prepared, Phase::Dispatching) | (Phase::Dispatching, Phase::Replaced)
            )
        {
            return Err(Error::InvalidTransition);
        }
        op.phase = phase;
        self.write_operation(Some(op))
    }

    /// Only pre-dispatch intent can be cancelled without outcome reconciliation.
    pub fn cancel_prepared_restart(&mut self, operation: &str) -> Result<(), Error> {
        let op = self
            .record()
            .restart_operation()?
            .ok_or(Error::InvalidTransition)?;
        if op.id != operation || op.phase != Phase::Prepared {
            return Err(Error::InvalidTransition);
        }
        self.write_operation(None)
    }

    pub fn write_operation(&mut self, op: Option<Operation>) -> Result<(), Error> {
        let value = op
            .map(serde_json::to_value)
            .transpose()
            .map_err(|_| Error::InvalidTransition)?;
        self.update(move |r| match value {
            Some(value) => {
                r.extra.insert(FIELD.into(), value);
            }
            None => {
                r.extra.remove(FIELD);
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{NewRecord, Store};

    struct Fixture(std::path::PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn fixture() -> (Fixture, Store, Record) {
        let dir = Fixture(
            std::env::temp_dir().join(format!("muxa-lifecycle-{:08x}", getrandom::u32().unwrap())),
        );
        let store = Store::open(&dir.0).unwrap();
        let record = store
            .create(NewRecord {
                title: "fixture".into(),
                workspace: "w".into(),
                ..Default::default()
            })
            .unwrap();
        (dir, store, record)
    }
    fn op() -> Operation {
        Operation {
            version: 1,
            id: "fixture-operation".into(),
            phase: Phase::Prepared,
            source_run: None,
            reserved_run: 1,
            pane: 1,
            source_pid: 1,
            worker: Some("Reviewer".into()),
            session: None,
            source_started: None,
            replacement: None,
            files: None,
            since: 0,
            seen: None,
        }
    }

    #[test]
    fn claimed_restart_excludes_an_independent_writer_before_external_effects() {
        let (_dir, store, record) = fixture();
        let mut claim = store.claim(&record.id, record.rev).unwrap();
        let other = Store::open(store.dir()).unwrap();
        assert!(matches!(
            other.claim(&record.id, record.rev),
            Err(StoreError::Busy(_))
        ));
        claim.prepare_restart(op()).unwrap();
        drop(claim);
        // Owner death releases the lock, but not the durable recovery barrier.
        let current = other.get(&record.id).unwrap();
        assert!(matches!(
            other.update(&record.id, current.rev, |r| r.status =
                super::super::Status::Done),
            Err(StoreError::Busy(_))
        ));
        assert_eq!(other.get(&record.id).unwrap(), current);
    }

    #[test]
    fn dispatch_authorization_survives_reopen_and_cannot_be_cancelled_or_replayed() {
        let (_dir, store, record) = fixture();
        let mut claim = store.claim(&record.id, record.rev).unwrap();
        claim.prepare_restart(op()).unwrap();
        claim
            .advance_restart("fixture-operation", Phase::Dispatching)
            .unwrap();
        let rev = claim.record().rev;
        drop(claim);
        let reopened = Store::open(store.dir()).unwrap();
        let mut recovery = reopened.claim(&record.id, rev).unwrap();
        assert_eq!(
            recovery
                .record()
                .restart_operation()
                .unwrap()
                .unwrap()
                .phase,
            Phase::Dispatching
        );
        assert!(
            recovery
                .cancel_prepared_restart("fixture-operation")
                .is_err()
        );
        assert!(recovery.prepare_restart(op()).is_err());
        assert_eq!(recovery.record().rev, rev);
        assert!(recovery.record().runs.is_empty());
    }

    #[test]
    fn cancelling_prepared_intent_preserves_run_and_assignment_and_advances_revision() {
        let (_dir, store, record) = fixture();
        let mut claim = store.claim(&record.id, record.rev).unwrap();
        claim.prepare_restart(op()).unwrap();
        claim.cancel_prepared_restart("fixture-operation").unwrap();
        assert!(claim.record().restart_operation().unwrap().is_none());
        assert_eq!(claim.record().worker(), None);
        assert_eq!(claim.record().runs, record.runs);
        assert_eq!(claim.record().rev, record.rev + 2);
    }

    #[test]
    fn unknown_operation_blocks_ordinary_writes_without_losing_unknown_bytes() {
        let (_dir, store, record) = fixture();
        let current = store
            .update(&record.id, record.rev, |r| {
                r.extra.insert(
                    FIELD.into(),
                    serde_json::json!({"version": 99, "phase": "future"}),
                );
            })
            .unwrap();
        assert!(current.restart_operation().is_err());
        assert!(matches!(
            store.update(&record.id, current.rev, |_| {}),
            Err(StoreError::Busy(_))
        ));
        assert_eq!(store.get(&record.id).unwrap(), current);
    }
    #[test]
    fn directory_aliases_share_a_claim_and_symlinked_lock_files_are_refused() {
        use std::os::unix::fs::symlink;
        let (dir, store, record) = fixture();
        let alias = dir.0.with_extension("alias");
        symlink(&dir.0, &alias).unwrap();
        let aliased = Store::open(&alias).unwrap();
        let claim = store.claim(&record.id, record.rev).unwrap();
        assert!(matches!(
            aliased.claim(&record.id, record.rev),
            Err(StoreError::Busy(_))
        ));
        drop(claim);
        assert!(aliased.claim(&record.id, record.rev).is_ok());
        std::fs::remove_file(&alias).unwrap();
        let other = store
            .create(NewRecord {
                title: "other".into(),
                workspace: "w".into(),
                ..Default::default()
            })
            .unwrap();
        let untouched = dir.0.join("untouched");
        std::fs::write(&untouched, b"synthetic marker").unwrap();
        symlink(&untouched, dir.0.join(format!(".{}.lock", other.id))).unwrap();
        assert!(matches!(
            store.claim(&other.id, other.rev),
            Err(StoreError::Io { .. })
        ));
        assert_eq!(std::fs::read(untouched).unwrap(), b"synthetic marker");
    }
}
