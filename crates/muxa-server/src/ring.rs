//! The last output of one channel, by offset, for a resume. Memory only:
//! terminal bytes never reach the disk (invariant 5).

use std::collections::VecDeque;

/// How much sent output a channel keeps for a resume.
pub(crate) const CAPACITY: usize = 256 * 1024;

pub(crate) struct Ring {
    /// The offset of `bytes[0]` since the last paint.
    start: u64,
    bytes: VecDeque<u8>,
    /// The offset of the first byte not yet sent.
    sent: u64,
    /// A returning client's offset, kept until it resumes.
    pinned: Option<u64>,
}

impl Ring {
    pub(crate) fn new() -> Self {
        Ring {
            start: 0,
            bytes: VecDeque::new(),
            sent: 0,
            pinned: None,
        }
    }

    fn end(&self) -> u64 {
        self.start + self.bytes.len() as u64
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        self.bytes.extend(bytes);
    }

    pub(crate) fn unsent(&self) -> u64 {
        self.end() - self.sent
    }

    /// Up to `max` unsent bytes and the offset of the first.
    pub(crate) fn take(&mut self, max: usize) -> (u64, Vec<u8>) {
        let from = usize::try_from(self.sent - self.start).unwrap_or(usize::MAX);
        let n = usize::try_from(self.unsent())
            .unwrap_or(usize::MAX)
            .min(max);
        let bytes = self.bytes.range(from..from + n).copied().collect();
        let offset = self.sent;
        self.sent += n as u64;
        (offset, bytes)
    }

    /// Drops the oldest bytes past [`CAPACITY`]. A connected client keeps
    /// what it was not sent yet; a parked one keeps only what is pinned.
    /// False when output from the last sent offset is gone.
    pub(crate) fn trim(&mut self, connected: bool) -> bool {
        let floor = match (self.pinned, connected) {
            (Some(p), _) => p,
            (None, true) => self.sent,
            (None, false) => self.end(),
        };
        let over = self.bytes.len().saturating_sub(CAPACITY);
        let n = over.min(usize::try_from(floor - self.start).unwrap_or(usize::MAX));
        self.bytes.drain(..n);
        self.start += n as u64;
        self.start <= self.sent
    }

    /// Sends again from `offset` if the buffer still holds it.
    pub(crate) fn rewind(&mut self, offset: u64) -> bool {
        if offset < self.start || offset > self.sent {
            return false;
        }
        self.sent = offset;
        self.pinned = Some(offset);
        true
    }

    pub(crate) fn unpin(&mut self) {
        self.pinned = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connected_ring_keeps_the_last_capacity_of_sent_output() {
        let mut r = Ring::new();
        r.push(&vec![b'a'; CAPACITY]);
        assert_eq!(r.take(usize::MAX).0, 0);
        r.push(b"bc");
        assert!(r.trim(true));
        assert_eq!(r.take(1), (CAPACITY as u64, b"b".to_vec()));
        assert!(r.rewind(2));
        assert!(!r.rewind(1));
        assert!(!r.rewind(CAPACITY as u64 + 5));
    }

    #[test]
    fn a_parked_ring_reports_output_the_client_can_no_longer_get() {
        let mut r = Ring::new();
        r.push(b"hello");
        r.take(usize::MAX);
        r.push(&vec![b'x'; CAPACITY - 5]);
        assert!(r.trim(false));
        r.push(b"y");
        // Offsets 1..5 are still there for a client that missed them.
        assert!(r.trim(false));
        r.push(b"yyyyy");
        assert!(!r.trim(false));
    }

    #[test]
    fn a_pinned_offset_is_kept() {
        let mut r = Ring::new();
        r.push(b"hello");
        r.take(usize::MAX);
        assert!(r.rewind(1));
        r.push(&vec![b'x'; CAPACITY]);
        assert!(r.trim(false));
        assert_eq!(r.take(4), (1, b"ello".to_vec()));
    }
}
