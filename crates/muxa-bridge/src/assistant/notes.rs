//! A delegate's notes, `<root>/.muxa/assistant/NOTES.md` (ADR 0047 §1): the
//! snapshot a run reads, and the checked write of a proposal. Each folder is
//! opened without following a symlink. Errors name paths and sizes, never text.
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rustix::fs::{AtFlags, Mode, OFlags};
use rustix::io::Errno;
use sha2::{Digest, Sha256};

/// Relative to a workspace root, and to a run folder for the copy.
pub const NOTES: &str = ".muxa/assistant/NOTES.md";
const FOLDERS: [&str; 2] = [".muxa", "assistant"];
const NAME: &str = "NOTES.md";
/// Bytes.
pub const MAX_NOTES: usize = 16 << 10;

/// The check of the base and the rename happen as one step in this process.
static WRITES: Mutex<()> = Mutex::new(());

/// The notes a run reads, and their hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub text: String,
    /// Lowercase hex SHA-256 of the file's bytes; of no bytes without a file.
    pub base: String,
}

#[derive(Debug, thiserror::Error)]
pub enum NotesError {
    #[error("{path} is {size} bytes; notes are at most 16 KiB. Shorten it, then ask again.")]
    TooLarge { path: PathBuf, size: u64 },
    #[error("{path} ({size} bytes) holds a NUL byte; notes are UTF-8 text.")]
    Nul { path: PathBuf, size: u64 },
    #[error("{path} ({size} bytes) is not UTF-8 text.")]
    NotUtf8 { path: PathBuf, size: u64 },
    #[error("{path} is a symlink or not a regular file; notes must be a regular file.")]
    NotRegular { path: PathBuf },
    #[error("{path} changed since the run read it; ask again.")]
    Changed { path: PathBuf },
    #[error("cannot {action} {path}: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
}

pub fn base_of(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Blocking. The live notes of the workspace at `root`; empty without a file.
pub fn read(root: &Path) -> Result<Snapshot, NotesError> {
    match folder(root, false)? {
        Some(dir) => read_in(&dir, &root.join(NOTES)),
        None => Ok(empty()),
    }
}

/// Blocking. Copies the live notes into the run folder `dir`, as long as
/// they are still the ones whose hash the run recorded as `base`.
pub fn copy_into(root: &Path, dir: &Path, base: &str) -> Result<(), NotesError> {
    let live = read(root)?;
    if live.base != base {
        return Err(NotesError::Changed {
            path: root.join(NOTES),
        });
    }
    let stage = folder(dir, true)?.ok_or_else(|| missing(dir))?;
    write_in(&stage, &dir.join(NOTES), live.text.as_bytes())
}

/// Blocking. Replaces the live notes with `text` while they still hash to
/// `base`; creates `.muxa/assistant` when missing.
pub fn apply(root: &Path, text: &str, base: &str) -> Result<(), NotesError> {
    let path = root.join(NOTES);
    if text.len() > MAX_NOTES || text.contains('\0') {
        return Err(NotesError::TooLarge {
            path,
            size: text.len() as u64,
        });
    }
    let _one = WRITES.lock().unwrap_or_else(|e| e.into_inner());
    let dir = folder(root, true)?.ok_or_else(|| missing(root))?;
    let live = match read_in(&dir, &path) {
        Ok(live) => live,
        Err(e @ NotesError::Io { .. }) => return Err(e),
        // A run only ever reads a valid file: an invalid one has changed.
        Err(_) => return Err(NotesError::Changed { path }),
    };
    if live.base != base {
        return Err(NotesError::Changed { path });
    }
    write_in(&dir, &path, text.as_bytes())
}

fn empty() -> Snapshot {
    Snapshot {
        text: String::new(),
        base: base_of(b""),
    }
}

fn missing(dir: &Path) -> NotesError {
    NotesError::NotRegular {
        path: dir.join(FOLDERS.join("/")),
    }
}

/// `<base>/.muxa/assistant`, opened. None when a folder is missing and
/// `create` is false.
fn folder(base: &Path, create: bool) -> Result<Option<OwnedFd>, NotesError> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let mut dir = rustix::fs::open(base, flags, Mode::empty()).map_err(io("open", base))?;
    let mut path = base.to_path_buf();
    for name in FOLDERS {
        path.push(name);
        if create {
            let mode = Mode::RWXU | Mode::RGRP | Mode::XGRP | Mode::ROTH | Mode::XOTH;
            match rustix::fs::mkdirat(dir.as_fd(), name, mode) {
                Ok(()) | Err(Errno::EXIST) => {}
                Err(e) => return Err(io("create", &path)(e)),
            }
        }
        dir = match rustix::fs::openat(dir.as_fd(), name, flags | OFlags::NOFOLLOW, Mode::empty()) {
            Ok(fd) => fd,
            Err(Errno::NOENT) if !create => return Ok(None),
            Err(Errno::LOOP | Errno::NOTDIR) => return Err(NotesError::NotRegular { path }),
            Err(e) => return Err(io("open", &path)(e)),
        };
    }
    Ok(Some(dir))
}

fn read_in(dir: &OwnedFd, path: &Path) -> Result<Snapshot, NotesError> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::openat(dir.as_fd(), NAME, flags, Mode::empty()) {
        Ok(fd) => fd,
        Err(Errno::NOENT) => return Ok(empty()),
        Err(Errno::LOOP) => return Err(not_regular(path)),
        Err(e) => return Err(io("open", path)(e)),
    };
    let file = File::from(fd);
    let meta = file.metadata().map_err(|source| NotesError::Io {
        action: "read",
        path: path.to_owned(),
        source,
    })?;
    if !meta.is_file() {
        return Err(not_regular(path));
    }
    let mut bytes = Vec::new();
    file.take(MAX_NOTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|source| NotesError::Io {
            action: "read",
            path: path.to_owned(),
            source,
        })?;
    let size = meta.len().max(bytes.len() as u64);
    if bytes.len() > MAX_NOTES || size > MAX_NOTES as u64 {
        return Err(NotesError::TooLarge {
            path: path.to_owned(),
            size,
        });
    }
    if bytes.contains(&0) {
        return Err(NotesError::Nul {
            path: path.to_owned(),
            size,
        });
    }
    let base = base_of(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| NotesError::NotUtf8 {
        path: path.to_owned(),
        size,
    })?;
    Ok(Snapshot { text, base })
}

/// Through a temp file and a rename in the same folder, so no reader sees half of it.
fn write_in(dir: &OwnedFd, path: &Path, bytes: &[u8]) -> Result<(), NotesError> {
    let rand = getrandom::u32().map_err(|e| NotesError::Io {
        action: "write",
        path: path.to_owned(),
        source: std::io::Error::other(e.to_string()),
    })?;
    let tmp = format!(".{NAME}.{rand:08x}.tmp");
    let flags = OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC;
    let mode = Mode::RUSR | Mode::WUSR | Mode::RGRP | Mode::ROTH;
    let fd =
        rustix::fs::openat(dir.as_fd(), tmp.as_str(), flags, mode).map_err(io("write", path))?;
    let mut file = File::from(fd);
    let written = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|source| NotesError::Io {
            action: "write",
            path: path.to_owned(),
            source,
        })
        .and_then(|()| {
            rustix::fs::renameat(dir.as_fd(), tmp.as_str(), dir.as_fd(), NAME)
                .map_err(io("write", path))
        });
    if let Err(e) = written {
        let _ = rustix::fs::unlinkat(dir.as_fd(), tmp.as_str(), AtFlags::empty());
        return Err(e);
    }
    rustix::fs::fsync(dir.as_fd()).map_err(io("sync", path))
}

fn not_regular(path: &Path) -> NotesError {
    NotesError::NotRegular {
        path: path.to_owned(),
    }
}

fn io(action: &'static str, path: &Path) -> impl Fn(Errno) -> NotesError {
    let path = path.to_owned();
    move |e| NotesError::Io {
        action,
        path: path.clone(),
        source: e.into(),
    }
}

#[cfg(test)]
#[path = "notes_tests.rs"]
mod tests;
