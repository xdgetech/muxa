use std::sync::Arc;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Labels, Launch, SessionBackend};
use muxa_server::Inbox;
use muxa_server::holds::holds;
use muxa_server::tasks::DIR;

use super::*;
use crate::v2::adopt::tests::{git, repo, temp};

struct Host {
    root: PathBuf,
    main: PathBuf,
    config: Arc<Config>,
    backend: FakeBackend,
    inbox: Inbox,
}

fn host(name: &str) -> Host {
    let root = temp(&format!("disk-{name}"));
    let main = root.join("r");
    repo(&main);
    let file = root.join("workspaces.toml");
    std::fs::write(
        &file,
        format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n",
            root.display()
        ),
    )
    .unwrap();
    Host {
        config: Arc::new(Config {
            state_dir: Some(root.join("state")),
            workspaces: Some(file),
            home: None,
        }),
        root,
        main,
        backend: FakeBackend::new(),
        inbox: Inbox::memory(),
    }
}

impl Host {
    fn worktree(&self, at: &Path, branch: &str) -> PathBuf {
        git(
            &self.main,
            &["worktree", "add", "-q", "-b", branch, at.to_str().unwrap()],
        );
        at.canonicalize().unwrap()
    }

    fn task_worktree(&self, branch: &str) -> PathBuf {
        let path = self.worktree(&self.root.join(format!(".worktrees/r-{branch}")), branch);
        place_records::update(self.config.state_dir.as_ref().unwrap(), |l| {
            l.put(PlaceRecord {
                workspace: "w".into(),
                kind: PlaceKind::Branch,
                path: path.clone(),
                branch: Some(branch.into()),
                build: vec![],
                made: 1,
                ended_since: None,
            })
        })
        .unwrap();
        path
    }

    async fn rows(&self) -> Vec<DiskRow> {
        let h = holds(&self.backend, &self.inbox).await.unwrap();
        report(&self.config, &h).await.unwrap()
    }

    async fn clean(
        &self,
        paths: &[&Path],
        force: bool,
        dry_run: bool,
    ) -> Result<DiskCleaned, TaskError> {
        let h = holds(&self.backend, &self.inbox).await.unwrap();
        let req = DiskClean {
            paths: paths.iter().map(|p| p.display().to_string()).collect(),
            force,
            dry_run,
        };
        clean(&self.config, &h, req).await
    }
}

fn row<'a>(rows: &'a [DiskRow], path: &Path) -> &'a DiskRow {
    let p = path.display().to_string();
    rows.iter()
        .find(|r| r.path == p)
        .unwrap_or_else(|| panic!("no row {p} in {rows:?}"))
}

#[tokio::test]
async fn the_report_lists_task_places_other_worktrees_and_loose_folders() {
    let h = host("report");
    let task = h.task_worktree("fix");
    std::fs::write(task.join("big"), vec![b'x'; 8192]).unwrap();
    git(&task, &["add", "big"]);
    git(
        &task,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "b",
        ],
    );
    git(&h.main, &["merge", "-q", "--ff-only", "fix"]);
    let dirty = h.task_worktree("dirty");
    std::fs::write(dirty.join("new.txt"), "x\n").unwrap();
    let claude = h.worktree(
        &h.main.join(".claude/worktrees/agent-1"),
        "worktree-agent-1",
    );
    let folder = h.root.join("tasks/old-t-000001");
    std::fs::create_dir_all(&folder).unwrap();
    let rows = h.rows().await;
    let t = row(&rows, &task);
    assert_eq!(t.kind, DiskKind::TaskWorktree);
    assert_eq!(
        (t.clean, t.merged, t.cleanable),
        (Some(true), Some(true), true)
    );
    assert!(t.bytes.unwrap() >= 8192);
    assert!(!row(&rows, &dirty).cleanable);
    let c = row(&rows, &claude);
    assert_eq!(c.kind, DiskKind::ClaudeWorktree);
    assert!(!c.cleanable, "a worktree used today is not cleanable");
    let f = row(&rows, &folder.canonicalize().unwrap());
    assert_eq!((f.kind, f.cleanable), (DiskKind::TaskFolder, false));
}

#[tokio::test]
async fn clean_is_a_dry_run_first_and_removes_only_cleanable_rows() {
    let h = host("clean");
    let task = h.task_worktree("fix");
    let dirty = h.task_worktree("dirty");
    std::fs::write(dirty.join("new.txt"), "x\n").unwrap();
    let dry = h.clean(&[], false, true).await.unwrap();
    assert_eq!(dry.removed, vec![task.display().to_string()]);
    assert!(task.exists());
    let done = h.clean(&[], false, false).await.unwrap();
    assert_eq!(done.removed, vec![task.display().to_string()]);
    assert!(!task.exists() && dirty.exists());
    let records = place_records::read(h.config.state_dir.as_ref().unwrap()).unwrap();
    assert!(records.get(&task).is_none());
    let kept = h.clean(&[&dirty], false, false).await.unwrap();
    assert!(kept.removed.is_empty() && kept.kept[0].why.contains("uncommitted"));
    let forced = h.clean(&[&dirty], true, false).await.unwrap();
    assert_eq!(forced.removed.len(), 1);
    assert!(!dirty.exists());
}

#[tokio::test]
async fn a_held_place_stays_and_an_unknown_path_is_refused() {
    let h = host("held");
    let task = h.task_worktree("fix");
    let mut labels = Labels::new();
    labels.insert(DIR.into(), task.display().to_string());
    h.backend
        .run(&Launch {
            argv: vec!["agent".into()],
            labels,
            ..Launch::default()
        })
        .await
        .unwrap();
    let done = h.clean(&[&task], true, false).await.unwrap();
    assert!(done.removed.is_empty());
    assert!(task.exists());
    let outside = h.root.join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    assert!(matches!(
        h.clean(&[&outside], true, false).await,
        Err(TaskError::Invalid(_))
    ));
    assert!(outside.exists());
}

#[test]
fn du_lines_are_kib() {
    assert_eq!(
        parse_du("12\t/a\n0\t/b c\nbad\n"),
        vec![(PathBuf::from("/a"), 12 * 1024), (PathBuf::from("/b c"), 0)]
    );
}
