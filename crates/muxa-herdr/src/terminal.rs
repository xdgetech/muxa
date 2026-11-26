//! Typed views of herdr-core terminals. Screen text is never deserialized, so
//! a `Terminal` can be logged without carrying terminal content.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::Error;

/// A terminal name herdr-core accepts: 1-32 of `a-z 0-9 - _ .`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(String);

impl Name {
    pub fn new(name: &str) -> Result<Self, Error> {
        let ok = (1..=32).contains(&name.len())
            && name.bytes().all(|b| {
                b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_' | b'.')
            });
        if ok {
            Ok(Name(name.to_owned()))
        } else {
            Err(Error::InvalidName(name.to_owned()))
        }
    }

    /// The closest name herdr-core accepts to a free title such as a Task's
    /// "Fix login" or a branch `feature/wbb-2319`: lowercase, other characters
    /// as `-`, at most 32. None when nothing is left.
    pub fn from_title(title: &str) -> Option<Self> {
        let mut out = String::new();
        for c in title.chars().flat_map(char::to_lowercase) {
            let keep = c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.');
            if keep {
                out.push(c);
            } else if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        }
        out.truncate(32);
        let out = out.trim_end_matches('-');
        Name::new(out).ok()
    }

    /// This name, else the first `<name>-2`, `-3`… that `taken` refuses,
    /// cut so the number stays within 32: herdr-core refuses a name a live
    /// terminal has, and a long Task title and its shell's `-sh` both cut to
    /// the same 32.
    pub fn free(&self, taken: impl Fn(&str) -> bool) -> Self {
        if !taken(&self.0) {
            return self.clone();
        }
        (2u32..)
            .map(|n| {
                let suffix = format!("-{n}");
                let mut base = self.0.clone();
                base.truncate(32 - suffix.len());
                Name(format!("{}{suffix}", base.trim_end_matches('-')))
            })
            .find(|name| !taken(&name.0))
            .unwrap_or_else(|| self.clone())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a program reported with OSC 7501. The words are the program's own.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Status {
    pub state: String,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
}

/// One terminal's facts. `address` (`t7`, `box/t7`) is its id.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Terminal {
    pub address: String,
    #[serde(default)]
    pub name: Option<String>,
    /// `live` or `finished`.
    pub state: String,
    #[serde(default)]
    pub status: Option<Status>,
    #[serde(default)]
    pub reported: bool,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    #[serde(default)]
    pub cols: u16,
    #[serde(default)]
    pub rows: u16,
    #[serde(default)]
    pub turn: u64,
    #[serde(default)]
    pub exit_code: Option<i64>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub pid: Option<u32>,
    /// When the OSC 7501 state last changed, in ms since the Unix epoch.
    #[serde(default)]
    pub status_ms: Option<u64>,
}

impl Terminal {
    pub fn is_live(&self) -> bool {
        self.state == "live"
    }
}

/// How to start a program in a new terminal.
#[derive(Debug, Clone, Default)]
pub struct RunSpec {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub name: Option<Name>,
    pub labels: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    /// Replace a lost program with a shell in place.
    pub keep: bool,
}

impl RunSpec {
    pub fn new(argv: Vec<String>) -> Self {
        RunSpec {
            argv,
            ..RunSpec::default()
        }
    }

    /// The `terminal.run` params. muxa starts terminals for itself: they have
    /// no parent and send no notes into another terminal.
    pub(crate) fn params(&self) -> Value {
        let mut p = Map::new();
        p.insert("argv".into(), json!(self.argv));
        if let Some(cwd) = &self.cwd {
            p.insert("cwd".into(), json!(cwd));
        }
        if let Some(name) = &self.name {
            p.insert("name".into(), json!(name.as_str()));
        }
        if !self.labels.is_empty() {
            p.insert("labels".into(), json!(self.labels));
        }
        if !self.env.is_empty() {
            p.insert("env".into(), json!(self.env));
        }
        p.insert("keep".into(), json!(self.keep));
        p.insert("no_parent".into(), json!(true));
        p.insert("no_notify".into(), json!(true));
        Value::Object(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_a_number_within_32() {
        let long = Name::from_title("Mac: ⌘D Split Pane still does nothing-sh").unwrap();
        assert_eq!(long.as_str(), "mac-d-split-pane-still-does-noth");
        let taken = [
            "mac-d-split-pane-still-does-noth",
            "mac-d-split-pane-still-does-no-2",
        ];
        let free = long.free(|n| taken.contains(&n));
        assert_eq!(free.as_str(), "mac-d-split-pane-still-does-no-3");
        assert!(Name::new(free.as_str()).is_ok());
        let short = Name::new("fix-sh").unwrap();
        assert_eq!(short.free(|n| n == "fix-sh").as_str(), "fix-sh-2");
        assert_eq!(short.free(|_| false).as_str(), "fix-sh");
    }

    #[test]
    fn names_follow_herdr_core_rules() {
        assert!(Name::new("lisk-app.review_1").is_ok());
        for bad in ["", "Upper", "has space", "slash/no", &"x".repeat(33)] {
            assert!(Name::new(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn titles_become_names() {
        let name = |t: &str| Name::from_title(t).map(|n| n.as_str().to_owned());
        assert_eq!(
            name("feature/wbb-2319").as_deref(),
            Some("feature-wbb-2319")
        );
        assert_eq!(
            name("Fix the Login  bug!").as_deref(),
            Some("fix-the-login-bug")
        );
        assert_eq!(
            name("lisk-app.review_1").as_deref(),
            Some("lisk-app.review_1")
        );
        assert!(name(&"a b".repeat(20)).is_some_and(|n| n.len() <= 32 && !n.ends_with('-')));
        assert_eq!(
            name("ab cdefghijklmnopqrstuvwxyz01234 x").as_deref(),
            Some("ab-cdefghijklmnopqrstuvwxyz01234")
        );
        assert_eq!(name("監査の返信"), None);
        assert_eq!(name(""), None);
    }

    #[test]
    fn run_params_never_make_a_child_terminal() {
        let mut spec = RunSpec::new(vec!["claude".into()]);
        spec.name = Some(Name::new("t-1").unwrap());
        spec.labels.insert("muxa.task".into(), "42".into());
        let p = spec.params();
        assert_eq!(p["no_parent"], json!(true));
        assert_eq!(p["no_notify"], json!(true));
        assert_eq!(p["name"], json!("t-1"));
        assert_eq!(p["labels"]["muxa.task"], json!("42"));
        assert!(p.get("cwd").is_none());
    }

    #[test]
    fn a_terminal_ignores_its_screen_text() {
        let t: Terminal = serde_json::from_value(json!({
            "address": "t7", "state": "live", "labels": {}, "argv": ["sh"], "cwd": "/",
            "created_ms": 1, "screen": "secret output", "cols": 80, "rows": 24,
            "status": {"state": "working", "app": "claude-code", "msg": "Reading a file"}
        }))
        .unwrap();
        assert_eq!(t.status.as_ref().map(|s| s.state.as_str()), Some("working"));
        assert!(!format!("{t:?}").contains("secret"));
        assert!(!format!("{t:?}").contains("Reading a file"));
    }
}
