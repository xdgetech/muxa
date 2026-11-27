//! `<state dir>/places.json`: the places muxa made for Tasks and their build
//! folders (ADR 0102 §2). The sweep removes only places listed here.

use std::fs::DirBuilder;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use muxa_protocol::v2::{Place, PlaceKind};
use serde::{Deserialize, Serialize};

use crate::records::build::BuildDisk;
use crate::records::lock::Lock;
use crate::routines::store::{StateError, read_json, write_json};

const ID: &str = "places";
const VERSION: u32 = 1;
const DIR_MODE: u32 = 0o700;
/// Paths and times only; thousands of places fit.
const MAX_FILE: u64 = 4 << 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaceRecord {
    pub workspace: String,
    pub kind: PlaceKind,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// Folders on the build disk that this place's links point to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build: Vec<PathBuf>,
    /// Unix seconds.
    pub made: u64,
    /// Unix seconds when the sweep first saw only ended agents hold it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_since: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Places {
    #[serde(default)]
    version: u32,
    /// The first sweep added the places of the terminals it found.
    #[serde(default)]
    pub seeded: bool,
    #[serde(default)]
    pub places: Vec<PlaceRecord>,
}

impl Places {
    /// Adds `record`, or replaces the record of the same path.
    pub fn put(&mut self, record: PlaceRecord) {
        match self.places.iter_mut().find(|p| p.path == record.path) {
            Some(old) => *old = record,
            None => self.places.push(record),
        }
    }

    pub fn drop_path(&mut self, path: &Path) {
        self.places.retain(|p| p.path != path);
    }

    pub fn get(&self, path: &Path) -> Option<&PlaceRecord> {
        self.places.iter().find(|p| p.path == path)
    }
}

/// Blocking. The file as it is; empty when there is none.
pub fn read(state_dir: &Path) -> Result<Places, StateError> {
    Ok(read_json(&state_dir.join(format!("{ID}.json")), ID, MAX_FILE)?.unwrap_or_default())
}

/// Blocking, under the file's lock: reads, changes and writes the file.
pub fn update<T>(state_dir: &Path, f: impl FnOnce(&mut Places) -> T) -> Result<T, StateError> {
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(state_dir)
        .map_err(|source| StateError::Io {
            action: "create",
            path: state_dir.to_owned(),
            source,
        })?;
    let _lock = Lock::acquire(state_dir, ID, true)?;
    let mut places = read(state_dir)?;
    let out = f(&mut places);
    places.version = VERSION;
    write_json(state_dir, ID, &places, MAX_FILE)?;
    Ok(out)
}

/// [`update`] that only logs a failure: the caller's work is done already.
pub fn note(state_dir: Option<&Path>, f: impl FnOnce(&mut Places)) {
    let Some(dir) = state_dir else { return };
    if let Err(e) = update(dir, f) {
        tracing::warn!(error = %e, "cannot update the list of Task places");
    }
}

/// After a Task's places are made: links each worktree's build paths to
/// the build disk and records the worktrees and folders. Returns why the
/// build disk could not be used; the places then build in place. Blocking.
pub fn after_make(
    state_dir: Option<&Path>,
    disk: Option<&BuildDisk>,
    workspace: &str,
    places: &[Place],
    now: u64,
) -> Option<String> {
    let mut problem = None;
    let records: Vec<PlaceRecord> = places
        .iter()
        .filter(|p| matches!(p.kind, PlaceKind::Branch | PlaceKind::Folder))
        .map(|p| {
            let path = PathBuf::from(&p.path);
            let linked = match (p.kind, disk) {
                (PlaceKind::Branch, Some(d)) => super::build_dirs::link(d, workspace, &path),
                _ => Ok(Vec::new()),
            };
            let build = linked.unwrap_or_else(|why| {
                tracing::warn!(path = %path.display(), error = %why, "builds stay in the place");
                problem.get_or_insert(why);
                Vec::new()
            });
            PlaceRecord {
                workspace: workspace.to_owned(),
                kind: p.kind,
                path,
                branch: p.branch.clone(),
                build,
                made: now,
                ended_since: None,
            }
        })
        .collect();
    note(state_dir, |list| {
        for r in records {
            let made = list.get(&r.path).map_or(r.made, |old| old.made);
            list.put(PlaceRecord { made, ..r });
        }
    });
    problem
}

/// After Close Task: forgets the worktrees that are gone and returns their
/// build folders, for the caller to remove. Blocking.
pub fn after_close(state_dir: Option<&Path>, places: &[Place]) -> Vec<PathBuf> {
    let Some(dir) = state_dir else {
        return Vec::new();
    };
    let gone: Vec<PathBuf> = places
        .iter()
        .filter(|p| p.kind == PlaceKind::Branch)
        .map(|p| PathBuf::from(&p.path))
        .filter(|p| !p.exists())
        .collect();
    if gone.is_empty() {
        return Vec::new();
    }
    let dropped = update(dir, |list| {
        let mut build = Vec::new();
        for path in &gone {
            if let Some(r) = list.get(path) {
                build.extend(r.build.iter().cloned());
            }
            list.drop_path(path);
        }
        build
    });
    dropped.unwrap_or_else(|e| {
        tracing::warn!(error = %e, "cannot update the list of Task places");
        Vec::new()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str) -> PlaceRecord {
        PlaceRecord {
            workspace: "w".into(),
            kind: PlaceKind::Branch,
            path: path.into(),
            branch: Some("fix".into()),
            build: vec![],
            made: 1,
            ended_since: None,
        }
    }

    #[test]
    fn put_replaces_the_same_path_and_the_file_round_trips() {
        let dir = crate::v2::adopt::tests::temp("place-records");
        update(&dir, |p| {
            p.put(record("/a"));
            p.put(record("/b"));
            p.put(PlaceRecord {
                made: 2,
                ..record("/a")
            });
        })
        .unwrap();
        let read = read(&dir).unwrap();
        assert_eq!(read.places.len(), 2);
        assert_eq!(read.get(Path::new("/a")).map(|r| r.made), Some(2));
        update(&dir, |p| p.drop_path(Path::new("/a"))).unwrap();
        assert!(read_back(&dir).get(Path::new("/a")).is_none());
    }

    #[test]
    fn after_close_forgets_gone_worktrees_and_leaves_their_build_folders_to_the_caller() {
        let state = crate::v2::adopt::tests::temp("after-close");
        let kept = state.join("kept-worktree");
        std::fs::create_dir_all(&kept).unwrap();
        let gone = state.join("gone-worktree");
        let build = state.join("build").join("target");
        std::fs::create_dir_all(&build).unwrap();
        update(&state, |p| {
            p.put(PlaceRecord {
                path: gone.clone(),
                build: vec![build.clone()],
                ..record("")
            });
            p.put(PlaceRecord {
                path: kept.clone(),
                ..record("")
            });
        })
        .unwrap();
        let place = |path: &Path| Place {
            kind: PlaceKind::Branch,
            path: path.display().to_string(),
            branch: Some("fix".into()),
        };

        let returned = after_close(Some(&state), &[place(&gone), place(&kept)]);

        assert_eq!(returned, vec![build.clone()]);
        assert!(build.is_dir(), "the caller removes the build folders");
        let list = read_back(&state);
        assert!(list.get(&gone).is_none());
        assert!(list.get(&kept).is_some());
    }

    fn read_back(dir: &Path) -> Places {
        read(dir).unwrap()
    }
}
