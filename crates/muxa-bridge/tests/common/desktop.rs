//! A desktop terminal for tests: a real tmux client on a pty, held open by
//! `scripts/lib/pty-client.py`. It types "x" when the test says so.

use std::io::Write;
use std::process::{Child, ChildStdin, Command, Stdio};

use super::Server;

const PTY_CLIENT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/lib/pty-client.py"
);

pub struct Desktop {
    child: Child,
    stdin: Option<ChildStdin>,
}

impl Desktop {
    /// Attaches to `session` as a `cols` x `rows` terminal. tmux gives the
    /// window one row less: the status line.
    pub fn attach(server: &Server, session: &str, cols: u16, rows: u16) -> Self {
        let mut child = Command::new("python3")
            .arg("-I")
            .arg(PTY_CLIENT)
            .args([&server.socket, session])
            .args([&cols.to_string(), &rows.to_string(), "300", "stdin"])
            .env_remove("TMUX")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the pty client");
        let stdin = child.stdin.take();
        Desktop { child, stdin }
    }

    /// A key press on the desktop: it becomes the window's latest client.
    pub fn type_key(&mut self) {
        let stdin = self.stdin.as_mut().expect("desktop stdin");
        stdin.write_all(b"\n").expect("type on the desktop");
        stdin.flush().expect("flush");
    }

    /// The terminal closes: its tmux client detaches.
    pub fn close(mut self) {
        self.stop();
    }

    /// End of input makes the pty client stop its tmux client itself.
    fn stop(&mut self) {
        drop(self.stdin.take());
        for _ in 0..30 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.stop();
    }
}
