//! W0 OS-lock evidence, not a test of the unimplemented restart transaction.
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{OpenOptionsExt, symlink};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use rustix::fs::{FlockOperation, flock};

struct Fixture(PathBuf);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Holder(Child);

impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn child(path: &std::path::Path, mode: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "lifecycle_lock_child", "--nocapture"])
        .env("MUXA_LOCK_PROBE_PATH", path)
        .env("MUXA_LOCK_PROBE_MODE", mode)
        .stdin(Stdio::null())
        .stdout(Stdio::piped());
    command
}

fn contender(path: &std::path::Path) -> String {
    let output = child(path, "try").output().unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn lifecycle_lock_child() {
    let Some(path) = std::env::var_os("MUXA_LOCK_PROBE_PATH") else {
        return;
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)
        .unwrap();
    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => println!("probe-acquired"),
        Err(rustix::io::Errno::WOULDBLOCK) => {
            println!("probe-blocked");
            return;
        }
        Err(error) => panic!("cannot lock probe file: {error}"),
    }
    std::io::stdout().flush().unwrap();
    if std::env::var("MUXA_LOCK_PROBE_MODE").unwrap() == "hold" {
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
}

#[test]
fn lifecycle_lock_excludes_other_processes_until_owner_dies() {
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "muxa-lock-probe-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    )));
    fs::create_dir(&fixture.0).unwrap();
    let real = fixture.0.join("records");
    fs::create_dir(&real).unwrap();
    let alias = fixture.0.join("alias");
    symlink(&real, &alias).unwrap();
    let path = real.join("task.lock");
    let alias_path = alias.join("task.lock");
    let mut owner = Holder(child(&path, "hold").spawn().unwrap());
    let mut reader = BufReader::new(owner.0.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(reader.read_line(&mut line).unwrap(), 0);
        if line.contains("probe-acquired") {
            break;
        }
    }
    assert!(contender(&alias_path).contains("probe-blocked"));
    // Existing record locks expire after ten seconds. A live OS owner must not.
    std::thread::sleep(Duration::from_secs(11));
    assert!(contender(&path).contains("probe-blocked"));
    assert!(contender(&alias_path).contains("probe-blocked"));
    let before = File::open(&path).unwrap().metadata().unwrap();
    owner.0.kill().unwrap();
    owner.0.wait().unwrap();
    assert!(contender(&alias_path).contains("probe-acquired"));
    use std::os::unix::fs::MetadataExt;
    let after = fs::metadata(&path).unwrap();
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
}
