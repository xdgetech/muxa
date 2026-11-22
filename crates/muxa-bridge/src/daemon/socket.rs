//! The daemon's Unix socket: `server.sock` in a 0700 directory, mode 0600,
//! peers checked by uid.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use tokio::net::{UnixListener, UnixStream};

/// `sun_path` is 104 bytes on macOS, with the final NUL.
pub const MAX_PATH: usize = 103;
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

/// Only the daemon's own user may talk to it. Each refusal is logged here.
pub fn peer_allowed(stream: &UnixStream) -> bool {
    match stream.peer_cred() {
        Ok(cred) if same_user(cred.uid(), rustix::process::getuid().as_raw()) => true,
        Ok(cred) => {
            tracing::warn!(uid = cred.uid(), "refused a connection from another user");
            false
        }
        // macOS: a client that closed before the check has no credentials.
        Err(e) if e.raw_os_error() == Some(rustix::io::Errno::NOTCONN.raw_os_error()) => {
            tracing::debug!("a client left before it was checked");
            false
        }
        Err(e) => {
            tracing::warn!(error = %e, "cannot read a peer's credentials");
            false
        }
    }
}

fn same_user(peer: u32, me: u32) -> bool {
    peer == me
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
}
