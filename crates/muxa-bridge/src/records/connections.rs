//! A workspace's `[[workspace.connection]]` tables (ADR 0069, 0075): a named
//! secret every agent of the workspace gets as one environment variable. The
//! secret lives in the macOS Keychain, never here.

use std::sync::atomic::AtomicBool;

use serde::Deserialize;

static DEFAULT_IGNORED: AtomicBool = AtomicBool::new(false);

const MAX_NAME: usize = 64;
const MAX_LABEL: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Access {
    /// The configuration says nothing.
    #[default]
    Unset,
    Read,
    Write,
}

/// A connection as configured. `access` and `account` are labels for the
/// user; the token itself decides what the agent can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub name: String,
    /// The environment variable that carries the secret.
    pub env: String,
    pub account: Option<String>,
    pub access: Access,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawConnection {
    name: String,
    env: String,
    account: Option<String>,
    access: Option<String>,
    /// Read and ignored: every agent gets every connection (ADR 0075).
    default: Option<bool>,
}

impl RawConnection {
    /// `taken` are the variables `[workspace.agent] env` sets already.
    pub(super) fn check(self, taken: &[&str]) -> Result<Connection, String> {
        let named = |r: &str| format!("connection {:?}: {r}", self.name);
        if !is_name(&self.name) {
            return Err(format!(
                "connection name {:?} must be 1 to {MAX_NAME} of a-z, 0-9 and -",
                self.name
            ));
        }
        if !is_env_key(&self.env) {
            return Err(named(&format!(
                "env {:?} must be an environment variable name",
                self.env
            )));
        }
        if taken.contains(&self.env.as_str()) {
            return Err(named(&format!(
                "[workspace.agent] env sets {} already; give it to one of them",
                self.env
            )));
        }
        let access = match self.access.as_deref() {
            None => Access::Unset,
            Some("read") => Access::Read,
            Some("write") => Access::Write,
            Some(_) => return Err(named("access must be \"read\" or \"write\"")),
        };
        let account = self
            .account
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty());
        if account
            .as_ref()
            .is_some_and(|a| a.len() > MAX_LABEL || a.chars().any(char::is_control))
        {
            return Err(named(&format!(
                "account must be one line of at most {MAX_LABEL} bytes"
            )));
        }
        if self.default.is_some() {
            super::ignored_once(
                &DEFAULT_IGNORED,
                "default in [[workspace.connection]] is no longer used: every agent of a workspace gets all its connections (ADR 0075); remove it",
            );
        }
        Ok(Connection {
            name: self.name,
            env: self.env,
            account,
            access,
        })
    }
}

/// Checks a workspace's list: names and variables are each used once.
pub(super) fn check_all(
    raw: Vec<RawConnection>,
    taken: &[&str],
) -> Result<Vec<Connection>, String> {
    let mut out: Vec<Connection> = Vec::with_capacity(raw.len());
    for r in raw {
        let c = r.check(taken)?;
        if out.iter().any(|o| o.name == c.name) {
            return Err(format!("connection {:?} appears twice", c.name));
        }
        if let Some(o) = out.iter().find(|o| o.env == c.env) {
            return Err(format!(
                "connections {:?} and {:?} both set {}",
                o.name, c.name, c.env
            ));
        }
        out.push(c);
    }
    Ok(out)
}

pub fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= MAX_NAME
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub(super) fn is_env_key(s: &str) -> bool {
    let mut bytes = s.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}
