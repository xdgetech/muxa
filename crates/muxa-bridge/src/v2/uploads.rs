//! Writes a file from the user's device into a Task's working place, under
//! `.muxa/uploads/` (ADR 0081). The server already checked the name and size.
//! The bytes never reach a log.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use muxa_server::TaskError;

const FOLDER: &str = ".muxa/uploads";
/// Names tried before giving up, as `name 2.ext` … `name 999.ext`.
const MAX_TRIES: u32 = 999;

pub async fn write(dir: &Path, name: &str, bytes: Vec<u8>) -> Result<String, TaskError> {
    let (dir, name) = (dir.to_owned(), name.to_owned());
    tokio::task::spawn_blocking(move || write_blocking(&dir, &name, &bytes))
        .await
        .map_err(|e| TaskError::Failed(format!("the upload did not finish: {e}")))?
}

fn write_blocking(dir: &Path, name: &str, bytes: &[u8]) -> Result<String, TaskError> {
    let root = dir
        .canonicalize()
        .map_err(|e| TaskError::Invalid(format!("the Task's working place is gone: {e}")))?;
    let folder = root.join(FOLDER);
    fs::create_dir_all(&folder).map_err(|e| failed("make the uploads folder", &e))?;
    // A symlink must not lead the file out of the working place.
    let real = folder
        .canonicalize()
        .map_err(|e| failed("open the uploads folder", &e))?;
    if !real.starts_with(&root) {
        return Err(TaskError::Invalid(format!(
            "{FOLDER} leads out of the Task's working place"
        )));
    }
    for n in 1..=MAX_TRIES {
        let file = numbered(name, n);
        match create(&real.join(&file), bytes) {
            Ok(()) => return Ok(format!("{FOLDER}/{file}")),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(failed("write the file", &e)),
        }
    }
    Err(TaskError::Busy(format!(
        "{FOLDER} already has {MAX_TRIES} files called {name}"
    )))
}

/// Never replaces a file: a name that is taken gets a number.
fn create(path: &PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(e) = f.write_all(bytes).and_then(|()| f.sync_all()) {
        let _ = fs::remove_file(path);
        return Err(e);
    }
    Ok(())
}

/// `log.txt`, then `log 2.txt`, `log 3.txt`.
fn numbered(name: &str, n: u32) -> String {
    if n == 1 {
        return name.to_owned();
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => format!("{stem} {n}.{ext}"),
        _ => format!("{name} {n}"),
    }
}

fn failed(what: &str, e: &std::io::Error) -> TaskError {
    TaskError::Failed(format!("could not {what}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("muxa-v2-uploads-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn a_file_lands_in_the_uploads_folder_and_never_replaces_one() {
        let dir = temp("land");
        assert_eq!(
            write(&dir, "build.log", b"one".to_vec()).await.unwrap(),
            ".muxa/uploads/build.log"
        );
        assert_eq!(
            write(&dir, "build.log", b"two".to_vec()).await.unwrap(),
            ".muxa/uploads/build 2.log"
        );
        assert_eq!(
            fs::read(dir.join(".muxa/uploads/build.log")).unwrap(),
            b"one"
        );
        assert_eq!(
            fs::read(dir.join(".muxa/uploads/build 2.log")).unwrap(),
            b"two"
        );
        assert_eq!(numbered("notes", 3), "notes 3");
    }

    #[tokio::test]
    async fn a_symlinked_uploads_folder_is_refused() {
        let dir = temp("link");
        let outside = temp("outside");
        fs::create_dir_all(dir.join(".muxa")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join(".muxa/uploads")).unwrap();
        let refused = write(&dir, "x.png", b"x".to_vec()).await;
        assert!(matches!(refused, Err(TaskError::Invalid(_))), "{refused:?}");
        assert!(
            fs::read_dir(&outside).unwrap().next().is_none(),
            "nothing was written outside"
        );
    }
}
