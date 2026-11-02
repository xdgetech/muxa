//! The daemon's Unix socket: `daemon.sock` in a 0700 directory, mode 0600,
//! peers checked by uid. It speaks protocol v1 as JSON lines, plus
//! `shutdown`, which exists only here (ADR 0019).

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncBufRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::lines::{self, Line};

pub const FILE: &str = "daemon.sock";
/// `sun_path` is 104 bytes on macOS, with the final NUL.
pub const MAX_PATH: usize = 103;
/// The longest line read from the socket. A `task-list` of 200 records of at
/// most 64 KiB each stays below it.
pub const MAX_LINE: usize = 32 << 20;

/// `{"type": "shutdown", "id": n}`: stop every loop for the user, then exit.
/// A bridge never forwards it; it is not a protocol request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Control {
    Shutdown { id: u64 },
}

pub fn path(dir: &Path) -> Result<PathBuf, String> {
    let path = dir.join(FILE);
    let len = path.as_os_str().len();
    if len > MAX_PATH {
        return Err(format!(
            "the socket path {} is {len} bytes; Unix sockets allow {MAX_PATH}. Pass a shorter --daemon-dir",
            path.display()
        ));
    }
    Ok(path)
}

/// Binds a fresh socket. Only the lock holder calls this, so a socket file
/// already there is a leftover of a daemon that died.
pub fn bind(path: &Path) -> io::Result<UnixListener> {
    match std::fs::remove_file(path) {
        Ok(()) => tracing::info!(path = %path.display(), "removed a leftover socket"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Only the daemon's own user may talk to it.
pub fn peer_allowed(stream: &UnixStream) -> bool {
    match stream.peer_cred() {
        Ok(cred) => same_user(cred.uid(), rustix::process::getuid().as_raw()),
        Err(e) => {
            tracing::warn!(error = %e, "cannot read a peer's credentials");
            false
        }
    }
}

fn same_user(peer: u32, me: u32) -> bool {
    peer == me
}

/// The next JSON line, `None` at the end of input. An unreadable line is an error.
pub async fn read<T: DeserializeOwned, R: AsyncBufRead + Unpin>(
    r: &mut R,
    max: usize,
) -> io::Result<Option<T>> {
    match lines::next(r, max).await? {
        None => Ok(None),
        Some(Line::TooLong) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a line longer than {max} bytes"),
        )),
        Some(Line::Text(line)) => serde_json::from_slice(&line)
            .map(Some)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)),
    }
}

pub async fn write<T: Serialize, W: AsyncWrite + Unpin>(w: &mut W, msg: &T) -> io::Result<()> {
    let mut line = serde_json::to_vec(msg).map_err(io::Error::other)?;
    line.push(b'\n');
    w.write_all(&line).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_same_uid_is_allowed() {
        assert!(same_user(501, 501));
        assert!(!same_user(0, 501), "not even root");
        assert!(!same_user(502, 501));
    }

    #[test]
    fn a_long_socket_path_is_refused() {
        assert!(path(Path::new("/tmp/muxa")).is_ok());
        let long = format!("/tmp/{}", "d".repeat(100));
        let e = path(Path::new(&long)).unwrap_err();
        assert!(e.contains("--daemon-dir"), "{e}");
    }

    #[test]
    fn shutdown_is_not_a_protocol_request() {
        let line = r#"{"type":"shutdown","id":3}"#;
        assert!(serde_json::from_str::<muxa_protocol::Request>(line).is_err());
        assert_eq!(
            serde_json::from_str::<Control>(line).unwrap(),
            Control::Shutdown { id: 3 }
        );
    }
}
