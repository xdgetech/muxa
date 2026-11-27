//! Whether the daemon may write to a build disk, with a time limit. On
//! macOS the first write to an external volume can wait on a privacy prompt
//! that a headless host never shows.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError, mpsc};
use std::time::Duration;

/// How long Task start and Settings wait for the first write.
pub const WAIT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Access {
    /// The folder is there and takes a new folder.
    Writable,
    /// No folder there: the disk is not mounted, or `dir` has a typo.
    Missing,
}

/// Dirs whose check still waits, so a blocked disk holds one thread only.
static WAITING: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// Checks `dir` in a thread and waits at most `wait`. The error is the
/// message for the user, with the path. Blocking.
pub fn check(dir: &Path, wait: Duration) -> Result<Access, String> {
    check_with(dir, wait, probe)
}

/// The folder is there and the daemon may write to it, within [`WAIT`].
/// A refusal or a wait is logged. Blocking.
pub fn usable(dir: &Path) -> bool {
    match check(dir, WAIT) {
        Ok(Access::Writable) => true,
        Ok(Access::Missing) => false,
        Err(why) => {
            tracing::warn!(dir = %dir.display(), error = %why, "skipping the build disk");
            false
        }
    }
}

fn check_with(
    dir: &Path,
    wait: Duration,
    probe: impl FnOnce(&Path) -> io::Result<Access> + Send + 'static,
) -> Result<Access, String> {
    if !waiting(|w| w.insert(dir.to_owned())) {
        return Err(blocked(dir));
    }
    let (tx, rx) = mpsc::channel();
    let owned = dir.to_owned();
    let spawned = std::thread::Builder::new()
        .name("build-access".into())
        .spawn(move || {
            let result = probe(&owned);
            waiting(|w| w.remove(&owned));
            // The caller may have stopped waiting.
            let _ = tx.send(result);
        });
    if let Err(e) = spawned {
        waiting(|w| w.remove(dir));
        return Err(format!("muxa-bridge cannot check {}: {e}", dir.display()));
    }
    match rx.recv_timeout(wait) {
        Ok(Ok(access)) => Ok(access),
        Ok(Err(e)) if e.kind() == io::ErrorKind::PermissionDenied => Err(blocked(dir)),
        Ok(Err(e)) => Err(format!(
            "muxa-bridge cannot write to {}: {e}",
            dir.display()
        )),
        Err(_) => {
            tracing::warn!(dir = %dir.display(), "the build disk does not answer; macOS may be asking for access");
            Err(blocked(dir))
        }
    }
}

fn waiting<T>(f: impl FnOnce(&mut HashSet<PathBuf>) -> T) -> T {
    let mut guard = WAITING.lock().unwrap_or_else(PoisonError::into_inner);
    f(guard.get_or_insert_with(HashSet::new))
}

fn blocked(dir: &Path) -> String {
    format!(
        "muxa-bridge cannot write to {}. Allow it in System Settings › Privacy & Security.",
        dir.display()
    )
}

/// Makes and removes a folder in `dir`.
fn probe(dir: &Path) -> io::Result<Access> {
    if !dir.is_dir() {
        return Ok(Access::Missing);
    }
    let at = dir.join(format!(".muxa-access-{}", std::process::id()));
    match std::fs::create_dir(&at) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    std::fs::remove_dir(&at)?;
    Ok(Access::Writable)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("muxa-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_folder_is_writable_or_missing() {
        let dir = temp("access-ok");
        assert_eq!(check(&dir, WAIT), Ok(Access::Writable));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        assert_eq!(check(&dir.join("not-mounted"), WAIT), Ok(Access::Missing));
    }

    #[test]
    fn a_folder_that_refuses_says_where_to_allow_it() {
        let dir = temp("access-denied");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let got = check(&dir, WAIT);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        let why = got.unwrap_err();
        assert!(why.contains(&dir.display().to_string()), "{why}");
        assert!(why.contains("Privacy & Security"), "{why}");
    }

    #[test]
    fn a_disk_that_blocks_stops_the_wait_and_holds_one_thread() {
        let dir = PathBuf::from("/muxa-test/blocking-disk");
        let (release, gate) = mpsc::channel::<()>();
        let started = std::time::Instant::now();
        let why = check_with(&dir, Duration::from_millis(50), move |_| {
            let _ = gate.recv();
            Ok(Access::Writable)
        })
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(why.contains("/muxa-test/blocking-disk") && why.contains("System Settings"));
        // While the first check waits, the next one answers at once.
        let again = check_with(&dir, WAIT, |_| panic!("a second probe started"));
        assert_eq!(again, Err(why));
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while waiting(|w| w.contains(&dir)) && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(
            check_with(&dir, WAIT, |_| Ok(Access::Writable)),
            Ok(Access::Writable)
        );
    }
}
