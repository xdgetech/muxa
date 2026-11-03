//! The record files: one `<id>.json` per Task, written atomically. `rev` is
//! the optimistic lock between bridges; a persistent OS-lock inode makes each
//! read-compare-write a single step and supports a wider Task claim.

use std::ffi::OsStr;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rustix::fs::{FlockOperation, flock};

use super::{
    FORMAT_VERSION, MAX_FIELD, MAX_FILE, MAX_GOAL, MAX_RUNS, MAX_TITLE, NewRecord, Record, Status,
    is_valid_id, new_id,
};

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
/// 24-bit ids collide rarely; a run of collisions means something else is wrong.
const ID_ATTEMPTS: usize = 16;
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_POLL: Duration = Duration::from_millis(5);

/// Messages carry ids, paths and field names, never title, goal or note text.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("task {id} changed: it is at rev {current}, not {expected}. Reload it and try again")]
    Conflict {
        id: String,
        expected: u64,
        current: u64,
    },
    #[error("task {0} does not exist")]
    NotFound(String),
    #[error("not a task id; ids look like t-1a2b3c")]
    InvalidId,
    #[error("{field} {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("task {id} is unreadable: {reason}")]
    Unreadable { id: String, reason: String },
    #[error("task {0} is being written by another client; try again")]
    Busy(String),
    #[error("cannot find a free task id after {ID_ATTEMPTS} tries")]
    NoFreeId,
    #[error("no random source for a task id: {0}")]
    Random(#[from] getrandom::Error),
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

fn io_error(action: &'static str, path: &Path) -> impl FnOnce(io::Error) -> StoreError {
    let path = path.to_owned();
    move |source| StoreError::Io {
        action,
        path,
        source,
    }
}

/// The directory of record files.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// `$XDG_STATE_HOME/muxa/tasks`, else `~/.local/state/muxa/tasks`. A
    /// relative `XDG_STATE_HOME` is ignored, as the XDG spec says.
    pub fn default_dir(xdg_state_home: Option<&OsStr>, home: Option<&Path>) -> Option<PathBuf> {
        let base = xdg_state_home
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| home.map(|h| h.join(".local/state")))?;
        Some(base.join("muxa/tasks"))
    }

    /// Opens `dir`, creating it (mode 0700) when it is missing.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let dir = dir.into();
        DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(&dir)
            .map_err(io_error("create", &dir))?;
        let dir = fs::canonicalize(&dir).map_err(io_error("resolve", &dir))?;
        Ok(Self { dir })
    }

    #[cfg(test)]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every readable record, oldest first. Unreadable files are logged by
    /// id and skipped, so one bad file does not hide the others.
    pub fn list(&self) -> Result<Vec<Record>, StoreError> {
        let entries = fs::read_dir(&self.dir).map_err(io_error("read", &self.dir))?;
        let mut records = Vec::new();
        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!(dir = %self.dir.display(), error = %e, "cannot read a task entry");
                    continue;
                }
            };
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            if !is_valid_id(id) {
                continue;
            }
            match self.read(id) {
                Ok(r) => records.push(r),
                Err(e) => tracing::warn!(id, error = %e, "skipping a task record"),
            }
        }
        records.sort_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id)));
        Ok(records)
    }

    pub fn get(&self, id: &str) -> Result<Record, StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::InvalidId);
        }
        self.read(id)
    }

    /// Writes a new record at rev 1, status `open`, under a fresh id.
    pub fn create(&self, new: NewRecord) -> Result<Record, StoreError> {
        let now = now();
        let mut record = Record {
            version: FORMAT_VERSION,
            id: String::new(),
            rev: 1,
            workspace: new.workspace,
            title: new.title,
            goal: new.goal,
            status: Status::Open,
            created_at: now,
            updated_at: now,
            project: new.project,
            repo: new.repo,
            branch: new.branch,
            dir: new.dir,
            session: new.session,
            note: new.note,
            runs: Vec::new(),
            extra: serde_json::Map::new(),
        };
        check(&record)?;
        for _ in 0..ID_ATTEMPTS {
            record.id = new_id()?;
            let tmp = self.write_temp(&record.id, &encode(&record)?)?;
            let target = self.path(&record.id);
            // A hard link never replaces an existing file, unlike rename.
            let linked = fs::hard_link(&tmp, &target);
            remove_temp(&tmp);
            match linked {
                Ok(()) => {
                    sync_directory(&self.dir)?;
                    return Ok(record);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(io_error("write", &target)(e)),
            }
        }
        Err(StoreError::NoFreeId)
    }

    /// Applies `change` to the record if it is still at `expected_rev`, and
    /// writes it at the next rev. `change` cannot alter the id, version,
    /// rev or times; the store sets them.
    pub fn update(
        &self,
        id: &str,
        expected_rev: u64,
        change: impl FnOnce(&mut Record),
    ) -> Result<Record, StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::InvalidId);
        }
        let _lock = Lock::acquire(&self.dir, id, true)?;
        let current = self.read(id)?;
        if current.extra.contains_key("restartOperation") {
            return Err(StoreError::Busy(id.to_owned()));
        }
        self.update_current(current, expected_rev, change)
    }

    /// A nonblocking lifecycle claim. Keep this guard through external effects.
    pub fn claim(&self, id: &str, expected_rev: u64) -> Result<ClaimedTask, StoreError> {
        if !is_valid_id(id) {
            return Err(StoreError::InvalidId);
        }
        let lock = Lock::acquire(&self.dir, id, false)?;
        let record = self.read(id)?;
        if record.rev != expected_rev {
            return Err(StoreError::Conflict {
                id: id.to_owned(),
                expected: expected_rev,
                current: record.rev,
            });
        }
        Ok(ClaimedTask {
            store: self.clone(),
            _lock: lock,
            record,
        })
    }

    fn update_current(
        &self,
        current: Record,
        expected_rev: u64,
        change: impl FnOnce(&mut Record),
    ) -> Result<Record, StoreError> {
        let id = current.id.clone();
        if current.rev != expected_rev {
            return Err(StoreError::Conflict {
                id: id.to_owned(),
                expected: expected_rev,
                current: current.rev,
            });
        }
        let mut next = current.clone();
        change(&mut next);
        next.version = FORMAT_VERSION;
        next.id = current.id;
        next.rev = current.rev + 1;
        next.created_at = current.created_at;
        next.updated_at = now().max(current.updated_at);
        check(&next)?;
        let tmp = self.write_temp(&id, &encode(&next)?)?;
        let target = self.path(&id);
        if let Err(e) = fs::rename(&tmp, &target) {
            remove_temp(&tmp);
            return Err(io_error("write", &target)(e));
        }
        sync_directory(&self.dir)?;
        Ok(next)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// `id` must be valid; callers check it first.
    fn read(&self, id: &str) -> Result<Record, StoreError> {
        let path = self.path(id);
        let file = match fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(StoreError::NotFound(id.to_owned()));
            }
            Err(e) => return Err(io_error("open", &path)(e)),
        };
        let unreadable = |reason: String| StoreError::Unreadable {
            id: id.to_owned(),
            reason,
        };
        let mut bytes = Vec::new();
        file.take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error("read", &path))?;
        if bytes.len() as u64 > MAX_FILE {
            return Err(unreadable(format!("larger than {MAX_FILE} bytes")));
        }
        // serde_json's message can quote file content; keep only its position.
        let record: Record = serde_json::from_slice(&bytes).map_err(|e| {
            unreadable(format!(
                "{:?} error at line {} column {}",
                e.classify(),
                e.line(),
                e.column()
            ))
        })?;
        if record.version != FORMAT_VERSION {
            return Err(unreadable(format!(
                "format version {} is not {FORMAT_VERSION}",
                record.version
            )));
        }
        if record.id != id {
            return Err(unreadable("its id does not match its file name".into()));
        }
        Ok(record)
    }

    /// A new file next to the records, so the rename that follows is atomic.
    fn write_temp(&self, id: &str, bytes: &[u8]) -> Result<PathBuf, StoreError> {
        let tmp = self
            .dir
            .join(format!(".{id}.{:08x}.tmp", getrandom::u32()?));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(FILE_MODE)
            .open(&tmp)
            .map_err(io_error("create", &tmp))?;
        let written = file.write_all(bytes).and_then(|()| file.sync_all());
        if let Err(e) = written {
            remove_temp(&tmp);
            return Err(io_error("write", &tmp)(e));
        }
        Ok(tmp)
    }
}

fn remove_temp(tmp: &Path) {
    if let Err(e) = fs::remove_file(tmp) {
        tracing::warn!(path = %tmp.display(), error = %e, "cannot remove a temporary task file");
    }
}

fn encode(record: &Record) -> Result<Vec<u8>, StoreError> {
    let mut bytes = serde_json::to_vec_pretty(record).map_err(|_| StoreError::Invalid {
        field: "task",
        reason: "cannot be encoded as JSON".into(),
    })?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_FILE {
        return Err(StoreError::Invalid {
            field: "task",
            reason: format!("is larger than {MAX_FILE} bytes"),
        });
    }
    Ok(bytes)
}

/// The caps and shapes every written record meets.
fn check(r: &Record) -> Result<(), StoreError> {
    let invalid = |field, reason: &str| {
        Err(StoreError::Invalid {
            field,
            reason: reason.to_owned(),
        })
    };
    if r.title.trim().is_empty() {
        return invalid("title", "is empty");
    }
    if r.title.chars().count() > MAX_TITLE {
        return invalid("title", &format!("is longer than {MAX_TITLE} characters"));
    }
    if r.title.chars().any(char::is_control) {
        return invalid("title", "has a line break or control character");
    }
    if r.goal.len() > MAX_GOAL {
        return invalid("goal", &format!("is longer than {MAX_GOAL} bytes"));
    }
    if r.workspace.is_empty() {
        return invalid("workspace", "is empty");
    }
    let texts = [
        ("workspace", Some(&r.workspace)),
        ("project", r.project.as_ref()),
        ("branch", r.branch.as_ref()),
        ("session", r.session.as_ref()),
        ("note", r.note.as_ref()),
    ];
    for (field, text) in texts {
        if text.is_some_and(|t| t.len() > MAX_FIELD) {
            return invalid(field, &format!("is longer than {MAX_FIELD} bytes"));
        }
    }
    for (field, path) in [("repo", &r.repo), ("dir", &r.dir)] {
        check_path(field, path.as_deref())?;
    }
    check_runs(r)
}

fn check_path(field: &'static str, path: Option<&Path>) -> Result<(), StoreError> {
    let Some(path) = path else { return Ok(()) };
    let reason = if !path.is_absolute() {
        "is not an absolute path".to_owned()
    } else {
        match path.to_str() {
            None => "is not valid UTF-8".to_owned(),
            Some(p) if p.len() > MAX_FIELD => format!("is longer than {MAX_FIELD} bytes"),
            Some(_) => return Ok(()),
        }
    };
    Err(StoreError::Invalid { field, reason })
}

fn check_runs(r: &Record) -> Result<(), StoreError> {
    if r.runs.len() > MAX_RUNS {
        return Err(StoreError::Invalid {
            field: "runs",
            reason: format!("has more than {MAX_RUNS} entries"),
        });
    }
    for run in &r.runs {
        for (field, text) in [
            ("run agent", &run.agent),
            ("run session", &run.agent_session),
        ] {
            if text.as_ref().is_some_and(|t| t.len() > MAX_FIELD) {
                return Err(StoreError::Invalid {
                    field,
                    reason: format!("is longer than {MAX_FIELD} bytes"),
                });
            }
        }
        check_path("run transcript", run.transcript.as_deref())?;
    }
    Ok(())
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// The inode stays in place; unlinking it would split ownership between writers.
struct Lock {
    _file: File,
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A concurrent spawn can briefly inherit the open description before
        // exec closes it. Normal release must not wait for that child's exec.
        if let Err(error) = flock(&self._file, FlockOperation::Unlock) {
            tracing::warn!(error = %error, "cannot release task lock");
        }
    }
}

impl Lock {
    fn acquire(dir: &Path, id: &str, wait: bool) -> Result<Self, StoreError> {
        let path = dir.join(format!(".{id}.lock"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(FILE_MODE)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&path)
            .map_err(io_error("open lock", &path))?;
        if !file
            .metadata()
            .map_err(io_error("inspect lock", &path))?
            .is_file()
        {
            return Err(StoreError::Invalid {
                field: "lock",
                reason: "is not a regular file".into(),
            });
        }
        let start = Instant::now();
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(Self { _file: file }),
                Err(rustix::io::Errno::WOULDBLOCK) => {
                    if !wait || start.elapsed() >= LOCK_WAIT {
                        return Err(StoreError::Busy(id.to_owned()));
                    }
                    std::thread::sleep(LOCK_POLL);
                }
                Err(error) => return Err(io_error("lock", &path)(error.into())),
            }
        }
    }
}

/// Writes under one OS guard, without reacquiring it through a second descriptor.
pub struct ClaimedTask {
    store: Store,
    _lock: Lock,
    record: Record,
}

impl ClaimedTask {
    pub fn record(&self) -> &Record {
        &self.record
    }

    pub fn update(&mut self, change: impl FnOnce(&mut Record)) -> Result<&Record, StoreError> {
        // Refresh under the guard, retaining revision checks even for internal callers.
        let current = self.store.read(&self.record.id)?;
        self.record = self
            .store
            .update_current(current, self.record.rev, change)?;
        Ok(&self.record)
    }
}

fn sync_directory(dir: &Path) -> Result<(), StoreError> {
    File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(io_error("sync directory", dir))
}
