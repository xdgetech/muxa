//! A published assistant run's folder stays so its result files can be read.
//! It goes when the request's next version publishes, or after
//! [`KEEP`] (ADR 0048 §5). A failed, replaced or cancelled run keeps its
//! folder for inspection, as a failed Routine run does (ADR 0037 §2).
use std::path::PathBuf;

use muxa_protocol::AssistantRequest;

use crate::records::{Record, Status, Workspaces};

/// Seconds a published run's folder is kept: 7 days.
pub const KEEP: u64 = 7 * 86_400;

/// The folders of `q`'s published runs that can go. Only a done record whose
/// session is closed counts, and only when its `dir` is the folder
/// `assistant_dir` checks inside the workspace root.
pub fn leftovers(
    workspaces: &Workspaces,
    q: &AssistantRequest,
    runs: &[&Record],
    now: u64,
) -> Vec<PathBuf> {
    let Some(ws) = workspaces.get(&q.workspace) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for r in runs {
        let Some(run) = r.request_run() else {
            continue;
        };
        if r.status != Status::Done || r.session.is_some() {
            continue;
        }
        let newer = q.results.iter().any(|x| x.version > run.version);
        if !newer && now.saturating_sub(r.updated_at) < KEEP {
            continue;
        }
        let Ok(dir) = ws.assistant_dir(&r.title, &r.id) else {
            continue;
        };
        if r.dir.as_deref() == Some(dir.as_path()) && dir.is_dir() {
            out.push(dir);
        }
    }
    out
}

/// Removes each folder; a failure is logged and tried again next tick.
pub async fn remove(dirs: Vec<PathBuf>) {
    for dir in dirs {
        let shown = dir.display().to_string();
        match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&dir)).await {
            Ok(Ok(())) => {
                tracing::info!(dir = %shown, "removed a published assistant run's folder")
            }
            Ok(Err(e)) => {
                tracing::warn!(dir = %shown, error = %e, "cannot remove an assistant run's folder")
            }
            Err(e) => tracing::warn!(dir = %shown, error = %e, "removing a run's folder stopped"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::record::{RequestRun, RunFor};
    use muxa_protocol::{AssistResult, AssistScope, Delivery};

    fn setup(name: &str) -> (PathBuf, Workspaces) {
        let root =
            std::env::temp_dir().join(format!("muxa-a-cleanup-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let text = format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n",
            root.display()
        );
        let file = root.join("workspaces.toml");
        std::fs::write(&file, text).unwrap();
        (root, Workspaces::load(&file, None).unwrap())
    }

    fn run(ws: &Workspaces, id: &str, version: u64, status: Status, updated: u64) -> Record {
        let fields = RunFor::Request(RequestRun {
            request: "q-0000000a".into(),
            version,
            work: "w-0000000b".into(),
            notes_base: None,
            step: None,
            member: None,
            shared_base: None,
        })
        .fields();
        let dir = ws.get("w").unwrap().assistant_dir("Plan", id).unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let mut r: Record = serde_json::from_value(serde_json::json!({
            "version": 1, "id": id, "rev": 1, "workspace": "w", "title": "Plan",
            "goal": "g", "status": "open", "createdAt": 1, "updatedAt": updated,
            "dir": dir,
        }))
        .unwrap();
        r.status = status;
        r.extra.extend(fields);
        r
    }

    fn request(results: &[u64]) -> AssistantRequest {
        AssistantRequest {
            steps: Vec::new(),
            id: "q-0000000a".into(),
            work: "w-0000000b".into(),
            version: 3,
            workspace: "w".into(),
            scope: AssistScope::Research,
            delivery: Delivery::Resulted,
            delivery_at: 1,
            created_at: 1,
            updated_at: 1,
            purpose: None,
            run: None,
            results: results
                .iter()
                .map(|v| AssistResult {
                    writes: Vec::new(),
                    worker: None,
                    step: None,
                    version: *v,
                    at: 1,
                    summary: "s".into(),
                    stale: false,
                    findings: Vec::new(),
                    open_questions: Vec::new(),
                    next: Vec::new(),
                    files: Vec::new(),
                    possible_duplicates: Vec::new(),
                    notes: None,
                })
                .collect(),
        }
    }

    #[test]
    fn a_published_folder_goes_after_a_newer_publish_or_a_week() {
        let (root, ws) = setup("rule");
        let now = 10 * 86_400;
        let older = run(&ws, "t-00000a", 1, Status::Done, now);
        let newest = run(&ws, "t-00000b", 2, Status::Done, now);
        let failed = run(&ws, "t-00000c", 1, Status::Dropped, 1);
        let old = run(&ws, "t-00000d", 2, Status::Done, now - KEEP);
        let mut open = run(&ws, "t-00000e", 1, Status::Done, now);
        open.session = Some("s".into());
        let runs = [&older, &newest, &failed, &old, &open];
        let gone = leftovers(&ws, &request(&[2, 1]), &runs, now);
        assert_eq!(
            gone,
            vec![older.dir.clone().unwrap(), old.dir.clone().unwrap()]
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
