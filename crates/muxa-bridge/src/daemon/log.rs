//! `daemon.log`, written by the daemon itself and rotated at 1 MiB with one
//! old file kept. Lines carry ids, paths, states and codes, never task text
//! or pane bytes (invariant 5).

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

pub const FILE: &str = "daemon.log";
const MAX: u64 = 1 << 20;

/// A log file that moves itself to `<name>.1` once it passes `max` bytes.
#[derive(Debug, Clone)]
pub struct Rotating(Arc<Mutex<Inner>>);

#[derive(Debug)]
struct Inner {
    path: PathBuf,
    file: File,
    size: u64,
    max: u64,
}

impl Rotating {
    pub fn open(dir: &Path) -> io::Result<Self> {
        Self::with_max(dir.join(FILE), MAX)
    }

    fn with_max(path: PathBuf, max: u64) -> io::Result<Self> {
        let file = append(&path)?;
        let size = file.metadata()?.len();
        Ok(Self(Arc::new(Mutex::new(Inner {
            path,
            file,
            size,
            max,
        }))))
    }
}

fn append(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
}

impl Inner {
    fn rotate(&mut self) -> io::Result<()> {
        let mut old = self.path.clone().into_os_string();
        old.push(".1");
        std::fs::rename(&self.path, old)?;
        self.file = append(&self.path)?;
        self.size = 0;
        Ok(())
    }
}

impl Write for Rotating {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if inner.size > 0 && inner.size + buf.len() as u64 > inner.max {
            // A failed rotation keeps appending to the current file.
            if let Err(e) = inner.rotate() {
                let _ = writeln!(inner.file, "cannot rotate the log: {e}");
            }
        }
        let n = inner.file.write(buf)?;
        inner.size += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        inner.file.flush()
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Rotating {
    type Writer = Rotating;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_past_the_limit_and_keeps_one_old_file() {
        let dir = std::env::temp_dir().join(format!("muxa-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE);
        let mut log = Rotating::with_max(path.clone(), 10).unwrap();
        log.write_all(b"aaaaaaaa\n").unwrap();
        log.write_all(b"bbbbbbbb\n").unwrap();
        log.write_all(b"cccccccc\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "cccccccc\n");
        assert_eq!(
            std::fs::read_to_string(dir.join("daemon.log.1")).unwrap(),
            "bbbbbbbb\n"
        );
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
