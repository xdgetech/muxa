//! What the Inbox keeps per Bot (ADR 0082): how many items it added today,
//! so a Bot stays within its daily limit, and how its items ended, so the
//! user can see whether it helps.

use serde::{Deserialize, Serialize};

/// One Bot's day and counts, keyed by `<workspace>/<bot>` in the book.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ledger {
    /// The host's local date the count is for, `YYYY-MM-DD`.
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub added: u32,
    #[serde(default)]
    pub done: u32,
    #[serde(default)]
    pub deleted: u32,
}

impl Ledger {
    /// Whether one more item fits `limit` on `day`; a new day starts at zero.
    pub fn take(&mut self, day: &str, limit: u32) -> bool {
        if self.day != day {
            self.day = day.to_owned();
            self.added = 0;
        }
        if self.added >= limit {
            return false;
        }
        self.added += 1;
        true
    }
}

pub(crate) fn key(workspace: &str, bot: &str) -> String {
    format!("{workspace}/{bot}")
}

/// The key of the one item that stands for a Bot's items past its limit.
pub(crate) fn more_key(workspace: &str, bot: &str, day: &str) -> String {
    format!("bot-more:{workspace}/{bot}:{day}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_allows_the_limit_and_the_next_day_starts_over() {
        let mut l = Ledger::default();
        assert!(l.take("2026-11-23", 2));
        assert!(l.take("2026-11-23", 2));
        assert!(!l.take("2026-11-23", 2));
        assert!(l.take("2026-11-24", 2));
        assert_eq!(l.added, 1);
    }
}
