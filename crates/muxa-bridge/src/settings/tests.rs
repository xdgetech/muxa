use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

use muxa_protocol::settings::{McpSpec, RoutineSpec, WorkerLoop, WorkerSpec, WriteOnly};
use muxa_protocol::v2::LoopChecks;
use serde_json::json;

use super::secrets::Fake;
use super::*;

struct Shared(Arc<Fake>);

impl Secrets for Shared {
    fn state(&self, account: &str) -> SecretState {
        self.0.state(account)
    }
    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        self.0.set(account, secret)
    }
    fn delete(&self, account: &str) -> Result<(), String> {
        self.0.delete(account)
    }
}

struct Host {
    dir: PathBuf,
    file: PathBuf,
    secrets: Arc<Fake>,
    store: SettingsStore,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

const FILE: &str = r#"# My workspaces.
[[workspace]]
name = "Work"
root = "ROOT/work"
park_after = "12h"

[workspace.agent]
command = "claude --model x"
env = { CLAUDE_CONFIG_DIR = "~/.claude-work", API_TOKEN = "t0ken-value" }

[workspace.routines]
enabled = true

# The Linear token.
[[workspace.connection]]
name = "linear"
env = "LINEAR_API_KEY"
access = "read"

[[workspace]]
name = "Home"
root = "ROOT/home"
"#;

const ROUTINES: &str = r#"# Daily.
[[routine]]
name = "digest"
at = "08:00"
timezone = "Europe/Berlin"
instructions = "routines/digest.md"
connections = ["linear"]
enabled = true
"#;

fn host(name: &str) -> Host {
    host_with(name, FILE, false, None)
}

fn host_with(name: &str, text: &str, read_only: bool, refuse: Option<&str>) -> Host {
    let dir = std::env::temp_dir().join(format!("muxa-settings-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("work/.muxa/routines")).unwrap();
    fs::create_dir_all(dir.join("home")).unwrap();
    let dir = dir.canonicalize().unwrap();
    fs::write(dir.join("work/.muxa/routines.toml"), ROUTINES).unwrap();
    fs::write(
        dir.join("work/.muxa/routines/digest.md"),
        "Sum up the day.\n",
    )
    .unwrap();
    let file = dir.join("workspaces.toml");
    fs::write(&file, text.replace("ROOT", &dir.to_string_lossy())).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let secrets = Arc::new(Fake {
        refuse: refuse.map(str::to_owned),
        ..Fake::default()
    });
    secrets
        .items
        .lock()
        .unwrap()
        .insert("Work/linear".into(), "old-secret".into());
    let store = SettingsStore::new(
        Some(file.clone()),
        Some(dir.clone()),
        read_only,
        Box::new(Shared(Arc::clone(&secrets))),
    );
    Host {
        dir,
        file,
        secrets,
        store,
    }
}

fn text(h: &Host) -> String {
    fs::read_to_string(&h.file).unwrap()
}

fn op(value: serde_json::Value) -> SettingsOp {
    serde_json::from_value(value).unwrap()
}

#[test]
fn the_view_shows_the_files_as_written_and_no_value() {
    let h = host("view");
    let s = h.store.view().unwrap();
    assert_eq!(s.problem, None);
    let work = &s.workspaces[0];
    assert_eq!(work.park_after.as_deref(), Some("12h"));
    assert_eq!(work.agent.command.as_deref(), Some("claude --model x"));
    let keys: Vec<&str> = work.agent.env.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(keys, ["API_TOKEN", "CLAUDE_CONFIG_DIR"]);
    assert!(work.agent.env.iter().all(|e| e.set));
    assert_eq!(work.connections[0].secret, SecretState::Set);
    assert!(work.routines_enabled && !work.direct_writes);
    assert_eq!(work.routines[0].spec.name, "digest");
    assert_eq!(
        work.routines[0].instructions_text.as_deref(),
        Some("Sum up the day.\n")
    );
    assert_eq!(work.routines_problem, None);
    let wire = serde_json::to_string(&s).unwrap();
    assert!(
        !wire.contains("t0ken-value") && !wire.contains("old-secret"),
        "{wire}"
    );
    assert_eq!(s.revision, h.store.view().unwrap().revision);
}

#[test]
fn an_update_edits_in_place_keeps_comments_and_a_backup() {
    let h = host("edit");
    let rev = h.store.view().unwrap().revision;
    let s = h
        .store
        .update(
            &rev,
            vec![
                op(json!({"op": "env-set", "workspace": "Work", "key": "NEW_KEY", "value": "n3w-value"})),
                op(json!({"op": "workspace-update", "workspace": "Work", "agent_command": null, "park_after": "2d"})),
                op(json!({"op": "loop-checks", "workspace": "Home", "checks": ["make test"]})),
            ],
        )
        .unwrap();
    assert_ne!(s.revision, rev);
    let after = text(&h);
    assert!(after.starts_with("# My workspaces.\n"), "{after}");
    assert!(after.contains("# The Linear token."), "{after}");
    assert!(after.contains("NEW_KEY = \"n3w-value\""), "{after}");
    assert!(
        after.contains("park_after = \"2d\"") && !after.contains("--model"),
        "{after}"
    );
    let wire = serde_json::to_string(&s).unwrap();
    assert!(!wire.contains("n3w-value"), "{wire}");
    assert_eq!(
        s.workspaces[1].task_loop.as_ref().map(|l| l.checks.clone()),
        Some(LoopChecks::List(vec!["make test".into()]))
    );
    let bak = h.dir.join("workspaces.toml.bak");
    assert_eq!(
        fs::read_to_string(&bak).unwrap(),
        FILE.replace("ROOT", &h.dir.to_string_lossy())
    );
    let mode = fs::metadata(&h.file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert!(Workspaces::load(&h.file, Some(&h.dir)).is_ok());
}

#[test]
fn found_repos_show_and_a_hidden_one_is_saved() {
    let h = host("hidden");
    for repo in ["app", "group/lib"] {
        fs::create_dir_all(h.dir.join("home").join(repo).join(".git")).unwrap();
    }
    let s = h.store.view().unwrap();
    assert_eq!(s.workspaces[1].found_repos, ["app", "group/lib"]);
    assert!(s.workspaces[1].hidden_repos.is_empty());
    let s = h
        .store
        .update(
            &s.revision,
            vec![op(json!({"op": "workspace-update", "workspace": "Home", "hidden_repos": ["group/lib"]}))],
        )
        .unwrap();
    assert_eq!(s.workspaces[1].hidden_repos, ["group/lib"]);
    assert_eq!(
        s.workspaces[1].found_repos,
        ["app", "group/lib"],
        "a hidden repo is still found"
    );
    let loaded = Workspaces::load(&h.file, Some(&h.dir)).unwrap();
    let home = loaded.all().iter().find(|w| w.name == "Home").unwrap();
    assert_eq!(home.repo_list(), [h.dir.join("home/app")]);
}

#[test]
fn a_connection_lands_in_its_own_workspace_and_its_secret_in_the_keychain() {
    let h = host("conn");
    let rev = h.store.view().unwrap().revision;
    let s = h
        .store
        .update(
            &rev,
            vec![op(json!({"op": "connection-add", "workspace": "Work",
                "connection": {"name": "github", "env": "GH_TOKEN", "default": true},
                "secret": "ghp_s3cret"}))],
        )
        .unwrap();
    let after = text(&h);
    assert!(!after.contains("ghp_s3cret"), "{after}");
    let github = after.find("name = \"github\"").unwrap();
    assert!(github < after.find("name = \"Home\"").unwrap(), "{after}");
    let all = Workspaces::load(&h.file, Some(&h.dir)).unwrap();
    assert_eq!(all.get("Work").unwrap().connections.len(), 2);
    assert!(all.get("Home").unwrap().connections.is_empty());
    assert_eq!(
        h.secrets
            .items
            .lock()
            .unwrap()
            .get("Work/github")
            .map(String::as_str),
        Some("ghp_s3cret")
    );
    assert_eq!(s.workspaces[0].connections[1].secret, SecretState::Set);
    assert!(!serde_json::to_string(&s).unwrap().contains("ghp_s3cret"));
}

#[test]
fn a_stale_revision_or_a_locked_host_changes_nothing() {
    let h = host("stale");
    let before = text(&h);
    let ops = || {
        vec![op(
            json!({"op": "env-unset", "workspace": "Work", "key": "API_TOKEN"}),
        )]
    };
    assert!(matches!(
        h.store.update("old", ops()),
        Err(SettingsError::Stale)
    ));
    let rev = h.store.view().unwrap().revision;
    fs::write(&h.file, format!("{before}\n# a hand edit\n")).unwrap();
    assert!(matches!(
        h.store.update(&rev, ops()),
        Err(SettingsError::Stale)
    ));

    let locked = host_with("locked", FILE, true, None);
    let rev = locked.store.view().unwrap().revision;
    assert!(locked.store.view().unwrap().read_only);
    assert!(matches!(
        locked.store.update(&rev, ops()),
        Err(SettingsError::ReadOnly)
    ));
}

#[test]
fn a_failed_op_changes_no_file_and_no_secret() {
    let h = host("atomic");
    let before = text(&h);
    let rev = h.store.view().unwrap().revision;
    let err = h
        .store
        .update(
            &rev,
            vec![
                op(json!({"op": "connection-secret", "workspace": "Work", "name": "linear", "secret": "n3w"})),
                op(json!({"op": "env-set", "workspace": "Work", "key": "TMUX", "value": "x"})),
            ],
        )
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("ops[1] env-set: "), "{err}");
    assert_eq!(text(&h), before);
    assert_eq!(
        h.secrets
            .items
            .lock()
            .unwrap()
            .get("Work/linear")
            .map(String::as_str),
        Some("old-secret")
    );
    let err = h
        .store
        .update(
            &rev,
            vec![op(
                json!({"op": "workspace-update", "workspace": "Home", "root": "/does/not/exist"}),
            )],
        )
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("ops[0] workspace-update: "), "{err}");
}

#[test]
fn a_keychain_failure_writes_no_file() {
    let h = host_with("refused", FILE, false, Some("Work/github"));
    let before = text(&h);
    let rev = h.store.view().unwrap().revision;
    let err = h.store.update(
        &rev,
        vec![op(json!({"op": "connection-add", "workspace": "Work",
            "connection": {"name": "github", "env": "GH_TOKEN"}, "secret": "x"}))],
    );
    assert!(matches!(err, Err(SettingsError::Failed(_))));
    assert_eq!(text(&h), before);
}

#[test]
fn a_routine_never_blocks_removing_a_connection_and_an_edit_drops_its_old_choice() {
    let h = host("uses");
    let rev = h.store.view().unwrap().revision;
    // `digest` still lists `connections = ["linear"]` from before ADR 0075.
    let remove = op(json!({"op": "connection-remove", "workspace": "Work", "name": "linear"}));
    let s = h.store.update(&rev, vec![remove]).unwrap();
    assert!(s.workspaces[0].connections.is_empty());
    assert!(s.workspaces[0].routines_problem.is_none());
    assert!(!h.secrets.items.lock().unwrap().contains_key("Work/linear"));
    let routine = RoutineSpec {
        name: "digest".into(),
        at: Some("09:00".into()),
        schedule: None,
        timezone: Some("Europe/Berlin".into()),
        worker: None,
        instructions: "routines/digest.md".into(),
        project: None,
        enabled: true,
        items: None,
        direct_writes: false,
        bot: None,
        read_only: false,
        on: None,
    };
    h.store
        .update(
            &s.revision,
            vec![SettingsOp::RoutineUpdate {
                workspace: "Work".into(),
                routine,
                instructions_text: None,
            }],
        )
        .unwrap();
    let routines = fs::read_to_string(h.dir.join("work/.muxa/routines.toml")).unwrap();
    assert!(
        routines.starts_with("# Daily.\n")
            && routines.contains("at = \"09:00\"")
            && !routines.contains("connections"),
        "{routines}"
    );
}

#[test]
fn a_new_routine_gets_its_file_and_its_instructions() {
    let h = host("routine");
    let rev = h.store.view().unwrap().revision;
    let spec = |instructions: &str| RoutineSpec {
        name: "weekly".into(),
        at: None,
        schedule: Some("0 9 * * 1".into()),
        timezone: Some("Asia/Tokyo".into()),
        worker: None,
        instructions: instructions.into(),
        project: None,
        enabled: false,
        items: Some(5),
        direct_writes: false,
        bot: None,
        read_only: false,
        on: None,
    };
    let add = |instructions: &str| SettingsOp::RoutineAdd {
        workspace: "Home".into(),
        routine: spec(instructions),
        instructions_text: Some("Plan the week.\n".into()),
    };
    let err = h
        .store
        .update(&rev, vec![add("../escape.md")])
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("ops[0] routine-add: "), "{err}");
    assert!(!h.dir.join("home/escape.md").exists());
    let s = h.store.update(&rev, vec![add("weekly.md")]).unwrap();
    let home = &s.workspaces[1];
    assert_eq!(home.routines[0].spec.items, Some(5));
    assert_eq!(
        home.routines[0].instructions_text.as_deref(),
        Some("Plan the week.\n")
    );
    assert_eq!(
        fs::read_to_string(h.dir.join("home/.muxa/weekly.md")).unwrap(),
        "Plan the week.\n"
    );
}

#[test]
fn mcp_servers_workers_and_workspaces_come_and_go() {
    let h = host("more");
    let rev = h.store.view().unwrap().revision;
    let mcp = McpSpec {
        name: "linear".into(),
        command: Some("npx".into()),
        args: Some(vec!["-y".into(), "linear-mcp".into()]),
        url: None,
        kind: None,
        env: Default::default(),
        headers: Default::default(),
        connections: vec!["linear".into()],
    };
    let worker = WorkerSpec {
        name: "reviewer".into(),
        description: Some("Reviews".into()),
        agent: "claude-code".into(),
        args: Vec::new(),
        instructions: None,
        workspaces: Some(vec!["Home".into()]),
        task_loop: None,
    };
    let side = h.dir.join("side");
    fs::create_dir_all(&side).unwrap();
    let s = h
        .store
        .update(
            &rev,
            vec![
                SettingsOp::McpAdd {
                    workspace: "Work".into(),
                    mcp,
                },
                SettingsOp::WorkerAdd {
                    worker: worker.clone(),
                },
                op(json!({"op": "workspace-add", "name": "Side", "root": side})),
                op(json!({"op": "routines-gate", "workspace": "Side", "enabled": true})),
            ],
        )
        .unwrap();
    assert_eq!(s.workspaces[0].mcp[0].connections, ["linear"]);
    assert_eq!(s.workers, std::slice::from_ref(&worker));
    assert!(s.workspaces[2].routines_enabled);
    let rev = s.revision;
    let err = h
        .store
        .update(
            &rev,
            vec![op(json!({"op": "workspace-remove", "workspace": "Home"}))],
        )
        .unwrap_err()
        .to_string();
    assert!(err.contains("worker"), "a worker allows Home: {err}");
    let looped = WorkerSpec {
        task_loop: Some(WorkerLoop {
            check: "make test".into(),
            max_attempts: None,
            timeout: None,
        }),
        ..worker
    };
    let err = h
        .store
        .update(&rev, vec![SettingsOp::WorkerUpdate { worker: looped }])
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("ops[0] worker-update: "),
        "Home allows no checks: {err}"
    );
    let s = h
        .store
        .update(
            &rev,
            vec![
                op(json!({"op": "worker-remove", "name": "reviewer"})),
                op(json!({"op": "workspace-remove", "workspace": "Work"})),
            ],
        )
        .unwrap();
    let names: Vec<&str> = s.workspaces.iter().map(|w| w.name.as_str()).collect();
    assert_eq!(names, ["Home", "Side"]);
    assert!(
        h.secrets.items.lock().unwrap().is_empty(),
        "Work's secret went too"
    );
}

#[test]
fn a_secret_too_long_is_refused_without_quoting_it() {
    let h = host("long");
    let rev = h.store.view().unwrap().revision;
    let long = "k".repeat(secrets::MAX_SECRET + 1);
    let err = h
        .store
        .update(
            &rev,
            vec![SettingsOp::ConnectionSecret {
                workspace: "Work".into(),
                name: "linear".into(),
                secret: WriteOnly(long.clone()),
            }],
        )
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("ops[0] connection-secret: ") && !err.contains(&long),
        "{err}"
    );
}

#[test]
fn a_bot_is_added_with_its_soul_and_owns_a_routine() {
    let h = host("bot-add");
    let rev = h.store.view().unwrap().revision;
    let s = h
        .store
        .update(
            &rev,
            vec![
                op(json!({"op": "bot-add", "workspace": "Work",
                    "bot": {"name": "pr-watch", "description": "Watches PRs", "enabled": true},
                    "soul_text": "I watch the PRs that wait for me.\n"})),
                op(json!({"op": "routine-add", "workspace": "Work",
                    "routine": {"name": "scan", "instructions": "routines/scan.md", "enabled": false,
                        "bot": "pr-watch", "read_only": true, "schedule": "*/30 9-18 * * 1-5",
                        "timezone": "Europe/Berlin"},
                    "instructions_text": "List open PRs.\n"})),
            ],
        )
        .unwrap();
    let work = &s.workspaces[0];
    let bot = &work.bots[0];
    assert_eq!(bot.spec.name, "pr-watch");
    assert_eq!(
        bot.soul_text.as_deref(),
        Some("I watch the PRs that wait for me.\n")
    );
    assert_eq!(
        (bot.memory_text.as_deref(), bot.memory_versions),
        (Some(""), 0)
    );
    let scan = work
        .routines
        .iter()
        .find(|r| r.spec.name == "scan")
        .unwrap();
    assert_eq!(
        (scan.spec.bot.as_deref(), scan.spec.read_only),
        (Some("pr-watch"), true)
    );
    let soul = h.dir.join("work/.muxa/bots/pr-watch/soul.md");
    assert_eq!(
        fs::read_to_string(soul).unwrap(),
        "I watch the PRs that wait for me.\n"
    );
    let file = fs::read_to_string(h.dir.join("work/.muxa/routines.toml")).unwrap();
    assert!(
        file.contains("[[bot]]") && file.contains("read_only = true"),
        "{file}"
    );
}

#[test]
fn a_bot_with_routines_cannot_go_and_a_bad_name_is_refused() {
    let h = host("bot-remove");
    let rev = h.store.view().unwrap().revision;
    let s = h
        .store
        .update(
            &rev,
            vec![
                op(json!({"op": "bot-add", "workspace": "Work", "bot": {"name": "b", "enabled": true}})),
                op(json!({"op": "routine-add", "workspace": "Work",
                    "routine": {"name": "r", "instructions": "routines/digest.md", "enabled": true, "bot": "b"}})),
            ],
        )
        .unwrap();
    let before = text(&h);
    let gone = h.store.update(
        &s.revision,
        vec![op(
            json!({"op": "bot-remove", "workspace": "Work", "name": "b"}),
        )],
    );
    assert!(matches!(gone, Err(SettingsError::Invalid(_))), "{gone:?}");
    let bad = h.store.update(
        &s.revision,
        vec![op(json!({"op": "bot-add", "workspace": "Work",
            "bot": {"name": "../x", "enabled": true}, "soul_text": "x"}))],
    );
    assert!(matches!(bad, Err(SettingsError::Invalid(_))), "{bad:?}");
    assert_eq!(text(&h), before);
}

#[test]
fn a_kept_memory_comes_back_and_the_current_one_is_kept() {
    let h = host("bot-memory");
    let rev = h.store.view().unwrap().revision;
    let s = h
        .store
        .update(
            &rev,
            vec![op(json!({"op": "bot-add", "workspace": "Work", "bot": {"name": "b", "enabled": true}}))],
        )
        .unwrap();
    let all = crate::records::Workspaces::load(&h.file, Some(&h.dir.join("home"))).unwrap();
    let bot = all.bot("Work", "b").unwrap();
    bot.write_memory("first").unwrap();
    bot.write_memory("second").unwrap();
    let missing = h.store.update(
        &s.revision,
        vec![op(
            json!({"op": "bot-memory-restore", "workspace": "Work", "name": "b", "version": 2}),
        )],
    );
    assert!(
        matches!(missing, Err(SettingsError::Invalid(_))),
        "{missing:?}"
    );
    let s = h.store.view().unwrap();
    assert_eq!(s.workspaces[0].bots[0].memory_versions, 1);
    let history = &s.workspaces[0].bots[0].memory_history;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].version, 1);
    assert!(history[0].at > 1_700_000_000, "the saved time: {history:?}");
    let s = h
        .store
        .update(
            &s.revision,
            vec![op(
                json!({"op": "bot-memory-restore", "workspace": "Work", "name": "b", "version": 1}),
            )],
        )
        .unwrap();
    assert_eq!(
        s.workspaces[0].bots[0].memory_text.as_deref(),
        Some("first")
    );
    assert_eq!(bot.read_memory_version(1).unwrap(), "second");
}

#[path = "tests_bots.rs"]
mod bots;
