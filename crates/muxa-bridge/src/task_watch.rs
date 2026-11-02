//! Clients learn of record changes from a poll (ADR 0019): every 2 s the
//! bridge looks at the `tasks/` directory and sends `tasks-changed` when a
//! record file came, went or was rewritten. This sees every writer: the
//! daemon, a bridge serving records itself, and `muxa-cli` on the host.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use muxa_protocol::Message;
use tokio::sync::mpsc;

const TICK: Duration = Duration::from_secs(2);

/// Each `*.json` file's name, inode, size and mtime. A write replaces the
/// file by rename, so the inode changes even when the size does not.
type Look = BTreeMap<OsString, (u64, u64, i64, i64)>;

pub async fn run(dir: PathBuf, out: mpsc::Sender<Message>) {
    let mut last: Option<Look> = None;
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tick.tick().await;
        let d = dir.clone();
        let Ok(now) = tokio::task::spawn_blocking(move || look(&d)).await else {
            continue;
        };
        let changed = last.as_ref().is_some_and(|l| *l != now);
        last = Some(now);
        if changed && out.send(Message::TasksChanged).await.is_err() {
            return;
        }
    }
}

/// A missing directory looks empty: the first record creates it.
fn look(dir: &Path) -> Look {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Look::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|e| {
            Path::new(&e.file_name())
                .extension()
                .is_some_and(|x| x == "json")
        })
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((
                e.file_name(),
                (m.ino(), m.size(), m.mtime(), m.mtime_nsec()),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewrite_a_new_file_and_a_removal_each_change_the_look() {
        let dir = std::env::temp_dir().join(format!("muxa-task-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(look(&dir).is_empty());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("t-000001.json"), "{}").unwrap();
        std::fs::write(dir.join("t-000001.lock"), "").unwrap();
        let first = look(&dir);
        assert_eq!(first.len(), 1, "only record files count");
        assert_eq!(look(&dir), first, "nothing changed");

        // Same size, written by rename as the store does.
        std::fs::write(dir.join("tmp"), "[]").unwrap();
        std::fs::rename(dir.join("tmp"), dir.join("t-000001.json")).unwrap();
        let rewritten = look(&dir);
        assert_ne!(rewritten, first);

        std::fs::write(dir.join("t-000002.json"), "{}").unwrap();
        let added = look(&dir);
        assert_ne!(added, rewritten);
        std::fs::remove_file(dir.join("t-000002.json")).unwrap();
        assert_eq!(look(&dir), rewritten);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
