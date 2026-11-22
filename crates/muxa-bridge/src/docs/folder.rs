//! Files of a Task's folder outside git (ADR 0033 §4a): every document and
//! image in it. The scan never follows a symlink and stops at its limits.
//! Blocking.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::files::mtime;

/// Levels below the folder.
const MAX_DEPTH: usize = 6;
/// Directory entries one scan reads, as `records/repos.rs` does.
const MAX_ENTRIES: usize = 2000;
const MAX_SCAN: Duration = Duration::from_millis(250);

/// A file the scan found, relative to the folder with `/` between names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub path: String,
    pub mtime: Option<u64>,
}

/// The files under `root` that `keep` takes by name, changed at or after
/// `since` (epoch seconds) when given. Breadth first, so a cut keeps the
/// shallow ones. Skips symlinks and every name that starts with a dot, so
/// `.muxa/` and `.claude/` are never read.
pub(crate) fn scan(root: &Path, keep: impl Fn(&str) -> bool, since: Option<u64>) -> Vec<Found> {
    let mut entries = MAX_ENTRIES;
    let until = Instant::now() + MAX_SCAN;
    let mut found = Vec::new();
    let mut queue = VecDeque::from([(PathBuf::new(), 0usize)]);
    while let Some((rel, depth)) = queue.pop_front() {
        let Ok(dir) = std::fs::read_dir(root.join(&rel)) else {
            continue;
        };
        for entry in dir {
            if entries == 0 || Instant::now() >= until {
                tracing::debug!(root = %root.display(), "the folder scan stopped at its limit");
                return found;
            }
            entries -= 1;
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|n| !n.starts_with('.')) else {
                continue;
            };
            // `file_type` does not follow a symlink: a link is neither.
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = rel.join(name);
            if kind.is_dir() && depth + 1 < MAX_DEPTH {
                queue.push_back((path, depth + 1));
            } else if kind.is_file() {
                let Some(text) = path.to_str().map(str::to_owned) else {
                    continue;
                };
                if !keep(&text) {
                    continue;
                }
                let mtime = mtime(&entry.path());
                if since.is_none_or(|s| mtime.is_some_and(|m| m >= s)) {
                    found.push(Found { path: text, mtime });
                }
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn dir() -> Dir {
        let d = std::env::temp_dir().join(format!(
            "muxa-folder-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d.canonicalize().unwrap())
    }

    fn paths(found: &[Found]) -> Vec<&str> {
        let mut out: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        out.sort_unstable();
        out
    }

    fn write(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    #[test]
    fn the_scan_skips_dot_folders_symlinks_and_other_names() {
        let d = dir();
        for rel in [
            "plan.md",
            "out/chart.png",
            "notes.txt",
            ".muxa/TASK.md",
            ".claude/plan.md",
            ".hidden.md",
        ] {
            write(&d.0, rel);
        }
        let outside = dir();
        write(&outside.0, "secret.md");
        std::os::unix::fs::symlink(outside.0.join("secret.md"), d.0.join("link.md")).unwrap();
        std::os::unix::fs::symlink(&outside.0, d.0.join("linked")).unwrap();
        let keep = |p: &str| crate::docs::files::is_doc(p) || crate::docs::images::is_image(p);
        assert_eq!(paths(&scan(&d.0, keep, None)), ["out/chart.png", "plan.md"]);
    }

    #[test]
    fn since_keeps_only_files_changed_from_then() {
        let d = dir();
        write(&d.0, "old.md");
        let now = crate::records::now();
        assert_eq!(
            paths(&scan(&d.0, crate::docs::files::is_doc, Some(now))),
            ["old.md"]
        );
        assert!(scan(&d.0, crate::docs::files::is_doc, Some(now + 60)).is_empty());
    }
}
