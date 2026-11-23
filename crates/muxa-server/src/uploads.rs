//! Files from the user's devices into a Task's working place (ADR 0081). The
//! server checks the request; the host writes the file. Their bytes never
//! reach a log.

use std::path::PathBuf;

use base64::Engine;
use muxa_backend::{SessionBackend, TerminalId};
use muxa_protocol::v2::ErrorCode;

use crate::session::{code_of, task_code};
use crate::tasks::{DIR, Host, KIND};

/// The largest file, before base64. A control frame holds 16 MiB.
pub(crate) const MAX_BYTES: usize = 10 * 1024 * 1024;
const MAX_NAME: usize = 255;

type Refusal = (ErrorCode, String);

/// Writes the file for the Task of `terminal`, its agent or one of its shells,
/// and returns the path relative to the working place.
pub(crate) async fn upload<B: SessionBackend, H: Host>(
    backend: &B,
    host: &H,
    terminal: &str,
    name: &str,
    data: &str,
) -> Result<String, Refusal> {
    let name = checked_name(name)?;
    // Base64 is 4/3 the size: refuse a big one before decoding it.
    if data.len() > MAX_BYTES / 3 * 4 + 4 {
        return Err(too_big());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| (ErrorCode::BadRequest, "data is not base64".to_owned()))?;
    if bytes.len() > MAX_BYTES {
        return Err(too_big());
    }
    let dir = task_dir(backend, terminal).await?;
    let size = bytes.len();
    let path = host
        .upload(&dir, &name, bytes)
        .await
        .map_err(|e| (task_code(&e), e.to_string()))?;
    tracing::info!(terminal, bytes = size, "upload");
    Ok(path)
}

fn too_big() -> Refusal {
    (
        ErrorCode::BadRequest,
        format!("a file is at most {} MB", MAX_BYTES / 1024 / 1024),
    )
}

/// The working place of a Task's agent or shell. A Routine run's folder is
/// not one: its output is published, not edited.
async fn task_dir<B: SessionBackend>(backend: &B, terminal: &str) -> Result<PathBuf, Refusal> {
    let t = backend
        .get(&TerminalId::new(terminal.to_owned()))
        .await
        .map_err(|e| (code_of(&e), e.to_string()))?;
    match (t.labels.get(KIND).map(String::as_str), t.labels.get(DIR)) {
        (Some("task"), Some(dir)) => Ok(PathBuf::from(dir)),
        _ => Err((
            ErrorCode::BadRequest,
            format!("{terminal} is not a Task's terminal; files go to a Task"),
        )),
    }
}

/// A file name only: no folder, nothing hidden, no control character.
pub(crate) fn checked_name(name: &str) -> Result<String, Refusal> {
    let bad = |why: &str| Err((ErrorCode::BadRequest, format!("the file name {why}")));
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." {
        return bad("is empty");
    }
    if name.len() > MAX_NAME {
        return bad(&format!("is longer than {MAX_NAME} bytes"));
    }
    if name.contains(['/', '\\']) {
        return bad("names a folder; send the file name only");
    }
    if name.starts_with('.') {
        return bad("starts with a dot");
    }
    if name.chars().any(char::is_control) {
        return bad("has a control character");
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::checked_name;

    #[test]
    fn only_a_plain_file_name_passes() {
        assert_eq!(checked_name(" build.log ").unwrap(), "build.log");
        assert_eq!(
            checked_name("スクリーンショット 1.png").unwrap(),
            "スクリーンショット 1.png"
        );
        for bad in [
            "",
            "..",
            "../x",
            "a/b",
            "a\\b",
            ".env",
            "a\nb",
            &"x".repeat(256),
        ] {
            assert!(checked_name(bad).is_err(), "{bad:?}");
        }
    }
}
