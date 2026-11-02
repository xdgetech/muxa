//! One daemon per daemon dir: an exclusive `flock` on `daemon.lock`. The
//! kernel drops it when the process dies, so a crash leaves no stale lock.

use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use rustix::fs::{FlockOperation, flock};

pub const FILE: &str = "daemon.lock";

/// Held while the daemon runs; dropping it releases the lock.
#[derive(Debug)]
pub struct Lock(#[allow(dead_code)] File);

/// The lock, or `None` when another process holds it.
pub fn acquire(dir: &Path) -> io::Result<Option<Lock>> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(dir.join(FILE))?;
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(Lock(file))),
        Err(e) if e == rustix::io::Errno::WOULDBLOCK => Ok(None),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_holder_is_refused_until_the_first_drops() {
        let dir = std::env::temp_dir().join(format!("muxa-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = acquire(&dir).unwrap().expect("the first lock");
        // flock locks belong to the open file, so a second open conflicts.
        assert!(acquire(&dir).unwrap().is_none());
        drop(first);
        assert!(acquire(&dir).unwrap().is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
