//! Adopting existing places and migrating v1 Tasks through the server, on
//! the fake backend and temp dirs (ADR 0076).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use muxa_backend::SessionBackend;
use muxa_backend::fake::FakeBackend;
use muxa_protocol::v2::{Frame, Kind, PlaceKind, PlaceSpec};
use muxa_server::{Config, Host, Inbox, Services, Sessions, TaskError, TaskSpec, serve};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

use super::super::adopt::AgentLookup;
use super::super::adopt::tests::{git, repo};
use super::super::v1::tests::record;
use super::BridgeHost;

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    claude: PathBuf,
}

impl Fixture {
    /// A Workspace `w` whose agent is `claude`, a state dir and a Claude Code
    /// config dir, all under a temp dir.
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!("muxa-resume-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("root/tasks")).unwrap();
        std::fs::create_dir_all(base.join("state")).unwrap();
        let base = base.canonicalize().unwrap();
        std::fs::write(
            base.join("workspaces.toml"),
            format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n",
                base.join("root").display()
            ),
        )
        .unwrap();
        Fixture {
            root: base.join("root"),
            claude: base.join("claude"),
            base,
        }
    }

    fn host(&self, busy: Option<&Path>) -> BridgeHost {
        let config = crate::config::Config {
            state_dir: Some(self.base.join("state")),
            workspaces: Some(self.base.join("workspaces.toml")),
            home: None,
        };
        let busy = busy.map(Path::to_owned);
        let lookup: Arc<AgentLookup> = Arc::new(move |dir: &Path| {
            Ok(busy
                .as_deref()
                .filter(|b| *b == dir)
                .map(|_| ("claude".to_owned(), 4242)))
        });
        BridgeHost::new("claude".into(), Arc::new(config))
            .with_claude_dirs(vec![self.claude.clone()])
            .with_agent_lookup(lookup)
    }

    /// A conversation Claude Code kept for `dir`.
    fn conversation(&self, dir: &Path) {
        let escaped: String = dir
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        let folder = self.claude.join("projects").join(escaped);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("0b6e-11.jsonl"), "{}\n").unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn existing(path: &Path) -> TaskSpec {
    TaskSpec {
        workspace: "w".into(),
        title: "Fix login".into(),
        goal: None,
        places: vec![PlaceSpec::Existing {
            path: path.to_string_lossy().into_owned(),
        }],
        connections: Vec::new(),
        mcp: Vec::new(),
        plan: false,
        bot: None,
    }
}

#[tokio::test]
async fn a_folder_is_adopted_and_its_notes_are_kept() {
    let f = Fixture::new("folder");
    let dir = f.root.join("tasks/fix-login-t-00000a");
    std::fs::create_dir_all(dir.join(".muxa")).unwrap();
    std::fs::write(dir.join(".muxa/TASK.md"), "# Fix login\n\nMy notes.\n").unwrap();
    let p = f.host(None).prepare(&existing(&dir)).await.unwrap();
    assert_eq!(p.dir, dir);
    assert_eq!((p.places.len(), p.places[0].kind), (1, PlaceKind::Folder));
    assert_eq!((p.repo, p.branch), (None, None));
    assert_eq!(
        p.argv[2], "claude -- 'Read .muxa/TASK.md and continue the Task.'",
        "no conversation: no --continue"
    );
    let notes = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert_eq!(notes, "# Fix login\n\nMy notes.\n");

    std::fs::remove_file(dir.join(".muxa/TASK.md")).unwrap();
    f.host(None).prepare(&existing(&dir)).await.unwrap();
    let written = std::fs::read_to_string(dir.join(".muxa/TASK.md")).unwrap();
    assert!(written.starts_with("# Fix login\n"), "{written}");
    let tasks: Vec<_> = std::fs::read_dir(f.root.join("tasks")).unwrap().collect();
    assert_eq!(tasks.len(), 1, "no new folder");
}

#[tokio::test]
async fn a_worktree_is_adopted_with_its_branch_for_approve() {
    let f = Fixture::new("worktree");
    let app = f.root.join("app");
    repo(&app);
    let tree = f.root.join(".worktrees/app-fix");
    git(
        &app,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "fix",
            &tree.to_string_lossy(),
        ],
    );
    let p = f.host(None).prepare(&existing(&tree)).await.unwrap();
    assert_eq!(p.places[0].kind, PlaceKind::Branch);
    assert_eq!(p.places[0].branch.as_deref(), Some("fix"));
    assert_eq!(
        (p.repo.as_deref(), p.branch.as_deref()),
        (Some(app.as_path()), Some("fix"))
    );
    let exclude = std::fs::read_to_string(app.join(".git/info/exclude")).unwrap();
    assert!(exclude.lines().any(|l| l == ".muxa/"), "{exclude}");
    let branches = std::process::Command::new("git")
        .arg("-C")
        .arg(&app)
        .args(["branch", "--list", "fix"])
        .output()
        .unwrap();
    assert!(!branches.stdout.is_empty(), "the branch stays");
}

#[tokio::test]
async fn a_path_outside_and_a_busy_folder_are_refused_before_anything_is_written() {
    let f = Fixture::new("refused");
    let outside = f.base.join("elsewhere");
    std::fs::create_dir_all(&outside).unwrap();
    let refused = f.host(None).prepare(&existing(&outside)).await;
    assert!(
        matches!(&refused, Err(TaskError::Invalid(m)) if m.contains("outside Workspace w")),
        "{refused:?}"
    );
    let dir = f.root.join("tasks/fix-t-00000b");
    std::fs::create_dir_all(&dir).unwrap();
    let busy = f.host(Some(&dir)).prepare(&existing(&dir)).await;
    assert!(
        matches!(&busy, Err(TaskError::Busy(m)) if m.contains("claude (pid 4242) already works in")),
        "{busy:?}"
    );
    assert!(!dir.join(".muxa").exists(), "nothing written");
    let mut both = existing(&dir);
    both.places.push(PlaceSpec::Folder);
    assert!(matches!(
        f.host(None).prepare(&both).await,
        Err(TaskError::Invalid(_))
    ));
}

#[tokio::test]
async fn claude_continues_only_where_it_kept_a_conversation() {
    let f = Fixture::new("continue");
    let (old, new) = (
        f.root.join("tasks/a-t-00000c"),
        f.root.join("tasks/b-t-00000d"),
    );
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    f.conversation(&old);
    let host = f.host(None);
    assert_eq!(
        host.prepare(&existing(&old)).await.unwrap().argv[2],
        "claude --continue -- 'Read .muxa/TASK.md and continue the Task.'"
    );
    assert_eq!(
        host.prepare(&existing(&new)).await.unwrap().argv[2],
        "claude -- 'Read .muxa/TASK.md and continue the Task.'"
    );
    // A new folder Task never continues, also in a folder with a conversation.
    let mut fresh = existing(&old);
    fresh.places = Vec::new();
    assert_eq!(
        host.prepare(&fresh).await.unwrap().argv[2],
        "claude -- 'Read .muxa/TASK.md and do the Task.'"
    );
}

/// A client of the server on `host`, past its welcome.
async fn connect(host: BridgeHost, inbox: Arc<Inbox>, backend: FakeBackend) -> DuplexStream {
    let (client, server_end) = tokio::io::duplex(1 << 20);
    tokio::spawn(async move {
        let (r, w) = tokio::io::split(server_end);
        let config = Config {
            server: "muxa test".into(),
            backend: "fake".into(),
            park: std::time::Duration::from_secs(5),
        };
        let sessions = Sessions::new();
        let services = Services {
            backend: &backend,
            host: &host,
            sessions: &sessions,
            inbox: &inbox,
        };
        let _ = serve(&services, r, w, &config).await;
    });
    let mut client = client;
    let hello = json!({"type": "hello", "version": 2, "client": "test", "resume": null});
    send(&mut client, &hello).await;
    assert_eq!(read(&mut client).await["type"], "welcome");
    client
}

async fn send(client: &mut DuplexStream, msg: &Value) {
    let frame = Frame::new(0, Kind::Data, msg.to_string().into_bytes());
    client.write_all(&frame.encode().unwrap()).await.unwrap();
}

async fn read(client: &mut DuplexStream) -> Value {
    let mut buf = Vec::new();
    loop {
        if let Some(f) = Frame::decode(&mut buf).unwrap() {
            return serde_json::from_slice(&f.payload).unwrap();
        }
        let mut chunk = [0u8; 65536];
        let n = client.read(&mut chunk).await.unwrap();
        assert!(n > 0, "the server closed");
        buf.extend_from_slice(&chunk[..n]);
    }
}

async fn request(client: &mut DuplexStream, msg: Value) -> Value {
    send(client, &msg).await;
    read(client).await
}

#[tokio::test]
async fn migrate_adds_each_open_task_once_and_resume_adopts_its_place() {
    let f = Fixture::new("migrate");
    let state = f.base.join("state");
    let kept = f.root.join("tasks/fix-login-t-000001");
    std::fs::create_dir_all(&kept).unwrap();
    record(&state, "t-000001", "open", "Fix login", &kept);
    record(
        &state,
        "t-000002",
        "open",
        "Lost",
        &f.root.join("tasks/lost-t-000002"),
    );
    record(&state, "t-000003", "done", "Shipped", &kept);
    let files = |dir: &Path| -> Vec<(PathBuf, Vec<u8>)> {
        let mut out: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| {
                let p = e.unwrap().path();
                (p.clone(), std::fs::read(&p).unwrap())
            })
            .collect();
        out.sort();
        out
    };
    let before = files(&state);
    let (inbox, backend) = (Arc::new(Inbox::memory()), FakeBackend::new());
    let mut c = connect(f.host(None), Arc::clone(&inbox), backend.clone()).await;

    let dry = request(
        &mut c,
        json!({"type": "migrate-v1", "id": 1, "dry_run": true}),
    )
    .await;
    let path = kept.to_string_lossy();
    assert_eq!(
        dry,
        json!({"type": "reply", "id": 1, "added": 0, "tasks": [
            {"id": "t-000001", "workspace": "w", "title": "Fix login", "path": path,
             "exists": true, "status": "new"},
            {"id": "t-000002", "workspace": "w", "title": "Lost",
             "path": f.root.join("tasks/lost-t-000002").to_string_lossy(),
             "exists": false, "status": "missing"},
        ]})
    );
    assert!(inbox.view().items.is_empty(), "a dry run adds nothing");

    let run = request(&mut c, json!({"type": "migrate-v1", "id": 2})).await;
    assert_eq!(
        (run["added"].clone(), run["tasks"][0]["status"].clone()),
        (json!(1), json!("added"))
    );
    let items = inbox.view().items;
    assert_eq!(items.len(), 1);
    let item = serde_json::to_value(&items[0]).unwrap();
    assert_eq!(item["title"], "Resume: Fix login");
    assert_eq!(
        (item["kind"].clone(), item["workspace"].clone()),
        (json!("todo"), json!("w"))
    );
    assert_eq!(item["place"], json!({"workspace": "w", "path": path}));
    assert!(item["note"].as_str().unwrap().contains("old muxa"));

    // Also after the item is done, a second run adds nothing.
    inbox.change(|b, _| b.done(items[0].id.as_str())).unwrap();
    let again = request(&mut c, json!({"type": "migrate-v1", "id": 3})).await;
    assert_eq!(
        (again["added"].clone(), again["tasks"][0]["status"].clone()),
        (json!(0), json!("in-inbox"))
    );
    assert!(inbox.view().items.is_empty());
    assert_eq!(files(&state), before, "no v1 record changed");

    // Resume Task: the agent starts in the kept folder; a second one finds it.
    let start = json!({"type": "start-task", "id": 4, "workspace": "w", "title": "Fix login",
        "places": [{"kind": "existing", "path": path}]});
    let started = request(&mut c, start.clone()).await;
    assert_eq!(started["existing"], false, "{started}");
    assert_eq!(started["terminal"]["labels"]["muxa.dir"], *path);
    let mut again = start;
    again["id"] = json!(5);
    let found = request(&mut c, again).await;
    assert_eq!(found["existing"], true, "{found}");
    assert_eq!(backend.list().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_paused_task_outlives_its_daemon_and_resumes_its_conversation() {
    let f = Fixture::new("pause");
    let place = f.root.join("tasks/fix-login-t-00000e");
    std::fs::create_dir_all(&place).unwrap();
    f.conversation(&place);
    let (inbox, backend) = (Arc::new(Inbox::memory()), FakeBackend::new());
    let mut c = connect(f.host(None), Arc::clone(&inbox), backend.clone()).await;
    let started = request(
        &mut c,
        json!({"type": "start-task", "id": 1, "workspace": "w", "title": "Fix login",
               "places": [{"kind": "existing", "path": place.to_string_lossy()}]}),
    )
    .await;
    let agent = started["terminal"]["id"].as_str().unwrap().to_owned();
    let paused = request(
        &mut c,
        json!({"type": "pause-task", "id": 2, "terminal": agent}),
    )
    .await;
    assert_eq!(
        paused["paused"]["terminal"]["id"],
        agent.as_str(),
        "{paused}"
    );

    // A new daemon reads the same state dir.
    let mut c = connect(f.host(None), Arc::clone(&inbox), backend.clone()).await;
    let listed = request(&mut c, json!({"type": "paused-tasks", "id": 3})).await;
    assert_eq!(
        listed["paused"][0]["terminal"]["id"],
        agent.as_str(),
        "{listed}"
    );
    let resumed = request(
        &mut c,
        json!({"type": "resume-task", "id": 4, "terminal": agent}),
    )
    .await;
    assert_eq!(
        resumed["terminal"]["labels"]["muxa.title"], "Fix login",
        "{resumed}"
    );
    let launch = backend.launches().last().cloned().unwrap();
    assert_eq!(
        launch.argv[2],
        "claude --continue -- 'Read .muxa/TASK.md and continue the Task.'"
    );
    let listed = request(&mut c, json!({"type": "paused-tasks", "id": 5})).await;
    assert_eq!(listed["paused"], json!([]));
}

#[tokio::test]
async fn resume_is_refused_while_another_agent_works_in_the_place() {
    let f = Fixture::new("pause-busy");
    let place = f.root.join("tasks/fix-login-t-00000f");
    std::fs::create_dir_all(&place).unwrap();
    let (inbox, backend) = (Arc::new(Inbox::memory()), FakeBackend::new());
    let mut c = connect(f.host(None), Arc::clone(&inbox), backend.clone()).await;
    let started = request(
        &mut c,
        json!({"type": "start-task", "id": 1, "workspace": "w", "title": "Fix login",
               "places": [{"kind": "existing", "path": place.to_string_lossy()}]}),
    )
    .await;
    let agent = started["terminal"]["id"].as_str().unwrap().to_owned();
    request(
        &mut c,
        json!({"type": "pause-task", "id": 2, "terminal": agent}),
    )
    .await;

    let mut c = connect(f.host(Some(&place)), Arc::clone(&inbox), backend.clone()).await;
    let busy = request(
        &mut c,
        json!({"type": "resume-task", "id": 3, "terminal": agent}),
    )
    .await;
    assert_eq!(busy["code"], "conflict", "{busy}");
    let listed = request(&mut c, json!({"type": "paused-tasks", "id": 4})).await;
    assert_eq!(
        listed["paused"][0]["terminal"]["id"],
        agent.as_str(),
        "it stays paused"
    );
}
