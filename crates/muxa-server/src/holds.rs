//! Which places muxa's terminals hold (ADR 0102 §3): the place sweep and
//! the disk report never remove a held place.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use muxa_backend::{SessionBackend, TerminalId, TerminalInfo};
use muxa_inbox::Inbox;
use muxa_protocol::v2::{Hold, Place};

use crate::tasks::{DIR, PLACES};

/// The places that terminals hold, and those terminals.
#[derive(Debug, Clone, Default)]
pub struct Holds {
    by_path: HashMap<PathBuf, Hold>,
    pub terminals: Vec<TerminalInfo>,
}

impl Holds {
    pub fn of(&self, path: &Path) -> Option<Hold> {
        self.by_path.get(path).copied()
    }
}

/// Live terminals hold their places. An ended agent holds its places while
/// an open Inbox item names it: the user has not decided yet.
pub async fn holds<B: SessionBackend>(backend: &B, inbox: &Inbox) -> Result<Holds, String> {
    let live = backend.list().await.map_err(|e| e.to_string())?;
    let mut by_path = HashMap::new();
    for t in &live {
        for path in paths_of(t) {
            by_path.insert(path, Hold::Live);
        }
    }
    let mut terminals = live;
    let named: Vec<String> = inbox
        .view()
        .items
        .into_iter()
        .filter_map(|i| i.terminal)
        .collect();
    for id in named {
        if terminals.iter().any(|t| t.id.as_str() == id) {
            continue;
        }
        let Ok(ended) = backend.get(&TerminalId::new(id)).await else {
            continue;
        };
        for path in paths_of(&ended) {
            by_path.entry(path).or_insert(Hold::Ended);
        }
        terminals.push(ended);
    }
    Ok(Holds { by_path, terminals })
}

/// Every place a muxa terminal names: its `muxa.places`, and its `muxa.dir`.
fn paths_of(t: &TerminalInfo) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = places_of(t)
        .into_iter()
        .map(|p| PathBuf::from(p.path))
        .collect();
    if let Some(dir) = t.labels.get(DIR) {
        out.push(PathBuf::from(dir));
    }
    out
}

/// The terminal's `muxa.places`; none when it has no readable label.
pub fn places_of(t: &TerminalInfo) -> Vec<Place> {
    t.labels
        .get(PLACES)
        .and_then(|raw| serde_json::from_str(raw).ok())
        .unwrap_or_default()
}
