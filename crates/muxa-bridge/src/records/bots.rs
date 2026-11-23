//! `[[bot]]` in a workspace's `.muxa/routines.toml` (ADR 0082): a named owner
//! of Routines with a Soul and a Memory in `.muxa/bots/<name>/`. Parsing and
//! reading only; the runner gives them to each run.
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::Workspace;
use super::routines::{ReadFail, read_regular};
use super::workers::{self, Worker};

pub const BOTS_DIR: &str = "bots";
pub const SOUL_FILE: &str = "soul.md";
pub const MEMORY_FILE: &str = "memory.md";
pub const DEFAULT_DAILY_ITEMS: u32 = 3;
pub const MAX_DAILY_ITEMS: u32 = 50;
/// Bytes. The Soul and the Memory both go into the run's `.muxa/TASK.md`.
pub const MAX_SOUL: usize = 12 * 1024;
pub const MAX_MEMORY: usize = 16 * 1024;
const MAX_NAME: usize = 64;
const MAX_DESCRIPTION: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bot {
    pub workspace: String,
    /// Its id within the workspace, and the name of its folder.
    pub name: String,
    pub description: Option<String>,
    /// None: the workspace default agent.
    pub worker: Option<String>,
    pub daily_items: u32,
    pub enabled: bool,
    /// The canonical `.muxa/` folder its files must stay in.
    pub dir: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawBot {
    name: String,
    description: Option<String>,
    worker: Option<String>,
    daily_items: Option<u32>,
    enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum BotFileError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("{path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
}

impl RawBot {
    /// Errors name the Bot and the field, never a value.
    pub(super) fn check(
        self,
        workspace: &Workspace,
        dir: &Path,
        workers: &[Worker],
    ) -> Result<Bot, String> {
        name(&self.name)?;
        let at = |field: &str| format!("bot {:?}: {field}", self.name);
        if let Some(d) = &self.description {
            if d.chars().count() > MAX_DESCRIPTION || d.chars().any(char::is_control) {
                return Err(at(&format!(
                    "description must be one line of at most {MAX_DESCRIPTION} characters"
                )));
            }
        }
        let worker = match self.worker.as_deref() {
            None | Some(workers::DEFAULT_WORKER) => None,
            Some(w) => {
                let found = workers
                    .iter()
                    .find(|x| x.name == w)
                    .ok_or_else(|| at("worker is not configured"))?;
                if !found.allows(&workspace.name) {
                    return Err(at("worker is not allowed in its workspace"));
                }
                Some(w.to_owned())
            }
        };
        let daily_items = self.daily_items.unwrap_or(DEFAULT_DAILY_ITEMS);
        if !(1..=MAX_DAILY_ITEMS).contains(&daily_items) {
            return Err(at(&format!("daily_items must be 1 to {MAX_DAILY_ITEMS}")));
        }
        Ok(Bot {
            workspace: workspace.name.clone(),
            name: self.name,
            description: self.description,
            worker,
            daily_items,
            enabled: self.enabled,
            dir: dir.to_owned(),
        })
    }
}

/// A Bot's name is its folder's name: lowercase letters, digits, `-` and
/// `_`, starting with a letter or digit.
pub fn name(value: &str) -> Result<(), String> {
    let ok = !value.is_empty()
        && value.len() <= MAX_NAME
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && value.as_bytes()[0].is_ascii_alphanumeric();
    if ok {
        Ok(())
    } else {
        Err(format!(
            "bot name must be 1 to {MAX_NAME} lowercase letters, digits, - or _, starting with a letter or digit"
        ))
    }
}

impl Bot {
    /// `.muxa/bots/<name>/`.
    pub fn folder(&self) -> PathBuf {
        self.dir.join(BOTS_DIR).join(&self.name)
    }

    /// The Soul as it is now; empty when the file is missing.
    pub fn read_soul(&self) -> Result<String, BotFileError> {
        self.read(SOUL_FILE, MAX_SOUL)
    }

    /// The Memory as it is now; empty when the file is missing.
    pub fn read_memory(&self) -> Result<String, BotFileError> {
        self.read(MEMORY_FILE, MAX_MEMORY)
    }

    /// A symlink may not lead out of `.muxa/`.
    fn read(&self, file: &str, max: usize) -> Result<String, BotFileError> {
        let path = self.folder().join(file);
        let invalid = |reason: String| BotFileError::Invalid {
            path: path.clone(),
            reason,
        };
        let real = match path.canonicalize() {
            Ok(real) => real,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
            Err(source) => {
                return Err(BotFileError::Read {
                    path: path.clone(),
                    source,
                });
            }
        };
        if !real.starts_with(&self.dir) {
            return Err(invalid("leads outside .muxa/".into()));
        }
        let bytes = match read_regular(&real, max) {
            Ok(bytes) => bytes,
            Err(ReadFail::Io(source)) => return Err(BotFileError::Read { path, source }),
            Err(ReadFail::Invalid("is too large")) => {
                return Err(invalid(format!("is larger than {} KiB", max / 1024)));
            }
            Err(ReadFail::Invalid(reason)) => return Err(invalid(reason.into())),
        };
        if bytes.contains(&0) {
            return Err(invalid("has a NUL byte".into()));
        }
        String::from_utf8(bytes).map_err(|_| invalid("is not UTF-8".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_folder_safe() {
        for good in ["triage", "pr-watch", "a1", "x_y"] {
            assert!(name(good).is_ok(), "{good}");
        }
        for bad in ["", "Triage", "-x", "a b", "a/b", "..", "ü", &"a".repeat(65)] {
            assert!(name(bad).is_err(), "{bad}");
        }
    }

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muxa-bots-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn bot(dir: &Path) -> Bot {
        Bot {
            workspace: "w".into(),
            name: "triage".into(),
            description: None,
            worker: None,
            daily_items: 3,
            enabled: true,
            dir: dir.canonicalize().unwrap(),
        }
    }

    #[test]
    fn missing_files_read_as_empty_and_present_ones_as_written() {
        let tmp = temp("read");
        let b = bot(&tmp);
        assert_eq!(b.read_soul().unwrap(), "");
        assert_eq!(b.read_memory().unwrap(), "");
        std::fs::create_dir_all(b.folder()).unwrap();
        std::fs::write(b.folder().join(SOUL_FILE), "I watch PRs.").unwrap();
        assert_eq!(b.read_soul().unwrap(), "I watch PRs.");
        std::fs::write(b.folder().join(MEMORY_FILE), "x".repeat(MAX_MEMORY + 1)).unwrap();
        assert!(b.read_memory().is_err());
    }

    #[test]
    fn a_symlink_out_of_the_folder_is_refused() {
        let tmp = temp("symlink");
        let muxa = tmp.join(".muxa");
        std::fs::create_dir_all(&muxa).unwrap();
        let b = bot(&muxa);
        std::fs::create_dir_all(b.folder()).unwrap();
        let outside = tmp.join("secret.txt");
        std::fs::write(&outside, "secret").unwrap();
        std::os::unix::fs::symlink(&outside, b.folder().join(SOUL_FILE)).unwrap();
        let e = b.read_soul().unwrap_err().to_string();
        assert!(
            e.contains("leads outside") && !e.contains("secret\""),
            "{e}"
        );
    }
}
