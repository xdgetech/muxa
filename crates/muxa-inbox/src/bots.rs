//! What the Inbox keeps per Bot (ADR 0082): how many items it added today
//! and how its items ended, so the user can see whether it helps.

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
    /// Counts one more item on `day`; a new day starts at zero.
    pub fn count(&mut self, day: &str) {
        if self.day != day {
            self.day = day.to_owned();
            self.added = 0;
        }
        self.added = self.added.saturating_add(1);
    }
}

pub(crate) fn key(workspace: &str, bot: &str) -> String {
    format!("{workspace}/{bot}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_counts_with_no_limit_and_the_next_day_starts_over() {
        let mut l = Ledger::default();
        for _ in 0..60 {
            l.count("2026-11-23");
        }
        assert_eq!(l.added, 60);
        l.count("2026-11-24");
        assert_eq!(l.added, 1);
    }
}
