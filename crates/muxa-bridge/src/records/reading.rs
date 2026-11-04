//! The record fields of a reading Task (ADR 0034): `read`, and `folder`, the
//! Task's own folder, which stays after Branch moves `dir` to a worktree.
//! Both are additive record fields, as `worker` is.
use std::path::{Path, PathBuf};

use super::Record;

const READ: &str = "read";
const FOLDER: &str = "folder";

impl Record {
    /// A reading Task: it has `repo`, no `branch`, and works in its folder.
    pub fn is_read(&self) -> bool {
        self.extra.get(READ).and_then(serde_json::Value::as_bool) == Some(true)
    }

    pub fn set_read(&mut self, read: bool) {
        if read {
            self.extra
                .insert(READ.into(), serde_json::Value::Bool(true));
        } else {
            self.extra.remove(READ);
        }
    }

    /// The Task's folder, for a Task that started as a reading Task.
    pub fn folder(&self) -> Option<PathBuf> {
        self.extra
            .get(FOLDER)
            .and_then(serde_json::Value::as_str)
            .map(PathBuf::from)
    }

    pub fn set_folder(&mut self, folder: &Path) {
        self.extra.insert(
            FOLDER.into(),
            serde_json::Value::String(folder.to_string_lossy().into_owned()),
        );
    }

    /// The folder when the agent no longer works there: a branched Task.
    pub fn earlier_folder(&self) -> Option<PathBuf> {
        self.folder()
            .filter(|f| self.dir.as_deref().is_none_or(|d| d != f.as_path()))
    }
}

/// The `extra` of a new reading Task's record.
pub fn new_fields() -> serde_json::Map<String, serde_json::Value> {
    let mut extra = serde_json::Map::new();
    extra.insert(READ.into(), serde_json::Value::Bool(true));
    extra
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record() -> Record {
        serde_json::from_value(serde_json::json!({
            "version":1,"id":"t-000001","rev":1,"workspace":"w","title":"t",
            "goal":"","status":"open","createdAt":1,"updatedAt":1
        }))
        .unwrap()
    }

    #[test]
    fn read_and_folder_survive_a_rewrite_and_branch_clears_read() {
        let mut r = record();
        assert!(!r.is_read());
        r.extra = new_fields();
        r.dir = Some("/w/tasks/t-1".into());
        r.set_folder(Path::new("/w/tasks/t-1"));
        let back: Record = serde_json::from_str(&serde_json::to_string(&r).unwrap()).unwrap();
        assert!(back.is_read());
        assert_eq!(back.earlier_folder(), None, "the agent still works there");
        let mut branched = back;
        branched.set_read(false);
        branched.dir = Some("/w/.worktrees/repo-x".into());
        assert!(!branched.is_read());
        assert_eq!(
            branched.earlier_folder().as_deref(),
            Some(Path::new("/w/tasks/t-1"))
        );
    }
}
