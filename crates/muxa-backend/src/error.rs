/// A backend failure. Messages carry operation names, terminal ids and the
/// backend's own error text, never terminal content.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("the connection to the terminal server is closed")]
    Closed,
    #[error("{0}")]
    Invalid(String),
    #[error("{op}: {message}")]
    Failed { op: &'static str, message: String },
}

impl BackendError {
    pub fn failed(op: &'static str, err: impl std::fmt::Display) -> Self {
        BackendError::Failed {
            op,
            message: err.to_string(),
        }
    }
}
