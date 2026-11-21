//! The Reader on protocol v2: the documents, changed files and images of a
//! terminal's folder, its `muxa.dir` label. Reading files belongs to the
//! host through [`Host`]; the session follows each open file and sends it
//! again when it changed.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::PathBuf;
use std::time::Duration;

use muxa_backend::{SessionBackend, TerminalId};
use muxa_protocol::v2::{DiffScope, ErrorCode, ReaderContent, ReaderKind};

use crate::tasks::DIR;

/// How often open files are read again. git status and diff cost more than
/// reading one file, so not every second.
pub(crate) const TICK: Duration = Duration::from_secs(2);

/// A file to open, as the client named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReaderOpen {
    pub kind: ReaderKind,
    pub path: String,
    pub html: bool,
    pub scope: DiffScope,
}

/// One followed file.
pub(crate) struct Open {
    pub dir: PathBuf,
    pub open: ReaderOpen,
    /// A hash of what was last sent.
    pub sent: u64,
}

/// The connection's open files by reader number. They end with it.
#[derive(Default)]
pub(crate) struct Readers {
    pub open: HashMap<u32, Open>,
    next: u32,
}

impl Readers {
    pub fn add(&mut self, dir: PathBuf, open: ReaderOpen, content: &ReaderContent) -> u32 {
        self.next += 1;
        let sent = stamp(content);
        self.open.insert(self.next, Open { dir, open, sent });
        self.next
    }
}

pub(crate) fn stamp(content: &ReaderContent) -> u64 {
    let mut h = DefaultHasher::new();
    (&content.text, &content.data, content.binary).hash(&mut h);
    h.finish()
}

/// The folder of `terminal`: only a Task's agent or a Routine run has one.
pub(crate) async fn folder<B: SessionBackend>(
    backend: &B,
    terminal: &str,
) -> Result<PathBuf, (ErrorCode, String)> {
    let t = backend
        .get(&TerminalId::new(terminal.to_owned()))
        .await
        .map_err(|e| (ErrorCode::NotFound, e.to_string()))?;
    t.labels.get(DIR).map(PathBuf::from).ok_or_else(|| {
        (
            ErrorCode::BadRequest,
            "this terminal has no folder: only a Task's agent or a Routine run has a Reader".into(),
        )
    })
}
