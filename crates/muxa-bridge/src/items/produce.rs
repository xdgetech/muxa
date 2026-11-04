//! Makes and resolves a Task's Inbox items from the states its adapters
//! supply (ADR 0033 §2). Never from terminal content (invariant 6).
use muxa_tmux::Session;

use crate::agents::Registry;
use crate::task_records::Config;

/// Blocking. Called after each look at the sessions, by a bridge and by
/// the daemon. Each need makes at most one item, whoever looks first.
pub(crate) fn observe(_sessions: &[Session], _config: &Config, _agents: &Registry) {}
