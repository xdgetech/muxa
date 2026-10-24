use crate::PaneId;

/// Errors from talking to tmux. Messages carry ids and tmux's own error text,
/// never pane content.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid tmux id {0:?}")]
    InvalidId(String),
    #[error("unsupported key {0:?}")]
    UnsupportedKey(String),
    #[error("pane {0} not found")]
    NoPane(PaneId),
    #[error("tmux: {message}")]
    Tmux { message: String },
    #[error("not run: an earlier command in the same line failed")]
    Skipped { block: u64 },
    #[error("tmux control client exited: {0}")]
    Exited(String),
    #[error("tmux output line longer than {0} bytes")]
    LineTooLong(usize),
    #[error("command line must be one line")]
    MultiLineCommand,
    #[error("unexpected tmux reply: {0}")]
    Unexpected(String),
    #[error("tmux i/o: {0}")]
    Io(#[from] std::io::Error),
}

impl Error {
    pub(crate) fn tmux(lines: &[Vec<u8>]) -> Self {
        let message = lines
            .iter()
            .map(|l| String::from_utf8_lossy(l))
            .collect::<Vec<_>>()
            .join("; ");
        Error::Tmux { message }
    }

    /// Maps tmux's "can't find pane" to [`Error::NoPane`].
    pub(crate) fn for_pane(self, pane: PaneId) -> Self {
        match self {
            Error::Tmux { message } if message.contains("can't find pane") => Error::NoPane(pane),
            other => other,
        }
    }
}
