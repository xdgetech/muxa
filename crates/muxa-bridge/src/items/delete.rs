//! `delete-inbox` (ADR 0046): removes the items made more than some days ago
//! from the Task item files and the Routines' entries. Protected ones stay.
use muxa_protocol::{ErrorBody, ErrorCode};

use super::store::ItemStore;
use crate::convert::error_body;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The most days `olderThanDays` takes: ten years.
pub const MAX_DAYS: u32 = 3650;
const DAY: u64 = 86_400;

/// Blocking. Deletes the unprotected items made more than `days` days before
/// `now` (epoch seconds). Returns how many it deleted.
pub fn delete(config: &Config, days: u32, now: u64) -> Result<u32, ErrorBody> {
    let cutoff = cutoff(days, now)?;
    let tasks = delete_task_items(config, cutoff)?;
    let entries = crate::routines::inbox::delete_before(config, cutoff)?;
    let deleted = tasks.saturating_add(entries);
    tracing::info!(days, deleted, "inbox items deleted");
    Ok(deleted)
}

fn cutoff(days: u32, now: u64) -> Result<u64, ErrorBody> {
    if !(1..=MAX_DAYS).contains(&days) {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!("olderThanDays must be 1 to {MAX_DAYS}"),
        ));
    }
    Ok(now.saturating_sub(u64::from(days) * DAY))
}

fn delete_task_items(config: &Config, cutoff: u64) -> Result<u32, ErrorBody> {
    let Some(dir) = config.state_dir.as_ref().filter(|d| d.is_dir()) else {
        return Ok(0);
    };
    let store = ItemStore::open(dir).map_err(|e| state_error(&e))?;
    let mut deleted = 0u32;
    for file in store.all().map_err(|e| state_error(&e))? {
        if !file.items.iter().any(|s| s.item.created_at < cutoff) {
            continue;
        }
        let n = store
            .update::<_, StateError>(&file.task, |t| Ok(t.delete_before(cutoff)))
            .map_err(|e| state_error(&e))?;
        deleted = deleted.saturating_add(n);
    }
    Ok(deleted)
}

fn state_error(e: &StateError) -> ErrorBody {
    match e {
        StateError::Lock(crate::records::StoreError::Busy(_)) => error_body(
            ErrorCode::Conflict,
            "an Inbox item is being written; try again",
        ),
        e => {
            tracing::warn!(error = %e, "inbox delete failed");
            error_body(ErrorCode::Internal, e.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cutoff_is_whole_days_before_now() {
        assert_eq!(cutoff(1, 10 * DAY).unwrap(), 9 * DAY);
        assert_eq!(cutoff(5, 2 * DAY).unwrap(), 0);
        assert_eq!(cutoff(0, DAY).unwrap_err().code, ErrorCode::BadRequest);
        assert!(cutoff(MAX_DAYS + 1, DAY).is_err());
    }
}
