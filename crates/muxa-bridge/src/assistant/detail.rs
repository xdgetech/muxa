//! `get-work` (ADR 0048 §9): one Work item with every Capture it links, sorted
//! or not, and its requests. `inbox-list` lists only unsorted Captures.
use muxa_protocol::{AssistantRequest, Capture, ErrorBody, ErrorCode, WorkItem};

use super::store::{Kind, Shelf};
use crate::convert::error_body;
use crate::task_records::Config;

/// What `work-detail` carries.
#[derive(Debug)]
pub struct Detail {
    /// With its derived state.
    pub entry: WorkItem,
    /// In `entry.captures` order. Ids with no file are left out.
    pub captures: Vec<Capture>,
    /// In `entry.requests` order, with their delivery derived again.
    pub requests: Vec<AssistantRequest>,
}

/// Blocking. An unknown or malformed id is `bad-request`.
pub fn detail(config: &Config, work: &str) -> Result<Detail, ErrorBody> {
    let missing = || error_body(ErrorCode::BadRequest, format!("no Work item {work}"));
    let has_store = config.state_dir.as_ref().is_some_and(|d| d.is_dir());
    if !has_store || !is_id::<WorkItem>(work) {
        return Err(missing());
    }
    let mut entry = super::shelf::<WorkItem>(config)?
        .get(work)
        .map_err(|e| super::state_error(&e))?
        .ok_or_else(missing)?;
    let captures = linked(&super::shelf::<Capture>(config)?, &entry.captures);
    let requests = requests_of(config, &entry)?;
    super::derive::states(config, std::slice::from_mut(&mut entry), &requests);
    tracing::debug!(
        work = %entry.id,
        captures = captures.len(),
        requests = requests.len(),
        "work item read"
    );
    Ok(Detail {
        entry,
        captures,
        requests,
    })
}

/// Blocking. The item's requests in its order, with their delivery derived again.
pub(super) fn requests_of(
    config: &Config,
    item: &WorkItem,
) -> Result<Vec<AssistantRequest>, ErrorBody> {
    let mut requests = linked(&super::shelf::<AssistantRequest>(config)?, &item.requests);
    super::delivery::refresh(config, &mut requests, crate::records::now());
    Ok(requests)
}

/// The records `ids` names, in that order. A missing one is skipped; an
/// unreadable one is logged by id and skipped, as [`Shelf::all`] does.
fn linked<T: Kind>(shelf: &Shelf<T>, ids: &[String]) -> Vec<T> {
    ids.iter()
        .filter_map(|id| match shelf.get(id) {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(id = %id, error = %e, "skipping an assistant file");
                None
            }
        })
        .collect()
}

/// The kind's prefix and 8 hex digits: the only ids the store has files for.
fn is_id<T: Kind>(id: &str) -> bool {
    id.strip_prefix(T::PREFIX)
        .is_some_and(|h| h.len() == 8 && h.bytes().all(|b| b.is_ascii_hexdigit()))
}
