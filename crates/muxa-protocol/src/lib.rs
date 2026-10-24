//! The muxa client/bridge wire format: one JSON object per line. See
//! `protocol/PROTOCOL.md`; the shared fixtures in `protocol/fixtures` are the contract.

/// Changes when a message changes incompatibly.
pub const VERSION: u32 = 1;
