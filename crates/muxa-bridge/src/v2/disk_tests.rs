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

const ID: [&str; 4] = ["-c", "user.name=t", "-c", "user.email=t@t"];

fn commit(dir: &Path, msg: &str) {
    let mut args = ID.to_vec();
    args.extend(["commit", "-q", "--allow-empty", "-m", msg]);
    git(dir, &args);
}

#[tokio::test]
async fn a_worktree_with_submodules_goes_only_when_they_hold_nothing() {
    let h = host("submodule");
    let lib = h.root.join("lib");
    repo(&lib);
    git(
        &h.main,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            lib.to_str().unwrap(),
            "lib",
        ],
    );
    commit(&h.main, "lib");
    let init = |at: &Path| {
        git(
            at,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "update",
                "--init",
                "-q",
            ],
        )
    };
    let clean = h.task_worktree("clean");
    init(&clean);
    let held = h.task_worktree("held");
    init(&held);
    // A commit on a local branch of the submodule only, then back: the
    // worktree looks clean, but removing it would lose that commit.
    let sub = held.join("lib");
    let at = String::from_utf8(
        std::process::Command::new("git")
            .arg("-C")
            .arg(&sub)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    git(&sub, &["checkout", "-q", "-b", "mine"]);
    commit(&sub, "mine");
    git(&sub, &["checkout", "-q", "--detach", at.trim()]);
    let done = h.clean(&[], false, false).await.unwrap();
    assert_eq!(done.removed, vec![clean.display().to_string()]);
    assert!(!clean.exists() && held.exists());
    let kept = h.clean(&[&held], false, false).await.unwrap();
    assert!(kept.kept[0].why.contains("submodule lib"), "{kept:?}");
}

#[tokio::test]
async fn a_build_disk_muxa_may_not_write_to_is_not_read() {
    use std::os::unix::fs::PermissionsExt;
    let h = host("blocked-disk");
    let ssd = h.root.join("ssd");
    let leftover = ssd.join("w/r-old");
    std::fs::create_dir_all(leftover.join("target")).unwrap();
    let file = h.config.workspaces.clone().unwrap();
    let toml = std::fs::read_to_string(&file).unwrap();
    std::fs::write(
        &file,
        format!("[build]\ndir = \"{}\"\n\n{toml}", ssd.display()),
    )
    .unwrap();
    let listed = |rows: &[DiskRow]| {
        rows.iter()
            .any(|r| r.path == leftover.display().to_string())
    };
    assert!(
        listed(&h.rows().await),
        "control: a usable disk lists its loose folder"
    );

    std::fs::set_permissions(&ssd, std::fs::Permissions::from_mode(0o500)).unwrap();
    let rows = h.rows().await;
    std::fs::set_permissions(&ssd, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!listed(&rows));
}
