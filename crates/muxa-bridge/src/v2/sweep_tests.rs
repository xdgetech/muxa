use std::path::Path;

use muxa_backend::fake::FakeBackend;
use muxa_backend::{Exit, Labels, Launch, TerminalId};
use muxa_protocol::v2::ItemKind;
use muxa_server::tasks::{DIR, PLACES, WORKSPACE};

use super::*;
use crate::v2::adopt::tests::{git, repo, temp};

const DAY: u64 = 86_400;

struct Host {
    root: PathBuf,
    main: PathBuf,
    config: Arc<Config>,
    inbox: Inbox,
    backend: FakeBackend,
}

fn host(name: &str) -> Host {
    let root = temp(&format!("sweep-{name}"));
    let main = root.join("r");
    repo(&main);
    let state = root.join("state");
    Host {
        config: Arc::new(Config {
            state_dir: Some(state),
            workspaces: None,
            home: None,
        }),
        root,
        main,
        inbox: Inbox::memory(),
        backend: FakeBackend::new(),
    }
}

impl Host {
    /// A worktree on `branch`, recorded as a Task place.
    fn worktree(&self, branch: &str) -> PathBuf {
        let path = self.root.join(".worktrees").join(format!("r-{branch}"));
        git(
            &self.main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                branch,
                path.to_str().unwrap(),
            ],
        );
        path
    }

    fn record(&self, path: &Path, branch: &str, build: Vec<PathBuf>, ended_since: Option<u64>) {
        place_records::update(self.config.state_dir.as_ref().unwrap(), |l| {
            l.put(PlaceRecord {
                workspace: "w".into(),
                kind: PlaceKind::Branch,
                path: path.to_owned(),
                branch: Some(branch.into()),
                build,
                made: 1,
                ended_since,
            });
            l.seeded = true;
        })
        .unwrap();
    }

    fn records(&self) -> Places {
        place_records::read(self.config.state_dir.as_ref().unwrap()).unwrap()
    }

    async fn agent_in(&self, path: &Path) -> TerminalId {
        let places = serde_json::json!([{"kind": "branch", "path": path, "branch": "x"}]);
        let mut labels = Labels::new();
        labels.insert(PLACES.into(), places.to_string());
        labels.insert(DIR.into(), path.display().to_string());
        labels.insert(WORKSPACE.into(), "w".into());
        let t = self
            .backend
            .run(&Launch {
                argv: vec!["agent".into()],
                labels,
                ..Launch::default()
            })
            .await
            .unwrap();
        t.id
    }

    async fn sweep(&self) -> Swept {
        sweep(&self.backend, &self.config, &self.inbox)
            .await
            .unwrap()
    }

    fn decide_items(&self) -> usize {
        self.inbox
            .view()
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Decide)
            .count()
    }
}

fn commit(dir: &Path, file: &str) {
    std::fs::write(dir.join(file), "y\n").unwrap();
    git(dir, &["add", "."]);
    git(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "-m",
            "c",
        ],
    );
}

#[tokio::test]
async fn a_free_clean_merged_worktree_goes_with_its_branch_and_build_folder() {
    let h = host("merged");
    let tree = h.worktree("fix");
    let build = h.root.join("ssd/w/r-fix/target");
    std::fs::create_dir_all(build.join("debug")).unwrap();
    h.record(&tree, "fix", vec![build.clone()], None);
    let swept = h.sweep().await;
    assert_eq!(swept.removed, vec![tree.clone()]);
    assert!(!tree.exists());
    assert!(!build.exists());
    assert!(h.records().places.is_empty());
    let branches = std::process::Command::new("git")
        .arg("-C")
        .arg(&h.main)
        .args(["branch", "--list", "fix"])
        .output()
        .unwrap();
    assert!(branches.stdout.is_empty());
}

#[tokio::test]
async fn dirty_or_unmerged_work_stays_and_the_inbox_asks_once() {
    let h = host("losses");
    let dirty = h.worktree("dirty");
    std::fs::write(dirty.join("new.txt"), "x\n").unwrap();
    let ahead = h.worktree("ahead");
    commit(&ahead, "a.txt");
    h.record(&dirty, "dirty", vec![], None);
    h.record(&ahead, "ahead", vec![], None);
    let swept = h.sweep().await;
    assert!(swept.removed.is_empty());
    assert_eq!(swept.kept.len(), 2);
    assert!(dirty.join("new.txt").exists() && ahead.exists());
    assert_eq!(h.decide_items(), 2);
    h.sweep().await;
    assert_eq!(h.decide_items(), 2);
}

#[tokio::test]
async fn a_live_terminal_holds_its_places() {
    let h = host("live");
    let tree = h.worktree("held");
    h.record(&tree, "held", vec![], None);
    h.agent_in(&tree).await;
    let swept = h.sweep().await;
    assert!(swept.removed.is_empty() && swept.kept.is_empty());
    assert!(tree.exists());
}

#[tokio::test]
async fn an_ended_agent_with_an_open_item_holds_its_places_but_loses_old_build_output() {
    let h = host("ended");
    let tree = h.worktree("done");
    let build = h.root.join("ssd/w/r-done/target");
    std::fs::create_dir_all(build.join("debug")).unwrap();
    let id = h.agent_in(&tree).await;
    h.backend
        .end(
            &id,
            Exit {
                reason: "exited".into(),
                code: Some(0),
                signal: None,
            },
        )
        .unwrap();
    h.inbox
        .change(|b, now| {
            b.add(
                NewItem {
                    kind: ItemKind::Review,
                    title: "Review".into(),
                    note: None,
                    workspace: None,
                    due: None,
                    source: "task".into(),
                    key: None,
                    once: false,
                    terminal: Some(id.as_str().to_owned()),
                    routine: None,
                    place: None,
                    extra: Default::default(),
                },
                now,
            )
            .map(drop)
        })
        .unwrap();
    let now = crate::records::now();
    h.record(&tree, "done", vec![build.clone()], Some(now - DAY));
    let swept = h.sweep().await;
    assert!(swept.removed.is_empty() && swept.emptied.is_empty());
    assert!(build.join("debug").exists());
    h.record(&tree, "done", vec![build.clone()], Some(now - 4 * DAY));
    let swept = h.sweep().await;
    assert_eq!(swept.emptied, vec![tree.clone()]);
    assert!(build.is_dir() && !build.join("debug").exists());
    assert!(tree.exists());
}

#[tokio::test]
async fn a_worktree_without_a_record_is_never_touched() {
    let h = host("unknown");
    let tree = h.worktree("mine");
    let swept = h.sweep().await;
    assert_eq!(swept, Swept::default());
    assert!(tree.exists());
}

#[tokio::test]
async fn the_first_sweep_records_the_places_of_running_agents() {
    let h = host("seed");
    let tree = h.worktree("running");
    h.agent_in(&tree).await;
    h.sweep().await;
    let records = h.records();
    assert!(records.seeded);
    assert!(records.get(&tree).is_some());
}

#[tokio::test]
async fn a_place_made_just_now_is_left_alone() {
    let h = host("grace");
    let tree = h.worktree("starting");
    place_records::update(h.config.state_dir.as_ref().unwrap(), |l| {
        l.put(PlaceRecord {
            workspace: "w".into(),
            kind: PlaceKind::Branch,
            path: tree.clone(),
            branch: Some("starting".into()),
            build: vec![],
            made: crate::records::now(),
            ended_since: None,
        });
        l.seeded = true;
    })
    .unwrap();
    assert_eq!(h.sweep().await, Swept::default());
    assert!(tree.exists());
}
