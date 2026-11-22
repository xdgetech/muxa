//! `<state dir>/inbox.json`, written whole and atomically on each change.
//! Only the muxa server writes it.

use std::fs::{self, DirBuilder, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;

use crate::book::{Book, FORMAT_VERSION};

/// A whole file, in bytes.
const MAX_FILE: u64 = 4 << 20;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} is not an Inbox file: {why}")]
    Bad { path: String, why: String },
    #[error("{path} was written by a newer muxa (format {found}); update muxa")]
    Newer { path: String, found: u32 },
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
}

/// The Inbox at `path`; an empty one when the file is missing.
pub fn load(path: &Path) -> Result<Book, StoreError> {
    let shown = path.display().to_string();
    let meta = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Book::default()),
        Err(source) => {
            return Err(StoreError::Read {
                path: shown,
                source,
            });
        }
    };
    if meta.len() > MAX_FILE {
        return Err(StoreError::Bad {
            path: shown,
            why: format!("over {MAX_FILE} bytes"),
        });
    }
    let text = fs::read(path).map_err(|source| StoreError::Read {
        path: shown.clone(),
        source,
    })?;
    let book: Book = serde_json::from_slice(&text).map_err(|e| StoreError::Bad {
        path: shown.clone(),
        why: e.to_string(),
    })?;
    if book.version() > FORMAT_VERSION {
        return Err(StoreError::Newer {
            path: shown,
            found: book.version(),
        });
    }
    Ok(book)
}

/// Writes a temporary file beside `path` (mode 0600), then renames it.
pub fn save(path: &Path, book: &Book) -> Result<(), StoreError> {
    let err = |source| StoreError::Write {
        path: path.display().to_string(),
        source,
    };
    if let Some(dir) = path.parent() {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(err)?;
    }
    let bytes = serde_json::to_vec_pretty(book).map_err(|e| err(e.into()))?;
    let tmp = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(err)?;
    file.write_all(&bytes).map_err(err)?;
    file.sync_all().map_err(err)?;
    fs::rename(&tmp, path).map_err(err)
}

#[cfg(test)]
mod tests {
    use muxa_protocol::v2::ItemKind;

    use super::*;
    use crate::NewItem;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("muxa-inbox-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_saved_inbox_loads_the_same() {
        let d = dir("round");
        let path = d.join("inbox.json");
        assert_eq!(load(&path).unwrap(), Book::default());
        let mut b = Book::default();
        let new = NewItem {
            kind: ItemKind::Read,
            title: "RFC".into(),
            note: None,
            workspace: None,
            due: None,
            source: "capture".into(),
            key: Some("k".into()),
            once: false,
            terminal: None,
            routine: None,
            place: None,
        };
        b.add(new, 7).unwrap();
        save(&path, &b).unwrap();
        assert_eq!(load(&path).unwrap(), b);
        let mode = fs::metadata(&path).unwrap().permissions();
        assert_eq!(
            std::os::unix::fs::PermissionsExt::mode(&mode) & 0o777,
            0o600
        );
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn items_saved_before_they_linked_their_maker_still_load() {
        let d = dir("old");
        fs::create_dir_all(&d).unwrap();
        let path = d.join("inbox.json");
        fs::write(
            &path,
            r#"{"version":1,"revision":1,"next_id":2,"items":[{"id":"i1","kind":"review",
                "title":"Review fix","created":5,"source":"task","key":"review:t1"}],"done":[]}"#,
        )
        .unwrap();
        let item = &load(&path).unwrap().view().items[0];
        assert_eq!((&item.terminal, &item.routine), (&None, &None));
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_newer_file_is_not_read() {
        let d = dir("newer");
        fs::create_dir_all(&d).unwrap();
        let path = d.join("inbox.json");
        fs::write(
            &path,
            r#"{"version":9,"revision":1,"next_id":1,"items":[],"done":[]}"#,
        )
        .unwrap();
        assert!(matches!(
            load(&path),
            Err(StoreError::Newer { found: 9, .. })
        ));
        fs::remove_dir_all(&d).unwrap();
    }
}
