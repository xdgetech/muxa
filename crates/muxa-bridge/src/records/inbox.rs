//! A Task's place in the Inbox queue (ADR 0027 §4): Move down and Later, kept
//! in the record so every device sees the same queue. Clients derive the
//! queue from it; the host only stores it.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::Record;

const FIELD: &str = "inbox";
/// The latest Later the host accepts, in seconds from now.
pub const MAX_LATER: u64 = 366 * 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxPlace {
    /// Move down: the host's clock in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
    /// Later: epoch seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<u64>,
    /// Epoch seconds when it was written.
    pub since: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaterError {
    #[error("until {until} is not after now ({now})")]
    Past { until: u64, now: u64 },
    #[error("until {until} is more than a year ahead")]
    TooFar { until: u64 },
}

/// The place an `update-task` asks for. Nothing asked clears it.
pub fn place(
    down: bool,
    until: Option<u64>,
    now_ms: u64,
) -> Result<Option<InboxPlace>, LaterError> {
    let now = now_ms / 1000;
    if let Some(until) = until {
        if until <= now {
            return Err(LaterError::Past { until, now });
        }
        if until - now > MAX_LATER {
            return Err(LaterError::TooFar { until });
        }
    }
    Ok((down || until.is_some()).then_some(InboxPlace {
        order: down.then_some(now_ms),
        until,
        since: now,
    }))
}

/// Epoch milliseconds.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

impl Record {
    /// None also when the field is unreadable: the Task keeps its derived place.
    pub fn inbox(&self) -> Option<InboxPlace> {
        let value = self.extra.get(FIELD)?;
        match serde_json::from_value(value.clone()) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(task = %self.id, error = %e, "unreadable inbox");
                None
            }
        }
    }

    pub fn set_inbox(&mut self, place: Option<InboxPlace>) {
        match place.map(serde_json::to_value) {
            Some(Ok(v)) => {
                self.extra.insert(FIELD.into(), v);
            }
            Some(Err(e)) => tracing::warn!(task = %self.id, error = %e, "cannot encode inbox"),
            None => {
                self.extra.remove(FIELD);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW_MS: u64 = 1_767_300_400_123;
    const NOW: u64 = 1_767_300_400;

    #[test]
    fn down_takes_the_clock_and_later_its_time() {
        assert_eq!(
            place(true, None, NOW_MS),
            Ok(Some(InboxPlace {
                order: Some(NOW_MS),
                until: None,
                since: NOW
            }))
        );
        assert_eq!(
            place(false, Some(NOW + 60), NOW_MS),
            Ok(Some(InboxPlace {
                order: None,
                until: Some(NOW + 60),
                since: NOW
            }))
        );
        assert_eq!(place(false, None, NOW_MS), Ok(None));
    }

    #[test]
    fn later_must_be_ahead_and_within_a_year() {
        assert!(matches!(
            place(false, Some(NOW), NOW_MS),
            Err(LaterError::Past { .. })
        ));
        assert!(matches!(
            place(true, Some(NOW + MAX_LATER + 1), NOW_MS),
            Err(LaterError::TooFar { .. })
        ));
        assert!(place(false, Some(NOW + MAX_LATER), NOW_MS).is_ok());
    }

    #[test]
    fn the_field_round_trips_and_unknown_parts_do_not_break_it() {
        let mut r: Record = serde_json::from_value(serde_json::json!({
            "version": 1, "id": "t-0000aa", "rev": 1, "workspace": "w", "title": "t",
            "goal": "g", "status": "open", "createdAt": 1, "updatedAt": 1
        }))
        .unwrap();
        assert_eq!(r.inbox(), None);
        let p = InboxPlace {
            order: Some(NOW_MS),
            until: None,
            since: NOW,
        };
        r.set_inbox(Some(p));
        assert_eq!(
            serde_json::to_value(&r).unwrap()["inbox"],
            serde_json::json!({"order": NOW_MS, "since": NOW})
        );
        r.extra.insert(
            FIELD.into(),
            serde_json::json!({"order": 5, "since": 1, "pin": true}),
        );
        assert_eq!(r.inbox().and_then(|p| p.order), Some(5));
        r.extra
            .insert(FIELD.into(), serde_json::json!({"order": "soon"}));
        assert_eq!(r.inbox(), None);
        r.set_inbox(None);
        assert!(!r.extra.contains_key(FIELD));
    }
}
