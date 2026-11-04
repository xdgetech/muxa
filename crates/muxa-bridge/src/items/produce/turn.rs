//! When the Task's current turn started: the `since` of the `running` state
//! last seen, kept in the Task's item file. A folder outside git has no diff,
//! so a turn's documents are the files changed since then (ADR 0033 §4a).
use muxa_tmux::{AgentState, AgentStatus};

use super::super::store::{ItemStore, TaskItems};
use crate::records::Record;
use crate::routines::store::StateError;

const FIELD: &str = "turnStart";

/// Keeps the start of a running turn; writes only when it is a new turn.
pub(super) fn note(
    store: &ItemStore,
    record: &Record,
    agent: Option<AgentStatus>,
) -> Result<(), StateError> {
    let Some(since) = running_since(agent) else {
        return Ok(());
    };
    if started(&store.get(&record.id)?) == Some(since) {
        return Ok(());
    }
    store.update::<_, StateError>(&record.id, |t| {
        t.extra.insert(FIELD.into(), since.into());
        Ok(())
    })
}

/// The start of the turn that ended at `ended`: the kept start, else the
/// run's start `run_start`. None when neither is known or both are after `ended`.
pub(super) fn start(items: &TaskItems, run_start: Option<u64>, ended: u64) -> Option<u64> {
    started(items)
        .filter(|&s| s <= ended)
        .or(run_start)
        .filter(|&s| s <= ended)
}

pub(super) fn running_since(agent: Option<AgentStatus>) -> Option<u64> {
    agent
        .filter(|s| s.state == AgentState::Running)
        .and_then(|s| s.since)
}

fn started(items: &TaskItems) -> Option<u64> {
    items.extra.get(FIELD)?.as_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kept_start_counts_only_before_the_end() {
        let mut t = TaskItems::new("t-000001");
        assert_eq!(start(&t, Some(50), 100), Some(50), "the run's start");
        assert_eq!(start(&t, None, 100), None);
        t.extra.insert(FIELD.into(), 80.into());
        assert_eq!(start(&t, Some(50), 100), Some(80));
        assert_eq!(start(&t, Some(50), 70), Some(50), "a newer turn's start");
        assert_eq!(start(&t, Some(50), 40), None);
    }
}
