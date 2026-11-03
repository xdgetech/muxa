//! A branch for a repo Task that names none (ADR 0031): the title's slug,
//! with `-2`, `-3`... when that branch or its worktree exists already.

use std::path::Path;

use crate::records::workspaces::slug;

use super::TaskError;
use super::builtin::{current_branch, has_ref, worktree_path};

/// Names tried before giving up.
const MAX_TRIES: u32 = 100;

/// The `n`th name for `title`: its slug, then `<slug>-<n>` from 2.
pub fn candidate(title: &str, n: u32) -> String {
    let base = slug(title);
    if n <= 1 { base } else { format!("{base}-{n}") }
}

/// The first candidate that no local or `origin` branch, worktree folder or
/// the main checkout's current branch uses. `main` is the main checkout.
pub async fn from_title(main: &Path, title: &str) -> Result<String, TaskError> {
    if title.trim().is_empty() {
        return Err(TaskError::Invalid(
            "a task without a branch needs a title to name one".into(),
        ));
    }
    let current = current_branch(main).await;
    for n in 1..=MAX_TRIES {
        let name = candidate(title, n);
        if !taken(main, &name, current.as_deref()).await {
            return Ok(name);
        }
    }
    Err(TaskError::Invalid(format!(
        "every branch for {:?} up to -{MAX_TRIES} exists; give a branch",
        candidate(title, 1)
    )))
}

async fn taken(main: &Path, name: &str, current: Option<&str>) -> bool {
    current == Some(name)
        || worktree_path(main, name).symlink_metadata().is_ok()
        || has_ref(main, &format!("refs/heads/{name}")).await
        || has_ref(main, &format!("refs/remotes/origin/{name}")).await
}
