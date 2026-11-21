/// Errors from talking to herdr-core. Messages carry method names, ids and
/// herdr-core's own error text, never terminal content.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not start {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
    #[error("herdr-core {method}: {message} (code {code})")]
    Rpc {
        method: String,
        code: i64,
        message: String,
    },
    #[error("herdr-core speaks protocol generation {found}, muxa needs {wanted}")]
    Generation { found: u64, wanted: u64 },
    #[error("herdr-core lacks the feature {0:?}")]
    MissingFeature(String),
    #[error("invalid terminal name {0:?}: use 1-32 of a-z 0-9 - _ .")]
    InvalidName(String),
    #[error("frame payload of {0} bytes is over the 16 MiB limit")]
    FrameTooLarge(usize),
    #[error("unknown frame kind {0}")]
    UnknownKind(u8),
    #[error("unexpected reply to {method}: {detail}")]
    Unexpected { method: String, detail: String },
    #[error("the connection to herdr-core is closed")]
    Closed,
    #[error("herdr-core i/o: {0}")]
    Io(#[from] std::io::Error),
    #[error("herdr-core json: {0}")]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub(crate) fn unexpected(method: &str, detail: impl Into<String>) -> Self {
        Error::Unexpected {
            method: method.to_owned(),
            detail: detail.into(),
        }
    }
}
