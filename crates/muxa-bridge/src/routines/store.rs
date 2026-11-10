//! One state file per Routine in `<state dir>/routines/`: its schedule
//! state, its open run, its committed output state and its stored Inbox
//! entries. One locked, atomic write makes a publish a single step.
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use muxa_protocol::{InboxEntry, RoutineFailure, RoutineRun, RoutineSuccess};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::records::store::Lock;

pub const FORMAT_VERSION: u32 = 1;
const DIR: &str = "routines";
const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
/// A whole state file, in bytes.
pub const MAX_FILE: u64 = 8 << 20;
/// Entries kept per Routine. Closed entries go first.
pub const KEEP_ENTRIES: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    #[error("state file {0} is unreadable: {1}")]
    Unreadable(String, String),
    #[error("state file {0} is too large")]
    TooLarge(String),
    #[error(transparent)]
    Lock(#[from] crate::records::StoreError),
    #[error("no random source for an entry id: {0}")]
    Random(#[from] getrandom::Error),
}

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> StateError {
    let path = path.to_owned();
    move |source| StateError::Io {
        action,
        path,
        source,
    }
}

/// A stored entry with the host's approval claim, which clients never see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stored {
    #[serde(flatten)]
    pub entry: InboxEntry,
    /// Epoch seconds when an approval began to create its Task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approving: Option<u64>,
}

/// A Routine's state file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineState {
    pub version: u32,
    pub workspace: String,
    pub name: String,
    /// The local date (`YYYY-MM-DD`) of the last scheduled occurrence handled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_occurrence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub running: Option<RoutineRun>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_success: Option<RoutineSuccess>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_failure: Option<RoutineFailure>,
    /// The agent's opaque state from the last successful publish.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Stored>,
    /// The keys of deleted entries, oldest first: a publish skips them as it
    /// skips stored keys (ADR 0046).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<String>,
    /// Fields written by a newer muxa; kept on rewrite.
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl RoutineState {
    pub fn new(workspace: &str, name: &str) -> Self {
        Self {
            version: FORMAT_VERSION,
            workspace: workspace.to_owned(),
            name: name.to_owned(),
            last_occurrence: None,
            running: None,
            last_success: None,
            last_failure: None,
            state: None,
            entries: Vec::new(),
            deleted: Vec::new(),
            extra: serde_json::Map::new(),
        }
    }

    /// Whether the Routine already published this key, kept or deleted.
    pub fn has_key(&self, key: &str) -> bool {
        self.entries.iter().any(|s| s.entry.key == key) || self.deleted.iter().any(|k| k == key)
    }

    /// Deletes the unprotected entries made before `cutoff` (epoch seconds),
    /// keeping their keys. An entry being approved stays. Returns how many.
    pub fn delete_before(&mut self, cutoff: u64) -> u32 {
        let (gone, kept): (Vec<Stored>, Vec<Stored>) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|s| {
                s.entry.created_at < cutoff
                    && s.entry.protected_at.is_none()
                    && s.approving.is_none()
            });
        self.entries = kept;
        self.deleted
            .extend(gone.iter().map(|s| s.entry.key.clone()));
        u32::try_from(gone.len()).unwrap_or(u32::MAX)
    }

    /// Drops the oldest closed entries, then the oldest of all, down to [`KEEP_ENTRIES`].
    pub fn prune(&mut self) {
        use muxa_protocol::Disposition::{Approved, Dismissed};
        while self.entries.len() > KEEP_ENTRIES {
            let closed = self.entries.iter().position(|s| {
                matches!(s.entry.disposition, Approved | Dismissed) && s.approving.is_none()
            });
            self.entries.remove(closed.unwrap_or(0));
        }
        let extra = self.deleted.len().saturating_sub(KEEP_ENTRIES);
        self.deleted.drain(..extra);
    }
}

/// The directory of Routine state files.
#[derive(Debug, Clone)]
pub struct RoutineStore {
    dir: PathBuf,
}

/// `r-` and 16 hex digits of the SHA-256 of workspace and name: a file name
/// that no name can escape.
pub fn file_id(workspace: &str, name: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(workspace.as_bytes());
    hash.update([0]);
    hash.update(name.as_bytes());
    let hex: String = hash.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("r-{hex}")
}

impl RoutineStore {
    /// `<state dir>/routines`, created with mode 0700.
    pub fn open(state_dir: &Path) -> Result<Self, StateError> {
        let dir = state_dir.join(DIR);
        DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(&dir)
            .map_err(io_error("create", &dir))?;
        Ok(Self { dir })
    }

    /// The Routine's state; a fresh one when it has none yet.
    pub fn get(&self, workspace: &str, name: &str) -> Result<RoutineState, StateError> {
        let id = file_id(workspace, name);
        Ok(self
            .read(&id)?
            .unwrap_or_else(|| RoutineState::new(workspace, name)))
    }

    /// Every readable state file. An unreadable one is logged by id and skipped.
    pub fn all(&self) -> Result<Vec<RoutineState>, StateError> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.dir).map_err(io_error("read", &self.dir))? {
            let name = match entry {
                Ok(e) => e.file_name(),
                Err(e) => {
                    tracing::warn!(error = %e, "cannot read a routine state entry");
                    continue;
                }
            };
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if !id.starts_with("r-") {
                continue;
            }
            match self.read(id) {
                Ok(Some(state)) => out.push(state),
                Ok(None) => {}
                Err(e) => tracing::warn!(routine = id, error = %e, "skipping a routine state"),
            }
        }
        Ok(out)
    }

    /// Applies `change` under the Routine's lock and writes the result
    /// atomically. Nothing is written when `change` fails.
    pub fn update<T, E: From<StateError>>(
        &self,
        workspace: &str,
        name: &str,
        change: impl FnOnce(&mut RoutineState) -> Result<T, E>,
    ) -> Result<T, E> {
        let id = file_id(workspace, name);
        let _lock = Lock::acquire(&self.dir, &id, true).map_err(StateError::from)?;
        let mut state = self
            .read(&id)?
            .unwrap_or_else(|| RoutineState::new(workspace, name));
        let out = change(&mut state)?;
        state.version = FORMAT_VERSION;
        state.prune();
        self.write(&id, &state)?;
        Ok(out)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn read(&self, id: &str) -> Result<Option<RoutineState>, StateError> {
        let Some(state) = read_json::<RoutineState>(&self.path(id), id, MAX_FILE)? else {
            return Ok(None);
        };
        if state.version != FORMAT_VERSION {
            return Err(StateError::Unreadable(
                id.to_owned(),
                format!("format version {} is not {FORMAT_VERSION}", state.version),
            ));
        }
        Ok(Some(state))
    }

    fn write(&self, id: &str, state: &RoutineState) -> Result<(), StateError> {
        write_json(&self.dir, id, state, MAX_FILE)
    }
}

/// Reads `<path>`, at most `max` bytes. None when it does not exist. Also
/// the Inbox item store's (ADR 0033).
pub(crate) fn read_json<T: serde::de::DeserializeOwned>(
    path: &Path,
    id: &str,
    max: u64,
) -> Result<Option<T>, StateError> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error("open", path)(e)),
    };
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error("read", path))?;
    if bytes.len() as u64 > max {
        return Err(StateError::TooLarge(id.to_owned()));
    }
    // serde_json's message can quote content; keep only its position.
    serde_json::from_slice(&bytes).map(Some).map_err(|e| {
        StateError::Unreadable(
            id.to_owned(),
            format!(
                "{:?} error at line {} column {}",
                e.classify(),
                e.line(),
                e.column()
            ),
        )
    })
}

/// Writes `<dir>/<id>.json` atomically, mode 0600. Nothing is written past `max` bytes.
pub(crate) fn write_json<T: Serialize>(
    dir: &Path,
    id: &str,
    value: &T,
    max: u64,
) -> Result<(), StateError> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| StateError::Unreadable(id.to_owned(), format!("cannot encode: {e}")))?;
    bytes.push(b'\n');
    if bytes.len() as u64 > max {
        return Err(StateError::TooLarge(id.to_owned()));
    }
    let tmp = dir.join(format!(".{id}.{:08x}.tmp", getrandom::u32()?));
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(&tmp)
        .and_then(|mut f| f.write_all(&bytes).and_then(|()| f.sync_all()));
    let target = dir.join(format!("{id}.json"));
    let renamed = written.and_then(|()| fs::rename(&tmp, &target));
    if let Err(e) = renamed {
        let _ = fs::remove_file(&tmp);
        return Err(io_error("write", &target)(e));
    }
    File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(io_error("sync", dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muxa-routine-store-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_failed_change_writes_nothing_and_a_good_one_is_kept() {
        let dir = temp();
        let store = RoutineStore::open(&dir).unwrap();
        assert_eq!(store.get("w", "r").unwrap().running, None);
        let failed: Result<(), StateError> = store.update("w", "r", |s| {
            s.state = Some(serde_json::json!(1));
            Err(StateError::TooLarge("x".into()))
        });
        assert!(failed.is_err());
        assert!(store.all().unwrap().is_empty());
        store
            .update::<_, StateError>("w", "r", |s| {
                s.state = Some(serde_json::json!({"cursor": 2}));
                Ok(())
            })
            .unwrap();
        assert_eq!(
            store.get("w", "r").unwrap().state,
            Some(serde_json::json!({"cursor": 2}))
        );
        assert_eq!(store.all().unwrap().len(), 1);
        assert_ne!(file_id("w", "r"), file_id("w", "s"));
        assert_ne!(file_id("w", "r"), file_id("r", "w"));
        let mode = fs::metadata(dir.join("routines")).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o700
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
