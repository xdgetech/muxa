//! The workspace memory, `<root>/.muxa/WORKSPACE.md` (ADR 0053 §1), which is
//! also the lead's notes, and a staff member's own notes,
//! `<root>/.muxa/assistant/staff/<worker>/NOTES.md` (ADR 0051 §5): the
//! snapshot a run reads, the checked write of a proposal, and the append of
//! a Task's close-out. Each folder is opened without following a symlink.
//! Errors name paths and sizes, never text.
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rustix::fs::{AtFlags, Mode, OFlags};
use rustix::io::Errno;
use sha2::{Digest, Sha256};

/// Relative to a workspace root, and to a run folder for the copy.
pub const NOTES: &str = ".muxa/WORKSPACE.md";
/// The lead's notes before ADR 0053: reported as a problem, never read.
pub const OLD_NOTES: &str = ".muxa/assistant/NOTES.md";
const WORKSPACE_NAME: &str = "WORKSPACE.md";
/// A member's folder is `.muxa/assistant/staff/<worker>`.
const MEMBER_FOLDERS: [&str; 3] = [".muxa", "assistant", STAFF];
const STAFF: &str = "staff";
const MEMBER_NAME: &str = "NOTES.md";

/// Which file: the workspace memory, which the lead keeps, or a member's own notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place<'a> {
    Workspace,
    /// A worker name, one path segment (checked at config load and again here).
    Member(&'a str),
}

impl Place<'_> {
    fn folders(&self) -> Result<Vec<&str>, NotesError> {
        let Self::Member(worker) = self else {
            return Ok(vec![".muxa"]);
        };
        let plain = |b: u8| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.');
        if worker.is_empty() || matches!(*worker, "." | "..") || !worker.bytes().all(plain) {
            return Err(NotesError::NotRegular {
                path: PathBuf::from(STAFF).join(worker),
            });
        }
        let mut out = MEMBER_FOLDERS.to_vec();
        out.push(worker);
        Ok(out)
    }

    /// The file's name in its folder.
    fn name(&self) -> &'static str {
        match self {
            Self::Workspace => WORKSPACE_NAME,
            Self::Member(_) => MEMBER_NAME,
        }
    }

    /// The file, relative to a workspace root or a run folder.
    pub fn file(&self) -> String {
        match self {
            Self::Workspace => NOTES.to_owned(),
            Self::Member(worker) => format!(".muxa/assistant/{STAFF}/{worker}/{MEMBER_NAME}"),
        }
    }
}
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
    #[error("{path} is {size} bytes; it may be at most 16 KiB. Shorten it, then try again.")]
    TooLarge { path: PathBuf, size: u64 },
    #[error("{path} ({size} bytes) holds a NUL byte; it must be UTF-8 text.")]
    Nul { path: PathBuf, size: u64 },
    #[error("{path} ({size} bytes) is not UTF-8 text.")]
    NotUtf8 { path: PathBuf, size: u64 },
    #[error("{path} is a symlink or not a regular file; it must be a regular file.")]
    NotRegular { path: PathBuf },
    #[error(
        "{} would be {} bytes; the cap is 16 KiB. Ask the lead to shorten it, then approve again.",
        path.display(),
        grouped(*size)
    )]
    WouldExceed { path: PathBuf, size: u64 },
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
    read_at(root, Place::Workspace)
}

/// Blocking. The live notes at `place` under `root`; empty without a file.
pub fn read_at(root: &Path, place: Place<'_>) -> Result<Snapshot, NotesError> {
    match folder(root, &place.folders()?, false)? {
        Some(dir) => read_in(&dir, place.name(), &root.join(place.file())),
        None => Ok(empty()),
    }
}

/// Blocking. When the live notes of the workspace at `root` last changed,
/// in epoch seconds; None without a file. Reads no content.
pub fn modified(root: &Path) -> Result<Option<u64>, NotesError> {
    modified_at(root, Place::Workspace)
}

/// Blocking. As [`modified`], for the notes at `place`.
pub fn modified_at(root: &Path, place: Place<'_>) -> Result<Option<u64>, NotesError> {
    let Some(dir) = folder(root, &place.folders()?, false)? else {
        return Ok(None);
    };
    let path = root.join(place.file());
    let stat = match rustix::fs::statat(dir.as_fd(), place.name(), AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(None),
        Err(e) => return Err(io("read", &path)(e)),
    };
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile {
        return Err(not_regular(&path));
    }
    Ok(Some(u64::try_from(stat.st_mtime).unwrap_or(0)))
}

/// Blocking. Copies the live notes into the run folder `dir`, as long as
/// they are still the ones whose hash the run recorded as `base`.
pub fn copy_into(root: &Path, dir: &Path, base: &str) -> Result<(), NotesError> {
    copy_at(root, dir, Place::Workspace, base)
}

/// Blocking. As [`copy_into`], for the notes at `place`; the copy lands at
/// the same place under `dir`.
pub fn copy_at(root: &Path, dir: &Path, place: Place<'_>, base: &str) -> Result<(), NotesError> {
    let live = read_at(root, place)?;
    if live.base != base {
        return Err(NotesError::Changed {
            path: root.join(place.file()),
        });
    }
    let stage = folder(dir, &place.folders()?, true)?.ok_or_else(|| missing(dir))?;
    write_in(
        &stage,
        place.name(),
        &dir.join(place.file()),
        live.text.as_bytes(),
    )
}

/// Blocking. Replaces the live notes with `text` while they still hash to
/// `base`; creates the folders when missing.
pub fn apply(root: &Path, text: &str, base: &str) -> Result<(), NotesError> {
    apply_at(root, Place::Workspace, text, base)
}

/// Blocking. As [`apply`], for the notes at `place`.
pub fn apply_at(root: &Path, place: Place<'_>, text: &str, base: &str) -> Result<(), NotesError> {
    let path = root.join(place.file());
    if text.len() > MAX_NOTES || text.contains('\0') {
        return Err(NotesError::TooLarge {
            path,
            size: text.len() as u64,
        });
    }
    let folders = place.folders()?;
    let _one = WRITES.lock().unwrap_or_else(|e| e.into_inner());
    let dir = folder(root, &folders, true)?.ok_or_else(|| missing(root))?;
    let live = match read_in(&dir, place.name(), &path) {
        Ok(live) => live,
        Err(e @ NotesError::Io { .. }) => return Err(e),
        // A run only ever reads a valid file: an invalid one has changed.
        Err(_) => return Err(NotesError::Changed { path }),
    };
    if live.base != base {
        return Err(NotesError::Changed { path });
    }
    write_in(&dir, place.name(), &path, text.as_bytes())
}

/// Blocking. Whether the lead's notes from before ADR 0053 are still at
/// [`OLD_NOTES`]. Reads no content.
pub fn old_notes_present(root: &Path) -> bool {
    std::fs::symlink_metadata(root.join(OLD_NOTES)).is_ok()
}

/// Blocking. A Task run's copy of the workspace memory (ADR 0053 §2): the
/// live file into `<dir>/.muxa/WORKSPACE.md`, and its hash. Without a live
/// file, or with an empty one, an older copy is removed and None returned.
pub fn give(root: &Path, dir: &Path) -> Result<Option<String>, NotesError> {
    let live = read(root)?;
    let place = Place::Workspace;
    if same_dir(root, dir) {
        return Ok((!live.text.is_empty()).then_some(live.base));
    }
    if live.text.is_empty() {
        if let Some(stage) = folder(dir, &place.folders()?, false)? {
            match rustix::fs::unlinkat(stage.as_fd(), place.name(), AtFlags::empty()) {
                Ok(()) | Err(Errno::NOENT) => {}
                Err(e) => return Err(io("remove", &dir.join(place.file()))(e)),
            }
        }
        return Ok(None);
    }
    let stage = folder(dir, &place.folders()?, true)?.ok_or_else(|| missing(dir))?;
    write_in(
        &stage,
        place.name(),
        &dir.join(place.file()),
        live.text.as_bytes(),
    )?;
    Ok(Some(live.base))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// What [`append`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appended {
    /// The file is this many bytes now.
    Added { size: u64 },
    /// `present` found the block's mark already: nothing was written.
    Present,
}

/// Blocking. Appends `block` to the workspace memory at `root`, after a
/// blank line, unless `present` finds it in the live text already. The
/// result may not pass [`MAX_NOTES`]. The caller holds the workspace's OS
/// lock, so the read and the rename are one step for every process.
pub fn append(
    root: &Path,
    block: &str,
    present: impl Fn(&str) -> bool,
) -> Result<Appended, NotesError> {
    let place = Place::Workspace;
    let path = root.join(place.file());
    let _one = WRITES.lock().unwrap_or_else(|e| e.into_inner());
    let dir = folder(root, &place.folders()?, true)?.ok_or_else(|| missing(root))?;
    let live = read_in(&dir, place.name(), &path)?;
    if present(&live.text) {
        return Ok(Appended::Present);
    }
    let gap = match live.text.as_str() {
        "" => "",
        t if t.ends_with("\n\n") => "",
        t if t.ends_with('\n') => "\n",
        _ => "\n\n",
    };
    let text = format!("{}{gap}{block}", live.text);
    let size = text.len() as u64;
    if text.len() > MAX_NOTES {
        return Err(NotesError::WouldExceed { path, size });
    }
    write_in(&dir, place.name(), &path, text.as_bytes())?;
    Ok(Appended::Added { size })
}

/// `17210` as `17,210`.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn empty() -> Snapshot {
    Snapshot {
        text: String::new(),
        base: base_of(b""),
    }
}

fn missing(dir: &Path) -> NotesError {
    NotesError::NotRegular {
        path: dir.join(".muxa"),
    }
}

/// `<base>/<folders>`, opened. None when a folder is missing and `create`
/// is false.
fn folder(base: &Path, folders: &[&str], create: bool) -> Result<Option<OwnedFd>, NotesError> {
    let flags = OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC;
    let mut dir = rustix::fs::open(base, flags, Mode::empty()).map_err(io("open", base))?;
    let mut path = base.to_path_buf();
    for name in folders.iter().copied() {
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

fn read_in(dir: &OwnedFd, name: &str, path: &Path) -> Result<Snapshot, NotesError> {
    let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
    let fd = match rustix::fs::openat(dir.as_fd(), name, flags, Mode::empty()) {
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
fn write_in(dir: &OwnedFd, name: &str, path: &Path, bytes: &[u8]) -> Result<(), NotesError> {
    let rand = getrandom::u32().map_err(|e| NotesError::Io {
        action: "write",
        path: path.to_owned(),
        source: std::io::Error::other(e.to_string()),
    })?;
    let tmp = format!(".{name}.{rand:08x}.tmp");
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
            rustix::fs::renameat(dir.as_fd(), tmp.as_str(), dir.as_fd(), name)
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
