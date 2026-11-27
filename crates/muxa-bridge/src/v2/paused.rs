//! `<state dir>/paused.json`: the paused Tasks (ADR 0104). The file is
//! written before memory changes, so a restarted daemon lists the same ones.

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use muxa_protocol::v2::Paused;
use muxa_server::{MemoryPaused, TaskError};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::records::lock::Lock;
use crate::routines::store::{StateError, read_json, write_json};

const ID: &str = "paused";
const VERSION: u32 = 1;
const DIR_MODE: u32 = 0o700;
/// Labels and ids only: room for thousands of paused Tasks.
const MAX_FILE: u64 = 4 << 20;

#[derive(Debug, Default, Serialize, Deserialize)]
struct PausedFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    paused: Vec<Paused>,
}

/// The paused Tasks, in memory and in the state dir. Without a state dir
/// they are kept in memory only.
pub struct PausedStore {
    dir: Option<PathBuf>,
    memory: MemoryPaused,
    /// Why the file could not be read; then nothing is written over it.
    broken: Option<String>,
}

impl PausedStore {
    /// Blocking. A file that cannot be read is left as it is, and pausing
    /// is refused until the user moves it away.
    pub fn open(state_dir: Option<&Path>) -> Self {
        let (list, broken) = match state_dir.map(read) {
            Some(Ok(list)) => (list, None),
            Some(Err(e)) => {
                tracing::error!(error = %e, "paused Tasks cannot be read; pausing is off");
                (Vec::new(), Some(e.to_string()))
            }
            None => (Vec::new(), None),
        };
        PausedStore {
            dir: state_dir.map(Path::to_owned),
            memory: MemoryPaused::new(list),
            broken,
        }
    }

    pub fn list(&self) -> Vec<Paused> {
        self.memory.list()
    }

    pub fn changes(&self) -> watch::Receiver<u64> {
        self.memory.changes()
    }

    /// Blocking, under the file's lock.
    pub fn keep(&self, paused: Paused) -> Result<(), TaskError> {
        self.writable()?;
        self.memory.keep(paused, |list| self.save(list))
    }

    /// Blocking, under the file's lock.
    pub fn remove(&self, id: &str) -> Result<(), TaskError> {
        self.writable()?;
        self.memory.remove(id, |list| self.save(list))
    }

    fn writable(&self) -> Result<(), TaskError> {
        match &self.broken {
            Some(why) => Err(TaskError::Failed(format!(
                "{why}; move paused.json out of the state dir to pause Tasks again"
            ))),
            None => Ok(()),
        }
    }

    fn save(&self, list: &[Paused]) -> Result<(), TaskError> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        write(dir, list)
            .map_err(|e| TaskError::Failed(format!("the paused Tasks were not kept: {e}")))
    }
}

fn read(state_dir: &Path) -> Result<Vec<Paused>, StateError> {
    let file: Option<PausedFile> = read_json(&state_dir.join(format!("{ID}.json")), ID, MAX_FILE)?;
    Ok(file.map(|f| f.paused).unwrap_or_default())
}

fn write(state_dir: &Path, list: &[Paused]) -> Result<(), StateError> {
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(state_dir)
        .map_err(|source| StateError::Io {
            action: "create",
            path: state_dir.to_owned(),
            source,
        })?;
    let _lock = Lock::acquire(state_dir, ID, true)?;
    let file = PausedFile {
        version: VERSION,
        paused: list.to_vec(),
    };
    write_json(state_dir, ID, &file, MAX_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::v2::Terminal;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muxa-paused-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn paused(id: &str) -> Paused {
        Paused {
            terminal: Terminal {
                id: id.into(),
                name: Some("fix".into()),
                live: false,
                status: None,
                labels: [("muxa.dir".to_owned(), "/work/fix".to_owned())].into(),
                cols: 80,
                rows: 24,
            },
            paused_at: 1_700_000_000,
            task_loop: None,
        }
    }

    #[test]
    fn a_new_store_lists_what_the_last_one_kept() {
        let dir = temp();
        let first = PausedStore::open(Some(&dir));
        first.keep(paused("t1")).unwrap();
        first.keep(paused("t2")).unwrap();
        first.remove("t1").unwrap();

        let again = PausedStore::open(Some(&dir));
        assert_eq!(again.list(), [paused("t2")]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unreadable_file_stays_and_pausing_is_refused() {
        let dir = temp();
        let file = dir.join("paused.json");
        std::fs::write(&file, "not json").unwrap();
        let store = PausedStore::open(Some(&dir));
        assert!(store.list().is_empty());
        assert!(store.keep(paused("t1")).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "not json");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
