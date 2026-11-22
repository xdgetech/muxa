//! Reading and writing the settings files (ADR 0074): a revision over their
//! bytes, and atomic writes that keep one `.bak` and the file's mode.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// A new settings file can hold env values: only the user may read it.
const NEW_MODE: u32 = 0o600;
/// Far above any real settings file.
const MAX_FILE: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum FileError {
    #[error("cannot read {path}: {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("cannot write {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
    #[error("{path} is larger than 1 MiB")]
    TooLarge { path: PathBuf },
    #[error("{path} is not UTF-8")]
    NotText { path: PathBuf },
}

/// The file's bytes; None when it does not exist.
pub fn read(path: &Path) -> Result<Option<Vec<u8>>, FileError> {
    let err = |source| FileError::Read {
        path: path.to_owned(),
        source,
    };
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(err(e)),
    };
    let mut bytes = Vec::new();
    file.take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(FileError::TooLarge {
            path: path.to_owned(),
        });
    }
    Ok(Some(bytes))
}

/// The file's text; None when it does not exist.
pub fn read_text(path: &Path) -> Result<Option<String>, FileError> {
    read(path)?
        .map(|b| {
            String::from_utf8(b).map_err(|_| FileError::NotText {
                path: path.to_owned(),
            })
        })
        .transpose()
}

/// A hash over `files` in order: each path, and its bytes or that it is missing.
#[derive(Default)]
pub struct Revision(Sha256);

impl Revision {
    pub fn add(&mut self, path: &Path, bytes: Option<&[u8]>) {
        let path = path.to_string_lossy();
        self.0.update((path.len() as u64).to_le_bytes());
        self.0.update(path.as_bytes());
        match bytes {
            Some(b) => {
                self.0.update([1]);
                self.0.update((b.len() as u64).to_le_bytes());
                self.0.update(b);
            }
            None => self.0.update([0]),
        }
    }

    pub fn finish(self) -> String {
        self.0
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }
}

/// Where a write to `path` lands: the target of a symlink (a dotfiles
/// checkout links `workspaces.toml`), else `path`.
pub fn target(path: &Path) -> PathBuf {
    match fs::read_link(path) {
        Ok(link) if link.is_absolute() => link,
        Ok(link) => path.parent().map_or(link.clone(), |p| p.join(&link)),
        Err(_) => path.to_owned(),
    }
}

/// Writes `text` to `path` whole: a temporary file beside it, then a rename.
/// The old file is kept as `<path>.bak`. The file keeps its mode; a new one
/// gets 0600. Missing parent folders are made.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), FileError> {
    let path = target(path);
    let err = |source| FileError::Write {
        path: path.clone(),
        source,
    };
    let parent = path
        .parent()
        .ok_or_else(|| err(io::ErrorKind::InvalidInput.into()))?;
    fs::create_dir_all(parent).map_err(err)?;
    let mode = match fs::metadata(&path) {
        Ok(m) => Some(m.permissions().mode() & 0o7777),
        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
        Err(e) => return Err(err(e)),
    };
    if mode.is_some() {
        let bak = sibling(&path, ".bak");
        fs::copy(&path, &bak).map_err(err)?;
    }
    let tmp = sibling(&path, &format!(".tmp-{}", std::process::id()));
    let written = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(mode.unwrap_or(NEW_MODE))
            .open(&tmp)?;
        file.set_permissions(fs::Permissions::from_mode(mode.unwrap_or(NEW_MODE)))?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, &path)
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&tmp);
        return Err(err(e));
    }
    Ok(())
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muxa-files-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_write_keeps_a_backup_and_the_mode_and_a_new_file_is_private() {
        let dir = temp("write");
        let new = dir.join("sub/new.toml");
        write_atomic(&new, "a = 1\n").unwrap();
        assert_eq!(fs::read_to_string(&new).unwrap(), "a = 1\n");
        assert_eq!(
            fs::metadata(&new).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!dir.join("sub/new.toml.bak").exists());

        let old = dir.join("old.toml");
        fs::write(&old, "b = 1\n").unwrap();
        fs::set_permissions(&old, fs::Permissions::from_mode(0o640)).unwrap();
        write_atomic(&old, "b = 2\n").unwrap();
        assert_eq!(fs::read_to_string(&old).unwrap(), "b = 2\n");
        assert_eq!(
            fs::read_to_string(dir.join("old.toml.bak")).unwrap(),
            "b = 1\n"
        );
        assert_eq!(
            fs::metadata(&old).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let names: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names.len(), 3, "no temporary file left: {names:?}");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_symlinked_file_is_written_at_its_target() {
        let dir = temp("link");
        let real = dir.join("real.toml");
        fs::write(&real, "x = 1\n").unwrap();
        let link = dir.join("link.toml");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_atomic(&link, "x = 2\n").unwrap();
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(fs::read_to_string(&real).unwrap(), "x = 2\n");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_revision_changes_with_any_byte() {
        let rev = |a: Option<&[u8]>| {
            let mut r = Revision::default();
            r.add(Path::new("/w.toml"), a);
            r.finish()
        };
        assert_eq!(rev(Some(b"a")), rev(Some(b"a")));
        assert_ne!(rev(Some(b"a")), rev(Some(b"b")));
        assert_ne!(rev(None), rev(Some(b"")));
    }
}
