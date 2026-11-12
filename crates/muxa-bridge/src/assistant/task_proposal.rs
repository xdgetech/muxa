//! A proposed Task (ADR 0060): its checks, shared by a Result's `task` and
//! `propose-task`. The text is untrusted: reasons name fields, never values.
use muxa_protocol::{
    Attachments, ErrorBody, ErrorCode, InboxItem, ItemKind, ItemState, TaskProposal, WorkItem,
};
use serde::Deserialize;

use crate::convert::error_body;
use crate::items::store::{ItemStore, StoredItem, new_id};
use crate::records::workspaces::Workspace;
use crate::routines::store::StateError;
use crate::task_records::Config;

/// The item file of the secretary's proposals: they belong to no run.
/// No Task has this id; record ids have six or more hex digits.
pub const SECRETARY_ITEMS: &str = "t-0";

/// Characters of a title.
pub const MAX_TITLE: usize = 200;
/// Bytes of a goal: it goes whole into the Task's goal, which keeps room
/// for the origin and the marker within `records::MAX_GOAL`.
pub const MAX_GOAL: usize = 4096;

/// `task` in `result.json`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawTask {
    pub workspace: Option<String>,
    pub repo: Option<String>,
    pub title: String,
    pub goal: String,
}

/// Pure. Every reason the title and the goal fail, named with `field`.
pub fn check_text(field: &str, title: &str, goal: &str, reasons: &mut Vec<String>) {
    super::result::one_line(&format!("{field}title"), title, MAX_TITLE, reasons);
    super::result::text(&format!("{field}goal"), goal, MAX_GOAL, reasons);
}

/// Blocking: resolves the repo. Why `repo` is not one of `ws`'s repos.
pub fn check_repo(ws: &Workspace, repo: &str) -> Result<(), String> {
    let path = ws.repo_path(repo).map_err(|e| e.to_string())?;
    let main = path
        .canonicalize()
        .map_err(|_| format!("no repo {repo:?} in workspace {:?}", ws.name))?;
    if ws.has_repo(&main) {
        Ok(())
    } else {
        Err(format!(
            "{repo:?} is not a repo of workspace {:?}: name one under its root or in its repos",
            ws.name
        ))
    }
}

/// Blocking. A Result's `task`, checked against the request's workspace
/// `ws` (None: it takes no proposals). A Task in another workspace is the
/// user's to ask for (ADR 0051 §4).
pub fn from_result(
    raw: RawTask,
    ws: Option<&Workspace>,
    reasons: &mut Vec<String>,
) -> Option<TaskProposal> {
    let Some(ws) = ws else {
        reasons.push("task: this workspace takes no Task proposals; leave task out.".into());
        return None;
    };
    let before = reasons.len();
    if raw.workspace.as_deref().is_some_and(|w| w != ws.name) {
        reasons.push(format!(
            "task.workspace must be {:?}, the request's workspace, or left out. A Task in \
             another workspace is for the user to ask for: write it in openQuestions.",
            ws.name
        ));
    }
    check_text("task.", &raw.title, &raw.goal, reasons);
    if let Some(repo) = raw.repo.as_deref() {
        // The reason never repeats the agent's text.
        if check_repo(ws, repo).is_err() {
            reasons.push(
                "task.repo must name one of this workspace's repos, under its root or in its \
                 repos; leave repo out for a Task without a repo."
                    .into(),
            );
        }
    }
    (reasons.len() == before).then(|| TaskProposal {
        workspace: ws.name.clone(),
        repo: raw.repo,
        title: raw.title,
        goal: raw.goal,
        work: None,
        step: None,
        worker: None,
    })
}

/// What `propose-task` asks for.
#[derive(Debug, Clone)]
pub struct Propose {
    pub workspace: String,
    pub repo: Option<String>,
    pub title: String,
    pub goal: String,
    pub work: Option<String>,
}

/// Blocking. `propose-task` (ADR 0060 §3): one open `new-task` item for
/// the proposal. The same proposal while its item is open returns that item.
pub fn propose(config: &Config, p: Propose) -> Result<InboxItem, ErrorBody> {
    let bad = |m: String| error_body(ErrorCode::BadRequest, m);
    let mut reasons = Vec::new();
    check_text("", &p.title, &p.goal, &mut reasons);
    if !reasons.is_empty() {
        return Err(bad(reasons.join(" ")));
    }
    let workspaces = config.load_workspaces()?;
    let ws = workspaces.get(&p.workspace).ok_or_else(|| {
        bad(format!(
            "no workspace named {:?}; muxa-cli tasks lists them",
            p.workspace
        ))
    })?;
    if let Some(repo) = p.repo.as_deref() {
        check_repo(ws, repo).map_err(bad)?;
    }
    if let Some(work) = p.work.as_deref() {
        let known = super::result::is_work_id(work)
            && super::shelf::<WorkItem>(config)?
                .get(work)
                .map_err(|e| super::state_error(&e))?
                .is_some();
        if !known {
            return Err(bad(format!(
                "no Work item {work}; muxa-cli work lists them"
            )));
        }
    }
    let proposal = TaskProposal {
        workspace: p.workspace,
        repo: p.repo,
        title: p.title,
        goal: p.goal,
        work: p.work,
        step: None,
        worker: None,
    };
    let dir = config
        .state_dir
        .as_ref()
        .ok_or_else(|| error_body(ErrorCode::Internal, "no state directory"))?;
    let store = ItemStore::open(dir).map_err(|e| super::state_error(&e))?;
    let at = crate::records::now();
    let made = store
        .update::<_, StateError>(SECRETARY_ITEMS, |items| {
            let mut item = item_for(proposal, new_id()?, at);
            let key = format!("new-task:{}", super::proposals::hash_of(&item));
            let open = items.items.iter().find(|s| {
                s.key.starts_with(&key)
                    && matches!(s.item.state, ItemState::Open | ItemState::Later)
            });
            if let Some(s) = open {
                return Ok((crate::items::shown(&s.item), false));
            }
            item.answers = crate::items::answers(&item);
            items.items.push(StoredItem {
                item: item.clone(),
                key: format!("{key}:{at}"),
            });
            Ok((item, true))
        })
        .map_err(|e| super::state_error(&e))?;
    tracing::info!(item = %made.0.id, new = made.1, "task proposed");
    Ok(made.0)
}

fn item_for(p: TaskProposal, id: String, at: u64) -> InboxItem {
    let one_line = crate::items::produce::one_line;
    let max = crate::items::produce::MAX_QUESTION;
    InboxItem {
        id,
        kind: ItemKind::NewTask,
        state: ItemState::Open,
        question: one_line(
            &format!(
                "The secretary proposes a Task in {}: {}",
                p.workspace, p.title
            ),
            max,
        ),
        workspace: p.workspace.clone(),
        title: one_line(&p.title, max),
        created_at: at,
        read_at: None,
        later_at: None,
        until: None,
        protected_at: None,
        answered_at: None,
        answer: None,
        summary: Some(p.goal.clone()),
        task: None,
        session: None,
        pane: None,
        prompt: None,
        attachments: Attachments::default(),
        routine: None,
        proposal: None,
        sort: None,
        delegate: None,
        hypothesis: None,
        handoff: None,
        write: None,
        memory: None,
        result: None,
        new_task: Some(p),
        answers: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(workspace: Option<&str>, repo: Option<&str>, title: &str, goal: &str) -> RawTask {
        RawTask {
            workspace: workspace.map(str::to_owned),
            repo: repo.map(str::to_owned),
            title: title.into(),
            goal: goal.into(),
        }
    }

    fn workspace(root: &std::path::Path) -> Workspace {
        Workspace {
            name: "muxa".into(),
            root: root.canonicalize().unwrap(),
            repos: Vec::new(),
            missing_repos: Vec::new(),
            agent: None,
            loop_checks: None,
            park_after: None,
            routines: None,
            assistant: None,
            role: Default::default(),
            memory: muxa_protocol::MemoryMode::Review,
        }
    }

    #[test]
    fn a_task_in_the_request_workspace_with_one_of_its_repos_passes() {
        let root = std::env::temp_dir().join(format!("muxa-task-proposal-{}", std::process::id()));
        std::fs::create_dir_all(root.join("app/.git")).unwrap();
        let ws = workspace(&root);
        let mut reasons = Vec::new();
        let p = from_result(
            raw(Some("muxa"), Some("app"), "Fix it", "Fix the crash."),
            Some(&ws),
            &mut reasons,
        );
        assert!(reasons.is_empty(), "{reasons:?}");
        assert_eq!(
            p.map(|p| (p.workspace, p.repo)),
            Some(("muxa".into(), Some("app".into())))
        );
        let none = from_result(
            raw(None, None, "Note", "Write it down."),
            Some(&ws),
            &mut reasons,
        );
        assert_eq!(none.map(|p| p.repo), Some(None), "no repo: a folder Task");

        let mut reasons = Vec::new();
        let p = from_result(
            raw(Some("other"), Some("missing"), "two\nlines", ""),
            Some(&ws),
            &mut reasons,
        );
        assert!(p.is_none());
        let all = reasons.join(" | ");
        for want in ["task.workspace", "task.title", "task.goal", "task.repo"] {
            assert!(all.contains(want), "{want} in {all}");
        }
        let mut reasons = Vec::new();
        assert!(from_result(raw(None, None, "t", "g"), None, &mut reasons).is_none());
        assert_eq!(reasons.len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }
}
