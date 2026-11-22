//! A per-file lock for the Routine state files: an exclusive `flock` on
//! `.<id>.lock` beside them. The inode stays in place; unlinking it would
//! split ownership between writers.

use std::fs::{File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rustix::fs::{FlockOperation, flock};

const FILE_MODE: u32 = 0o600;
const LOCK_WAIT: Duration = Duration::from_secs(2);
const LOCK_POLL: Duration = Duration::from_millis(5);

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0} is not a regular file")]
    NotAFile(PathBuf),
    #[error("{0} is busy; try again")]
    Busy(String),
}

pub(crate) struct Lock {
    file: File,
}

impl Drop for Lock {
    fn drop(&mut self) {
        if let Err(error) = flock(&self.file, FlockOperation::Unlock) {
            tracing::warn!(error = %error, "cannot release a state lock");
        }
    }
}

impl Lock {
    /// Takes the lock of `id` in `dir`; with `wait`, for up to 2 s.
    pub(crate) fn acquire(dir: &Path, id: &str, wait: bool) -> Result<Self, LockError> {
        let path = dir.join(format!(".{id}.lock"));
        let io = |action| {
            let path = path.clone();
            move |source| LockError::Io {
                action,
                path,
                source,
            }
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(FILE_MODE)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
            .open(&path)
            .map_err(io("open lock"))?;
        if !file.metadata().map_err(io("inspect lock"))?.is_file() {
            return Err(LockError::NotAFile(path));
        }
        let start = Instant::now();
        loop {
            match flock(&file, FlockOperation::NonBlockingLockExclusive) {
                Ok(()) => return Ok(Self { file }),
                Err(rustix::io::Errno::WOULDBLOCK) => {
                    if !wait || start.elapsed() >= LOCK_WAIT {
                        return Err(LockError::Busy(id.to_owned()));
                    }
                    std::thread::sleep(LOCK_POLL);
                }
                Err(error) => return Err(io("lock")(error.into())),
            }
        }
    }
}
