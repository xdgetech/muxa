use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// Free-form `key=value` tags a backend stores with a terminal and never
/// interprets. muxa keeps Workspace and Task ids here.
pub type Labels = BTreeMap<String, String>;

/// A backend's id for a terminal (`t7` in herdr-core). Opaque to muxa.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TerminalId(String);

impl TerminalId {
    pub fn new(id: impl Into<String>) -> Self {
        TerminalId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

/// What a program reported with OSC 7501: `idle`, `working`, `done`,
/// `blocked` or `error`, in the program's own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatus {
    pub state: String,
    pub app: Option<String>,
}

/// How a program ended: `exited`, `killed`, `server_stop`, ...
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exit {
    pub reason: String,
    pub code: Option<i64>,
    pub signal: Option<i64>,
}

/// A terminal's facts. Never its screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInfo {
    pub id: TerminalId,
    pub name: Option<String>,
    pub live: bool,
    pub status: Option<ProgramStatus>,
    pub labels: Labels,
    pub size: Size,
    pub exit: Option<Exit>,
}

/// How to start a program.
#[derive(Debug, Clone, Default)]
pub struct Launch {
    pub argv: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// A human-readable name, if the backend supports one. The backend may
    /// shorten it, and adds a number when a live terminal has it.
    pub name: Option<String>,
    pub labels: Labels,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamEvent {
    /// Raw terminal bytes; the first ones are the first paint.
    Output(Vec<u8>),
    Resized(Size),
    /// The stream is over: the program ended, the viewer detached, or the
    /// connection was lost (`reason` = `connection_lost`).
    Closed(Exit),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Created(TerminalInfo),
    Status {
        id: TerminalId,
        status: Option<ProgramStatus>,
    },
    Resized {
        id: TerminalId,
        size: Size,
    },
    Ended {
        id: TerminalId,
        exit: Exit,
    },
    Removed {
        id: TerminalId,
    },
    /// The subscriber fell behind: this list replaces what it knew.
    Resync(Vec<TerminalInfo>),
}
