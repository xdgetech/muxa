//! Files outside git that a pane may read (ADR 0033 §4a): every document and
//! image of a Task folder, or else the paths the pane's Inbox items attach.
//! The scan never follows a symlink and stops at its limits. Blocking.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::files::mtime;
use crate::agents::PaneInfo;
use crate::task_records::Config;

/// Levels below the folder.
const MAX_DEPTH: usize = 6;
/// Directory entries one scan reads, as `records/repos.rs` does.
const MAX_ENTRIES: usize = 2000;
const MAX_SCAN: Duration = Duration::from_millis(250);
/// A Task folder is `<workspace root>/tasks/<slug>-<id>`.
const TASKS_DIR: &str = "tasks";

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

/// `rel` under `root` as a canonical path, when it is a file that stays
/// inside `root` after symlinks.
pub(crate) fn inside(root: &Path, rel: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let real = root.join(rel).canonicalize().ok()?;
    (real.starts_with(&root) && real.is_file()).then_some(real)
}

/// What a pane whose folder is not in a git repo serves, by `keep`, relative
/// to the pane's folder.
pub(crate) fn served(records: &Config, pane: &PaneInfo, keep: fn(&str) -> bool) -> Vec<Found> {
    let Ok(dir) = Path::new(&pane.path).canonicalize() else {
        return Vec::new();
    };
    if task_folder(records, &dir) {
        return scan(&dir, keep, None);
    }
    crate::items::evidence_paths(records, &pane.pane.to_string())
        .into_iter()
        .filter(|p| keep(p))
        .filter_map(|p| {
            let real = inside(&dir, Path::new(&p))?;
            Some(Found {
                mtime: mtime(&real),
                path: p,
            })
        })
        .collect()
}

/// `dir` is the folder of a repo-less or reading Task: a record's `dir`, with
/// no `branch`, in a `tasks/` folder.
fn task_folder(records: &Config, dir: &Path) -> bool {
    let in_tasks = dir
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|n| n == TASKS_DIR);
    in_tasks
        && records.records_if_any().iter().any(|r| {
            r.branch.is_none()
                && r.dir
                    .as_deref()
                    .and_then(|d| d.canonicalize().ok())
                    .is_some_and(|d| d == dir)
        })
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

    fn write(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "x").unwrap();
    }

    fn pane(path: &Path) -> PaneInfo {
        PaneInfo {
            pane: "%9".into(),
            session: "s".into(),
            pid: 1,
            path: path.to_string_lossy().into_owned(),
            shell_foreground: false,
        }
    }

    /// A state directory with one repo-less Task whose folder is `dir`.
    fn records(state: &Path, dir: Option<&Path>) -> Config {
        crate::records::Store::open(state)
            .unwrap()
            .create(crate::records::NewRecord {
                workspace: "w".into(),
                title: "T".into(),
                goal: "Go.\n".into(),
                project: None,
                repo: None,
                branch: None,
                dir: dir.map(Path::to_path_buf),
                session: None,
                note: None,
                worker: None,
                extra: serde_json::Map::new(),
            })
            .unwrap();
        Config {
            state_dir: Some(state.to_path_buf()),
            workspaces: None,
            home: None,
        }
    }

    #[test]
    fn a_task_folder_serves_all_its_documents() {
        let (state, root) = (dir(), dir());
        let folder = root.0.join("tasks/design-t-000001");
        write(&folder, "design.md");
        write(&folder, "notes/old.md");
        write(&folder, ".muxa/TASK.md");
        let config = records(&state.0, Some(&folder));
        let found = served(&config, &pane(&folder), crate::docs::is_doc);
        assert_eq!(paths(&found), ["design.md", "notes/old.md"]);

        // The same files in a folder no Task owns: only attached ones.
        let other = root.0.join("tasks/other");
        write(&other, "design.md");
        assert!(served(&config, &pane(&other), crate::docs::is_doc).is_empty());
    }

    #[test]
    fn another_folder_serves_only_what_its_items_attach() {
        use crate::items::store::{ItemStore, StoredItem, tests_item};
        let (state, home) = (dir(), dir());
        write(&home.0, "report.md");
        write(&home.0, "private.md");
        let config = records(&state.0, None);
        let task = config.records_if_any()[0].id.clone();
        ItemStore::open(&state.0)
            .unwrap()
            .update::<_, crate::routines::store::StateError>(&task, |t| {
                let mut item = tests_item("i-00000001", muxa_protocol::ItemState::Open);
                item.pane = Some("%9".into());
                item.attachments.docs = vec!["report.md".into(), "../escape.md".into()];
                t.items.push(StoredItem {
                    item,
                    key: "review:1".into(),
                });
                Ok(())
            })
            .unwrap();
        let found = served(&config, &pane(&home.0), crate::docs::is_doc);
        assert_eq!(paths(&found), ["report.md"]);
        let mut elsewhere = pane(&home.0);
        elsewhere.pane = "%10".into();
        assert!(served(&config, &elsewhere, crate::docs::is_doc).is_empty());
    }

    fn paths(found: &[Found]) -> Vec<&str> {
        let mut p: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        p.sort_unstable();
        p
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
        let keep = |p: &str| crate::docs::is_doc(p) || crate::docs::is_image(p);
        assert_eq!(paths(&scan(&d.0, keep, None)), ["out/chart.png", "plan.md"]);
    }

    #[test]
    fn since_keeps_only_files_changed_from_then() {
        let d = dir();
        write(&d.0, "old.md");
        let now = crate::records::now();
        assert_eq!(
            paths(&scan(&d.0, crate::docs::is_doc, Some(now))),
            ["old.md"]
        );
        assert!(scan(&d.0, crate::docs::is_doc, Some(now + 60)).is_empty());
    }

    #[test]
    fn inside_refuses_a_symlink_out_and_a_missing_file() {
        let d = dir();
        write(&d.0, "a/b.md");
        let outside = dir();
        write(&outside.0, "secret.md");
        std::os::unix::fs::symlink(outside.0.join("secret.md"), d.0.join("out.md")).unwrap();
        assert_eq!(inside(&d.0, Path::new("a/b.md")), Some(d.0.join("a/b.md")));
        assert_eq!(inside(&d.0, Path::new("out.md")), None);
        assert_eq!(inside(&d.0, Path::new("../x.md")), None);
        assert_eq!(inside(&d.0, Path::new("missing.md")), None);
        assert_eq!(inside(&d.0, Path::new("a")), None, "a folder");
    }
}
