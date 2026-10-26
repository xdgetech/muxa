//! The normalized entries of one conversation, bounded. Shared by the adapters.

use std::collections::HashMap;

use muxa_protocol::ChatEntry;

/// Entries sent on a reset.
pub const INITIAL_ENTRIES: usize = 200;
/// Entries kept for resends; older ones are dropped.
pub const MAX_ENTRIES: usize = 500;
pub const MAX_TEXT: usize = 64 << 10;
pub const MAX_RESULT: usize = 2 << 10;
pub const MAX_SUMMARY: usize = 200;

#[derive(Default)]
pub struct EntryLog {
    entries: Vec<ChatEntry>,
    index: HashMap<String, usize>,
}

impl EntryLog {
    pub fn get(&self, id: &str) -> Option<&ChatEntry> {
        self.entries.get(*self.index.get(id)?)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.index.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// The newest `INITIAL_ENTRIES`, for a reset or a resync.
    pub fn recent(&self) -> Vec<ChatEntry> {
        self.entries[self.entries.len().saturating_sub(INITIAL_ENTRIES)..].to_vec()
    }

    #[cfg(test)]
    pub fn all(&self) -> &[ChatEntry] {
        &self.entries
    }

    /// Adds `entry`, or replaces the one with its id, and returns it. Returns true
    /// as well when old entries were dropped, so callers can drop what pointed at them.
    pub fn upsert(&mut self, entry: ChatEntry) -> (ChatEntry, bool) {
        match self.index.get(&entry.id) {
            Some(&i) => {
                self.entries[i] = entry.clone();
                (entry, false)
            }
            None => {
                self.index.insert(entry.id.clone(), self.entries.len());
                self.entries.push(entry.clone());
                // Trim in steps, not on every entry.
                let trimmed = self.entries.len() > MAX_ENTRIES + MAX_ENTRIES / 5;
                if trimmed {
                    self.trim();
                }
                (entry, trimmed)
            }
        }
    }

    fn trim(&mut self) {
        let drop = self.entries.len() - MAX_ENTRIES;
        self.entries.drain(..drop);
        self.index = self
            .entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
    }
}

/// At most `max` bytes, cut on a character boundary, with an ellipsis when cut.
pub fn cap(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}
