use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::workspaces::{ConfigError, PathError, slug};
use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-records-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

fn new(title: &str) -> NewRecord {
    NewRecord {
        workspace: "work".into(),
        title: title.into(),
        goal: "Ship the thing.".into(),
        ..NewRecord::default()
    }
}

fn store(name: &str) -> Store {
    Store::open(temp(name).join("state/muxa/tasks")).unwrap()
}

#[test]
fn ids_are_t_and_six_lowercase_hex_digits() {
    assert!(is_valid_id("t-7f3a2c"));
    for bad in [
        "",
        "t-",
        "t-7f3a2",
        "t-7f3a2c0",
        "t-7F3A2C",
        "x-7f3a2c",
        "t-7f3a2g",
        "t-../aa",
        "t-7f3a/c",
    ] {
        assert!(!is_valid_id(bad), "{bad:?}");
    }
    for _ in 0..100 {
        assert!(is_valid_id(&new_id().unwrap()));
    }
}

#[test]
fn default_dirs_follow_xdg_then_home() {
    let home = Path::new("/home/u");
    assert_eq!(
        Store::default_dir(Some("/state".as_ref()), Some(home)),
        Some(PathBuf::from("/state/muxa/tasks"))
    );
    assert_eq!(
        Store::default_dir(Some("relative".as_ref()), Some(home)),
        Some(PathBuf::from("/home/u/.local/state/muxa/tasks"))
    );
    assert_eq!(Store::default_dir(None, None), None);
    assert_eq!(
        Workspaces::default_path(None, Some(home)),
        Some(PathBuf::from("/home/u/.config/muxa/workspaces.toml"))
    );
}

#[test]
fn create_then_list_and_get() {
    let store = store("create");
    let mode = std::fs::metadata(store.dir()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);

    let a = store.create(new("First")).unwrap();
    let b = store
        .create(NewRecord {
            repo: Some("/src/app".into()),
            branch: Some("feat/x".into()),
            ..new("Second")
        })
        .unwrap();
    assert_eq!((a.rev, a.version, a.status), (1, 1, Status::Open));
    assert_ne!(a.id, b.id);

    let file = store.dir().join(format!("{}.json", a.id));
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(json["createdAt"], a.created_at);
    assert_eq!(json["status"], "open");
    assert!(json.get("repo").is_none(), "absent fields are omitted");
    let mode = std::fs::metadata(&file).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);

    let mut listed = store.list().unwrap();
    listed.sort_by(|x, y| x.title.cmp(&y.title));
    assert_eq!(listed, vec![a.clone(), b]);
    assert_eq!(store.get(&a.id).unwrap(), a);
    assert!(matches!(
        store.get("t-000000"),
        Err(StoreError::NotFound(_))
    ));
    assert!(matches!(store.get("../x"), Err(StoreError::InvalidId)));
}

#[test]
fn update_increments_rev_and_keeps_what_the_store_owns() {
    let store = store("update");
    let a = store.create(new("Task")).unwrap();
    let b = store
        .update(&a.id, 1, |r| {
            r.status = Status::Review;
            r.session = Some("$3".into());
            r.rev = 99;
            r.id = "t-ffffff".into();
            r.created_at = 0;
        })
        .unwrap();
    assert_eq!(b.rev, 2);
    assert_eq!(b.id, a.id);
    assert_eq!(b.created_at, a.created_at);
    assert!(b.updated_at >= a.updated_at);
    assert_eq!(b.status, Status::Review);
    assert_eq!(store.get(&a.id).unwrap(), b);
    assert!(!store.dir().join("t-ffffff.json").exists());
}

#[test]
fn a_stale_rev_is_a_conflict() {
    let store = store("conflict");
    let other = Store::open(store.dir()).unwrap();
    let a = store.create(new("Task")).unwrap();

    // Both writers saw rev 1; the second one loses.
    store.update(&a.id, 1, |r| r.status = Status::Done).unwrap();
    let err = other
        .update(&a.id, 1, |r| r.status = Status::Dropped)
        .unwrap_err();
    assert!(
        matches!(
            err,
            StoreError::Conflict {
                expected: 1,
                current: 2,
                ..
            }
        ),
        "{err}"
    );
    assert_eq!(store.get(&a.id).unwrap().status, Status::Done);
}

#[test]
fn concurrent_writers_lose_no_update() {
    let store = store("concurrent");
    let id = store.create(new("Task")).unwrap().id;
    let writers = 8;
    let threads: Vec<_> = (0..writers)
        .map(|n| {
            let store = store.clone();
            let id = id.clone();
            std::thread::spawn(move || {
                loop {
                    let rev = store.get(&id).unwrap().rev;
                    let result = store.update(&id, rev, |r| {
                        let notes = r.note.take().unwrap_or_default();
                        r.note = Some(format!("{notes}{n},"));
                    });
                    match result {
                        Ok(_) => return,
                        Err(StoreError::Conflict { .. }) => continue,
                        Err(e) => panic!("{e}"),
                    }
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let r = store.get(&id).unwrap();
    assert_eq!(r.rev, 1 + writers);
    let note = r.note.unwrap();
    for n in 0..writers {
        assert!(note.contains(&format!("{n},")), "{note}");
    }
    let leftovers: Vec<_> = std::fs::read_dir(store.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().starts_with('.'))
        .collect();
    // OS-lock inodes persist across owners; temporary record files do not.
    assert_eq!(
        leftovers,
        vec![std::ffi::OsString::from(format!(".{id}.lock"))]
    );
    let claim = store.claim(&id, r.rev);
    assert!(claim.is_ok(), "{:?}", claim.err());
}

#[test]
fn unknown_fields_survive_a_rewrite() {
    let store = store("unknown");
    let a = store.create(new("Task")).unwrap();
    let file = store.dir().join(format!("{}.json", a.id));
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    json["agentRuns"] = serde_json::json!([{"id": "r1"}]);
    std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();

    store.update(&a.id, 1, |r| r.status = Status::Done).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(json["agentRuns"][0]["id"], "r1");
    assert_eq!(json["status"], "done");
}

#[test]
fn unreadable_files_are_skipped() {
    let store = store("corrupt");
    let good = store.create(new("Good")).unwrap();
    let dir = store.dir();
    std::fs::write(dir.join("t-aaaaaa.json"), b"{\"title\": \"secret").unwrap();
    std::fs::write(dir.join("t-bbbbbb.json"), vec![b' '; MAX_FILE as usize + 1]).unwrap();
    // A file whose id does not match its name.
    let copy = std::fs::read(dir.join(format!("{}.json", good.id))).unwrap();
    std::fs::write(dir.join("t-cccccc.json"), copy).unwrap();
    let mut future = serde_json::to_value(&good).unwrap();
    future["version"] = 2.into();
    future["id"] = "t-dddddd".into();
    std::fs::write(dir.join("t-dddddd.json"), future.to_string()).unwrap();
    std::fs::write(dir.join("notes.txt"), b"x").unwrap();

    assert_eq!(store.list().unwrap(), vec![good]);
    let err = store.get("t-aaaaaa").unwrap_err();
    assert!(matches!(err, StoreError::Unreadable { .. }));
    assert!(!err.to_string().contains("secret"), "{err}");
    assert!(matches!(
        store.update("t-dddddd", 1, |_| {}),
        Err(StoreError::Unreadable { .. })
    ));
}

#[test]
fn caps_refuse_without_echoing_text() {
    let store = store("caps");
    let long = "x".repeat(MAX_TITLE + 1);
    let err = store.create(new(&long)).unwrap_err();
    assert!(matches!(err, StoreError::Invalid { field: "title", .. }));
    assert!(!err.to_string().contains("xxxx"));
    assert!(
        store.create(new(&"é".repeat(MAX_TITLE))).is_ok(),
        "chars, not bytes"
    );
    assert!(store.create(new("  ")).is_err());
    assert!(store.create(new("two\nlines")).is_err());

    let goal = NewRecord {
        goal: "g".repeat(MAX_GOAL + 1),
        ..new("Task")
    };
    let err = store.create(goal).unwrap_err();
    assert!(matches!(err, StoreError::Invalid { field: "goal", .. }));
    let relative = NewRecord {
        dir: Some("tasks/x".into()),
        ..new("Task")
    };
    assert!(matches!(
        store.create(relative),
        Err(StoreError::Invalid { field: "dir", .. })
    ));

    let a = store.create(new("Task")).unwrap();
    let big = store.update(&a.id, 1, |r| {
        r.extra
            .insert("blob".into(), "b".repeat(MAX_FILE as usize).into());
    });
    assert!(matches!(
        big,
        Err(StoreError::Invalid { field: "task", .. })
    ));
    assert_eq!(store.get(&a.id).unwrap().rev, 1);
}

#[test]
fn slugs() {
    assert_eq!(slug("Fix the login bug"), "fix-the-login-bug");
    assert_eq!(
        slug("  --Q3 plan: budget & hiring!! "),
        "q3-plan-budget-hiring"
    );
    assert_eq!(slug("../../etc/passwd"), "etc-passwd");
    assert_eq!(slug("請求書を整理する"), "task");
    assert_eq!(slug(""), "task");
    let long = slug(&"abc def ".repeat(20));
    assert!(long.len() <= 40, "{long}");
    assert!(!long.ends_with('-'));
    assert_eq!(slug(&format!("{}-b", "a".repeat(39))), "a".repeat(39));
}

fn config(dir: &Path, text: &str) -> Result<Workspaces, ConfigError> {
    let path = dir.join("workspaces.toml");
    std::fs::write(&path, text).unwrap();
    Workspaces::load(&path, Some(dir))
}

#[test]
fn the_example_config_loads() {
    let home = temp("example");
    for dir in [
        "work/company/api",
        "work/company/web",
        "projects/side/app",
        "src/shared-ui",
        "personal",
    ] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
    }
    std::fs::create_dir_all(home.join("work/company/.muxa")).unwrap();
    std::fs::write(
        home.join("work/company/.muxa/routines.toml"),
        include_str!("../../../../docs/examples/routines.toml"),
    )
    .unwrap();
    let text = include_str!("../../../../docs/examples/workspaces.toml");
    let loaded = config(&home, text).unwrap();
    let names: Vec<String> = loaded.all().iter().map(|w| w.name.clone()).collect();
    assert_eq!(names, ["Company", "Side project", "Personal"]);
    assert!(loaded.routine_problems().is_empty());
    assert!(loaded.routine("Company", "situation").is_some());
}

#[test]
fn memory_is_review_by_default_and_refuses_another_word() {
    use muxa_protocol::MemoryMode;
    let dir = temp("memory");
    let ws = |extra: &str| {
        config(
            &dir,
            &format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n{extra}",
                dir.display()
            ),
        )
    };
    let mode = |extra: &str| ws(extra).unwrap().all()[0].memory;
    assert_eq!(mode(""), MemoryMode::Auto);
    assert_eq!(mode("memory = \"review\""), MemoryMode::Review);
    assert_eq!(mode("memory = \"auto\""), MemoryMode::Auto);
    assert_eq!(mode("memory = \"off\""), MemoryMode::Off);
    let e = ws("memory = \"always\"").unwrap_err().to_string();
    assert!(
        e.contains("memory must be \"review\", \"auto\" or \"off\""),
        "{e}"
    );
}

#[test]
fn workspaces_load_and_validate() {
    let dir = temp("config");
    std::fs::create_dir_all(dir.join("work/app")).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();

    let none = Workspaces::load(&dir.join("missing.toml"), None).unwrap();
    assert!(none.all().is_empty());

    let ws = config(
        &dir,
        &format!(
            "[[workspace]]\nname = \"work\"\nroot = \"~/work\"\nrepos = [\"{}/work/app\"]\n\n\
             [[workspace]]\nname = \"home\"\nroot = \"{}/home/.\"\n",
            dir.display(),
            dir.display()
        ),
    )
    .unwrap();
    let names: Vec<_> = ws.all().iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["work", "home"]);
    let work = ws.get("work").unwrap();
    assert_eq!(work.root, dir.join("work"));
    assert_eq!(work.repos, [dir.join("work/app")]);
    assert_eq!(ws.get("home").unwrap().root, dir.join("home"));
    assert!(ws.get("nope").is_none());

    let twice = "[[workspace]]\nname = \"a\"\nroot = \"~/work\"\n\
                 [[workspace]]\nname = \"a\"\nroot = \"~/home\"\n";
    assert!(matches!(
        config(&dir, twice),
        Err(ConfigError::Invalid { .. })
    ));
    let relative = "[[workspace]]\nname = \"a\"\nroot = \"work\"\n";
    assert!(matches!(
        config(&dir, relative),
        Err(ConfigError::Invalid { .. })
    ));
    let missing = "[[workspace]]\nname = \"a\"\nroot = \"~/nowhere\"\n";
    assert!(matches!(
        config(&dir, missing),
        Err(ConfigError::Invalid { .. })
    ));
    let typo = "[[workspace]]\nname = \"a\"\nroot = \"~/work\"\nrepo = []\n";
    assert!(matches!(config(&dir, typo), Err(ConfigError::Parse { .. })));
    assert!(config(&dir, "").unwrap().all().is_empty());
}

fn agent_config(dir: &Path, table: &str) -> Result<Workspaces, ConfigError> {
    std::fs::create_dir_all(dir.join("work")).unwrap();
    config(
        dir,
        &format!("[[workspace]]\nname = \"a\"\nroot = \"~/work\"\n\n[workspace.agent]\n{table}"),
    )
}

#[test]
fn an_agent_table_sets_command_and_env() {
    let dir = temp("agent");
    let ws = agent_config(
        &dir,
        "command = \"claude --model x\"\n\
         env = { CLAUDE_CONFIG_DIR = \"~/.claude-a\", CODEX_HOME = \"/c/h\", GH_TOKEN = \"t ~/x $y\" }\n",
    )
    .unwrap();
    let agent = ws.get("a").unwrap().agent.clone().unwrap();
    assert_eq!(agent.command.as_deref(), Some("claude --model x"));
    let claude_dir = dir.join(".claude-a");
    assert_eq!(
        agent.env("CLAUDE_CONFIG_DIR"),
        Some(claude_dir.to_str().unwrap()),
        "a leading ~/ is the home directory"
    );
    assert_eq!(agent.env("CODEX_HOME"), Some("/c/h"));
    assert_eq!(
        agent.env("GH_TOKEN"),
        Some("t ~/x $y"),
        "only a leading ~/ expands"
    );
    assert_eq!(
        agent.keys(),
        ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "GH_TOKEN"]
    );
    let shown = format!("{agent:?}");
    assert!(!shown.contains("t ~/x"), "Debug shows no value: {shown}");

    let empty = agent_config(&dir, "").unwrap();
    assert_eq!(empty.get("a").unwrap().agent, None);
    let only_command = agent_config(&dir, "command = \"codex\"\n").unwrap();
    let agent = only_command.get("a").unwrap().agent.clone().unwrap();
    assert!(agent.env.is_empty());
}

#[test]
fn a_bad_agent_table_is_refused_without_its_values() {
    let dir = temp("agent-bad");
    for (table, says) in [
        ("env = { \"1X\" = \"secret\" }\n", "not a variable name"),
        ("env = { \"A-B\" = \"secret\" }\n", "not a variable name"),
        ("env = { TMUX_PANE = \"secret\" }\n", "set by tmux"),
        ("env = { TMUX = \"secret\" }\n", "set by tmux"),
        ("env = { CLAUDE_CONFIG_DIR = \"rel/secret\" }\n", "absolute"),
        ("env = { CODEX_HOME = \"~user/secret\" }\n", "absolute"),
        ("env = { K = \"secret\\nline\" }\n", "control character"),
        ("command = \" \"\n", "empty"),
    ] {
        let err = agent_config(&dir, table).unwrap_err().to_string();
        assert!(err.contains(says), "{table}: {err}");
        assert!(!err.contains("secret"), "the value leaked: {err}");
    }
    let long = format!("env = {{ K = \"{}\" }}\n", "s".repeat(4097));
    let err = agent_config(&dir, &long).unwrap_err().to_string();
    assert!(err.contains("longer"), "{err}");
    let typo = agent_config(&dir, "commnd = \"x\"\n");
    assert!(matches!(typo, Err(ConfigError::Parse { .. })));
}

#[test]
fn a_repo_belongs_to_the_workspaces_that_list_it() {
    let dir = temp("listing");
    for d in ["a/app", "b", "shared"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    let ws = config(
        &dir,
        "[[workspace]]\nname = \"a\"\nroot = \"~/a\"\nrepos = [\"~/a/app\", \"~/shared\"]\n\
         [[workspace]]\nname = \"b\"\nroot = \"~/b\"\nrepos = [\"~/shared\"]\n",
    )
    .unwrap();
    let names =
        |p: &Path| -> Vec<String> { ws.listing(p).iter().map(|w| w.name.clone()).collect() };
    assert_eq!(names(&dir.join("a/app")), ["a"]);
    assert_eq!(names(&dir.join("shared")), ["a", "b"]);
    assert!(names(&dir.join("b")).is_empty());
}

#[test]
fn the_wire_workspace_lists_its_repos_and_omits_none() {
    let (dir, mut ws) = workspace("wire-repos");
    ws.repos = vec![dir.join("app")];
    let wire = crate::convert::workspace(&ws);
    assert_eq!(wire.repos, [dir.join("app").display().to_string()]);
    ws.repos.clear();
    let json = serde_json::to_value(crate::convert::workspace(&ws)).unwrap();
    assert!(json.get("repos").is_none(), "{json}");
    assert_eq!(json["memory"], "auto");
    assert!(json.get("memoryAt").is_none(), "no file: {json}");
    std::fs::create_dir_all(ws.root.join(".muxa")).unwrap();
    std::fs::write(ws.root.join(".muxa/WORKSPACE.md"), "# Learned\n").unwrap();
    let wire = crate::convert::workspace(&ws);
    assert!(wire.memory_at.is_some_and(|at| at > 0));
}

fn workspace(name: &str) -> (PathBuf, Workspace) {
    let dir = temp(name);
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let ws = Workspace {
        name: "work".into(),
        root: root.clone(),
        repos: Vec::new(),
        agent: None,
        loop_checks: None,
        park_after: None,
        routines: None,
        assistant: None,
        role: Default::default(),
        memory: muxa_protocol::MemoryMode::Auto,
    };
    (dir, ws)
}

#[test]
fn task_dir_is_slug_and_id_under_tasks() {
    let (_, ws) = workspace("taskdir");
    assert_eq!(
        ws.task_dir("Plan Q3 budget", "t-7f3a2c").unwrap(),
        ws.root.join("tasks/plan-q3-budget-t-7f3a2c")
    );
    assert!(matches!(
        ws.task_dir("x", "../../x"),
        Err(PathError::InvalidId)
    ));
}

#[test]
fn paths_that_leave_the_root_are_refused() {
    let (dir, ws) = workspace("escape");
    let outside = dir.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::create_dir_all(ws.root.join("docs")).unwrap();

    assert_eq!(
        ws.inside(&ws.root.join("docs/new/deeper")).unwrap(),
        ws.root.join("docs/new/deeper")
    );
    assert_eq!(ws.inside(&ws.root.join("docs/..")).unwrap(), ws.root);
    assert!(matches!(
        ws.inside(&ws.root.join("../outside")),
        Err(PathError::Outside { .. })
    ));
    assert!(matches!(
        ws.inside(&ws.root.join("missing/../../outside")),
        Err(PathError::Outside { .. })
    ));
    assert!(matches!(
        ws.inside(Path::new("docs")),
        Err(PathError::NotAbsolute(_))
    ));

    std::os::unix::fs::symlink(&outside, ws.root.join("link")).unwrap();
    assert!(matches!(
        ws.inside(&ws.root.join("link/new")),
        Err(PathError::Outside { .. })
    ));
    // tasks/ itself pointing out of the root makes every task dir refused.
    std::os::unix::fs::symlink(&outside, ws.root.join("tasks")).unwrap();
    assert!(matches!(
        ws.task_dir("Task", "t-000001"),
        Err(PathError::Outside { .. })
    ));
    // A dangling link would be followed when the directory is created.
    std::os::unix::fs::symlink(dir.join("gone"), ws.root.join("dangling")).unwrap();
    assert!(matches!(
        ws.inside(&ws.root.join("dangling/new")),
        Err(PathError::Outside { .. })
    ));
}

/// W0 counterexample: record CAS cannot undo external effects that happened first.
#[test]
fn record_lock_does_not_serialize_external_restart_effects() {
    use std::sync::{Arc, Barrier};
    let store = store("restart-external-effects");
    let record = store.create(new("Task")).unwrap();
    let barrier = Arc::new(Barrier::new(2));
    let effects = store.dir().join("fixture-effects");
    std::fs::create_dir(&effects).unwrap();
    let actors: Vec<_> = (0..2)
        .map(|actor| {
            let store = Store::open(store.dir()).unwrap();
            let id = record.id.clone();
            let barrier = Arc::clone(&barrier);
            let effect = effects.join(actor.to_string());
            std::thread::spawn(move || {
                let rev = store.get(&id).unwrap().rev;
                barrier.wait();
                // A stand-in for respawn, outside the record transaction.
                std::fs::write(effect, b"synthetic effect").unwrap();
                barrier.wait();
                store.update(&id, rev, |r| r.note = Some("fixture".into()))
            })
        })
        .collect();
    let results: Vec<_> = actors.into_iter().map(|a| a.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(StoreError::Conflict { .. })))
            .count(),
        1
    );
    assert_eq!(std::fs::read_dir(effects).unwrap().count(), 2);
    assert_eq!(store.get(&record.id).unwrap().rev, record.rev + 1);
}

#[test]
fn worker_fields_survive_legacy_record_and_run_rewrites() {
    let store = store("worker-fields");
    let record = store.create(new("Worker Task")).unwrap();
    let assignment =
        serde_json::json!({"kind":"future-assignment", "name":"Scheduler", "later":true});
    let saved = store
        .update(&record.id, record.rev, |r| {
            r.extra
                .insert("worker".into(), serde_json::json!("Reviewer"));
            r.start_run(RunReason::Start, r.created_at);
            r.runs[0]
                .extra
                .insert("assignment".into(), assignment.clone());
        })
        .unwrap();
    let rewritten = store
        .update(&record.id, saved.rev, |r| {
            r.status = Status::Review;
            r.runs[0].fill(&Seen {
                agent: "codex".into(),
                session: None,
                transcript: None,
            });
        })
        .unwrap();
    let loaded = store.get(&record.id).unwrap();
    assert_eq!(loaded, rewritten);
    assert_eq!(loaded.worker(), Some("Reviewer"));
    assert_eq!(loaded.runs[0].extra.get("assignment"), Some(&assignment));
    assert_eq!(
        loaded.runs[0].worker(),
        None,
        "an unknown kind is unrecorded"
    );
}
