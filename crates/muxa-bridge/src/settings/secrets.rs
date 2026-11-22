//! Writing connection secrets (ADR 0074). Everything goes through
//! `/usr/bin/security`, as reading does (`connections.rs`), so the items keep
//! the access list a hand-added item has. A secret never reaches a command
//! line, a log or an error: it goes hex-encoded on `security -i`'s stdin.

use std::io::Write;
use std::process::{Command, Stdio};

use muxa_protocol::settings::SecretState;

use crate::connections::{SERVICE, keychain};

const SECURITY: &str = "/usr/bin/security";
/// Bytes. `security -i` reads lines of about 4 KiB; the hex form is twice as
/// long, and a longer line would be split and echoed.
pub const MAX_SECRET: usize = 1800;
/// `security` exits with this when no item matches.
const NOT_FOUND: i32 = 44;
/// `security show-keychain-info` exits with this when the keychain is locked.
const LOCKED: i32 = 36;

/// Where connection secrets live. Tests use [`Fake`]; never the real Keychain.
pub trait Secrets: Send + Sync {
    /// Whether the secret of `account` exists. Never reads the secret.
    fn state(&self, account: &str) -> SecretState;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    /// Removing a secret that does not exist is fine.
    fn delete(&self, account: &str) -> Result<(), String>;
}

/// A secret a client sent: one line, at most [`MAX_SECRET`] bytes. The error
/// never quotes it.
pub fn check(secret: &str) -> Result<(), String> {
    if secret.is_empty() {
        return Err("the secret is empty".into());
    }
    if secret.len() > MAX_SECRET {
        return Err(format!("the secret is longer than {MAX_SECRET} bytes"));
    }
    if secret.chars().any(char::is_control) {
        return Err("the secret must be one line without control characters".into());
    }
    Ok(())
}

/// The host's login Keychain.
pub struct Keychain;

impl Keychain {
    fn status(args: &[&str]) -> Option<i32> {
        Command::new(SECURITY)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .ok()
            .and_then(|s| s.code())
    }
}

/// `security -i` splits its lines on spaces and quotes; the account is quoted,
/// so it may hold neither a quote nor a backslash.
fn quotable(account: &str) -> Result<(), String> {
    if account.contains(['"', '\\']) || account.chars().any(char::is_control) {
        return Err(
            "muxa cannot store a secret for a name with a quote or a backslash; add it on the host with security add-generic-password".into(),
        );
    }
    Ok(())
}

impl Secrets for Keychain {
    fn state(&self, account: &str) -> SecretState {
        if !cfg!(target_os = "macos") {
            return SecretState::Unavailable;
        }
        if Self::status(&["show-keychain-info"]) == Some(LOCKED) {
            return SecretState::Locked;
        }
        match Self::status(&["find-generic-password", "-s", SERVICE, "-a", account]) {
            Some(0) => SecretState::Set,
            Some(NOT_FOUND) => SecretState::Missing,
            _ => SecretState::Locked,
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Err("muxa keeps secrets in the macOS Keychain; this host has none".into());
        }
        check(secret)?;
        quotable(account)?;
        let hex: String = secret.bytes().map(|b| format!("{b:02x}")).collect();
        let line = format!("add-generic-password -U -s {SERVICE} -a \"{account}\" -X {hex}\n");
        let mut child = Command::new(SECURITY)
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot run {SECURITY}: {e}"))?;
        let written = child
            .stdin
            .take()
            .ok_or_else(|| "cannot write to security".to_owned())
            .and_then(|mut stdin| {
                stdin
                    .write_all(line.as_bytes())
                    .map_err(|e| format!("cannot write to security: {e}"))
            });
        let waited = child
            .wait()
            .map_err(|e| format!("security did not finish: {e}"));
        written?;
        waited?;
        // `security -i` exits 0 even when a command failed: read it back.
        match keychain(account) {
            Ok(stored) if stored == secret => Ok(()),
            _ => Err(
                "the Keychain did not take the secret; if it is locked, log in on the host".into(),
            ),
        }
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        if !cfg!(target_os = "macos") {
            return Ok(());
        }
        match Self::status(&["delete-generic-password", "-s", SERVICE, "-a", account]) {
            Some(0) | Some(NOT_FOUND) => Ok(()),
            code => Err(format!(
                "the Keychain refused to remove the secret (security exited with {code:?}); if it is locked, log in on the host"
            )),
        }
    }
}

/// Secrets in memory, for tests.
#[cfg(test)]
#[derive(Default)]
pub struct Fake {
    pub items: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
    /// `set` fails for this account.
    pub refuse: Option<String>,
}

#[cfg(test)]
impl Secrets for Fake {
    fn state(&self, account: &str) -> SecretState {
        match self.items.lock().unwrap().contains_key(account) {
            true => SecretState::Set,
            false => SecretState::Missing,
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        check(secret)?;
        if self.refuse.as_deref() == Some(account) {
            return Err("the fake Keychain refused".into());
        }
        self.items
            .lock()
            .unwrap()
            .insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        self.items.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_is_one_short_line_and_errors_never_quote_it() {
        assert!(check("ghp_abc").is_ok());
        for bad in ["", "two\nlines", &"x".repeat(MAX_SECRET + 1)] {
            let e = check(bad).unwrap_err();
            assert!(bad.is_empty() || !e.contains(bad), "{e}");
        }
        assert!(quotable("Company/linear").is_ok());
        assert!(quotable("Bad \"name\"/x").is_err());
    }
}
