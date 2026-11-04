//! A reading Task's unexpected changes (ADR 0034 §2): `git status
//! --porcelain` of the main checkout when a turn starts, compared when it
//! ends. The bridge only reports them; it never reverts a file. Blocking.
use std::collections::BTreeSet;
use std::path::Path;

use muxa_protocol::Attachments;
use muxa_tmux::AgentStatus;
use serde::{Deserialize, Serialize};

use super::evidence::{MAX_SUMMARY, cut, of_changes};
use super::turn::running_since;
use crate::changes::git;
use crate::records::Record;
use crate::routines::store::StateError;

use super::super::store::{ItemStore, TaskItems};

/// Where the turn's start status is kept, in the Task's item file.
const FIELD: &str = "readBaseline";
/// Status lines kept and compared; a larger change is reported as it is cut.
const MAX_LINES: usize = 2000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Baseline {
    /// The `since` of the `running` state that started the turn.
    since: u64,
    status: Vec<String>,
}

/// What a turn changed in the main checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Changed {
    /// New `git status --porcelain` lines, e.g. ` M src/lib.rs`.
    pub lines: Vec<String>,
}

impl Changed {
    /// The item's summary: the main checkout and the changed files. Repo
    /// metadata only, stored in the item; never logged.
    pub fn summary(&self, main: &Path) -> String {
        let mut out = format!(
            "The agent of this reading Task changed files in the main checkout {}. \
             muxa did not revert them. Branch moves the Task to a worktree; the changes stay \
             where they are.\n\n",
            main.display()
        );
        for line in &self.lines {
            out.push_str(line);
            out.push('\n');
        }
        cut(&out, MAX_SUMMARY)
    }

    /// The change count, from the main checkout's own diff. No docs or
    /// images: a client opens those through the pane, whose folder is not the repo.
    pub fn attachments(&self, main: &Path) -> Attachments {
        let paths: BTreeSet<&str> = self.lines.iter().filter_map(|l| path_of(l)).collect();
        let entries: Vec<_> = git::list(main)
            .entries
            .into_iter()
            .filter(|e| paths.contains(e.path.as_str()))
            .collect();
        let mut out = of_changes(&entries, None);
        if let Some(count) = out.changes.as_mut() {
            count.files = u32::try_from(self.lines.len()).unwrap_or(u32::MAX);
        }
        out.docs.clear();
        out.images.clear();
        out
    }
}

/// At a turn's start, remembers the main checkout's status once for that turn.
pub(super) fn note_start(
    store: &ItemStore,
    record: &Record,
    agent: Option<AgentStatus>,
) -> Result<(), StateError> {
    let (Some(main), Some(since)) = (record.repo.as_deref(), running_since(agent)) else {
        return Ok(());
    };
    if baseline(&store.get(&record.id)?).is_some_and(|b| b.since == since) {
        return Ok(());
    }
    let Some(status) = status(main) else {
        return Ok(());
    };
    store.update::<_, StateError>(&record.id, |t| {
        set(t, Some(&Baseline { since, status }));
        Ok(())
    })
}

/// At a turn's end (`ended`), the changes since its start. The start status
/// is used once. None without one, or when nothing new changed.
pub(super) fn take(
    store: &ItemStore,
    record: &Record,
    ended: u64,
) -> Result<Option<Changed>, StateError> {
    let Some(main) = record.repo.as_deref() else {
        return Ok(None);
    };
    let Some(start) = baseline(&store.get(&record.id)?).filter(|b| b.since <= ended) else {
        return Ok(None);
    };
    let Some(now) = status(main) else {
        return Ok(None);
    };
    store.update::<_, StateError>(&record.id, |t| {
        set(t, None);
        Ok(())
    })?;
    Ok(new_lines(&start.status, &now).map(|lines| Changed { lines }))
}

/// Lines in `now` that `before` did not have: a new file, or a new state of one.
fn new_lines(before: &[String], now: &[String]) -> Option<Vec<String>> {
    let old: BTreeSet<&String> = before.iter().collect();
    let new: Vec<String> = now.iter().filter(|l| !old.contains(l)).cloned().collect();
    (!new.is_empty()).then_some(new)
}

/// ` M a.rs`, `?? b.md`, `R  old -> new`: the path the line is about now.
fn path_of(line: &str) -> Option<&str> {
    let path = line.get(3..)?;
    Some(path.rsplit(" -> ").next().unwrap_or(path))
}

fn status(main: &Path) -> Option<Vec<String>> {
    let out = git::git(main)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .take(MAX_LINES)
            .map(|l| {
                l.chars()
                    .map(|c| if c.is_control() { '?' } else { c })
                    .collect()
            })
            .collect(),
    )
}

fn baseline(items: &TaskItems) -> Option<Baseline> {
    serde_json::from_value(items.extra.get(FIELD)?.clone()).ok()
}

fn set(items: &mut TaskItems, b: Option<&Baseline>) {
    match b.and_then(|b| serde_json::to_value(b).ok()) {
        Some(v) => {
            items.extra.insert(FIELD.into(), v);
        }
        None => {
            items.extra.remove(FIELD);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn only_lines_the_turn_added_count() {
        let before = lines(&[" M a.rs", "?? notes.md"]);
        assert_eq!(new_lines(&before, &before), None);
        assert_eq!(
            new_lines(&before, &lines(&[" M a.rs"])),
            None,
            "a revert is not new"
        );
        assert_eq!(
            new_lines(
                &before,
                &lines(&[" M a.rs", "?? notes.md", " M b.rs", "MM a.rs"])
            ),
            Some(lines(&[" M b.rs", "MM a.rs"]))
        );
    }

    #[test]
    fn a_line_names_the_path_it_is_about_now() {
        assert_eq!(path_of(" M src/a.rs"), Some("src/a.rs"));
        assert_eq!(path_of("?? b.md"), Some("b.md"));
        assert_eq!(path_of("R  old.rs -> new.rs"), Some("new.rs"));
        assert_eq!(path_of("x"), None);
    }

    #[test]
    fn a_baseline_round_trips_through_the_item_file() {
        let mut t = TaskItems::new("t-000001");
        let b = Baseline {
            since: 5,
            status: lines(&[" M a.rs"]),
        };
        set(&mut t, Some(&b));
        let back: TaskItems = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!(baseline(&back), Some(b));
        set(&mut t, None);
        assert_eq!(baseline(&t), None);
    }
}
