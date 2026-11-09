//! `capture`: save the user's input as it is. Saving never starts an agent;
//! with `assist` the input is saved first, so a refused start loses nothing.
use muxa_protocol::{AssistSpec, Capture, CaptureSource, CaptureState, ErrorBody, ErrorCode};

use super::store::Captured;
use crate::convert::error_body;
use crate::task_records::Config;

/// Bytes.
pub const MAX_TEXT: usize = 16 * 1024;
/// Bytes.
const MAX_CLIENT_ID: usize = 64;

pub struct NewCapture {
    pub client_id: String,
    pub text: String,
    pub source: Option<CaptureSource>,
    pub workspace: Option<String>,
    pub assist: Option<AssistSpec>,
}

/// Blocking. The Capture, and whether this call wrote it.
pub fn save(config: &Config, new: NewCapture) -> Result<(Capture, bool), ErrorBody> {
    check(&new)?;
    if let Some(name) = &new.workspace {
        if config.load_workspaces()?.get(name).is_none() {
            return Err(error_body(
                ErrorCode::BadRequest,
                format!("capture: no workspace {name}"),
            ));
        }
    }
    if new.assist.is_some() && new.workspace.is_none() {
        return Err(error_body(
            ErrorCode::BadRequest,
            "capture: assist needs a workspace",
        ));
    }
    let shelf = super::shelf::<Capture>(config)?;
    let at = crate::records::now();
    let captured = shelf
        .capture(&new.client_id, |id| {
            Ok::<_, crate::routines::store::StateError>(Capture {
                id,
                rev: 1,
                client_id: new.client_id.clone(),
                text: new.text.clone(),
                source: new.source.unwrap_or(CaptureSource::App),
                state: CaptureState::Received,
                created_at: at,
                updated_at: at,
                workspace: new.workspace.clone(),
                work: Vec::new(),
            })
        })
        .map_err(|e| super::state_error(&e))?;
    Ok(match captured {
        Captured::New(c) => {
            tracing::info!(capture = %c.id, bytes = c.text.len(), "capture saved");
            (c, true)
        }
        Captured::Known(c) => {
            tracing::info!(capture = %c.id, "capture sent again");
            (c, false)
        }
    })
}

/// Errors name the field only, never the text.
fn check(new: &NewCapture) -> Result<(), ErrorBody> {
    let id_ok = (1..=MAX_CLIENT_ID).contains(&new.client_id.len())
        && new.client_id.bytes().all(|b| b.is_ascii_graphic());
    if !id_ok {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!("capture: clientId must be 1 to {MAX_CLIENT_ID} printable ASCII bytes"),
        ));
    }
    if new.text.trim().is_empty() || new.text.len() > MAX_TEXT {
        return Err(error_body(
            ErrorCode::BadRequest,
            format!("capture: text must be 1 to {MAX_TEXT} bytes"),
        ));
    }
    if new.text.contains('\0') {
        return Err(error_body(
            ErrorCode::BadRequest,
            "capture: text has a NUL byte",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(client_id: &str, text: &str) -> NewCapture {
        NewCapture {
            client_id: client_id.into(),
            text: text.into(),
            source: None,
            workspace: None,
            assist: None,
        }
    }

    #[test]
    fn a_bad_client_id_or_text_is_refused_without_quoting_it() {
        let secret = "SECRET-7f3a";
        for bad in [
            new("", "x"),
            new(&"a".repeat(65), "x"),
            new("has space", "x"),
            new("ok", ""),
            new("ok", "   \n"),
            new("ok", &"x".repeat(MAX_TEXT + 1)),
            new("ok", &format!("{secret}\0")),
        ] {
            let e = check(&bad).unwrap_err();
            assert_eq!(e.code, ErrorCode::BadRequest);
            assert!(!e.message.contains(secret), "{}", e.message);
        }
        assert!(check(&new("5b0e8c1e-3f7a", &"x".repeat(MAX_TEXT))).is_ok());
    }
}
