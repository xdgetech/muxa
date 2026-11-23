use std::path::{Path, PathBuf};

use super::workspaces::{ConfigError, PathError, slug};
use super::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-records-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
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
fn a_file_from_before_adr_0066_still_loads() {
    // The assistant layer is gone; its keys are read and ignored, so the
    // daemon keeps starting with an older workspaces.toml.
    let dir = temp("old-assistant");
    let loaded = config(
        &dir,
        &format!(
            "[assistant]\nenabled = true\n\n[[workspace]]\nname = \"w\"\nroot = \"{}\"\n\
             memory = \"review\"\n\n[workspace.assistant]\noverall = true\nresident = true\n\
             staff = [\"a\"]\n",
            dir.display()
        ),
    )
    .unwrap();
    assert_eq!(loaded.all()[0].name, "w");
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

#[test]
fn a_missing_listed_repo_is_skipped_and_the_rest_still_loads() {
    let dir = temp("missing-repo");
    std::fs::create_dir_all(dir.join("work/app")).unwrap();
    std::fs::create_dir_all(dir.join("home")).unwrap();
    let ws = config(
        &dir,
        &format!(
            "[[workspace]]\nname = \"work\"\nroot = \"~/work\"\n\
             repos = [\"{d}/work/app\", \"~/renamed\"]\n\n\
             [[workspace]]\nname = \"home\"\nroot = \"~/home\"\n",
            d = dir.display()
        ),
    )
    .unwrap();
    let work = ws.get("work").unwrap();
    assert_eq!(work.repos, [dir.join("work/app")]);
    assert_eq!(work.missing_repos, [dir.join("renamed")]);
    assert!(ws.get("home").unwrap().missing_repos.is_empty());
    // A repo that is not absolute is still a mistake in the file.
    let relative = "[[workspace]]\nname = \"a\"\nroot = \"~/work\"\nrepos = [\"app\"]\n";
    assert!(matches!(
        config(&dir, relative),
        Err(ConfigError::Invalid { .. })
    ));
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

fn workspace(name: &str) -> (PathBuf, Workspace) {
    let dir = temp(name);
    let root = dir.join("root");
    std::fs::create_dir_all(&root).unwrap();
    let ws = Workspace {
        name: "work".into(),
        root: root.clone(),
        repos: Vec::new(),
        missing_repos: Vec::new(),
        hidden_repos: Vec::new(),
        agent: None,
        loop_checks: None,
        park_after: None,
        routines: None,
        connections: Vec::new(),
        mcp: Vec::new(),
        claude_connectors: false,
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
fn connections_load_and_a_routines_old_choice_is_ignored() {
    let dir = temp("connections");
    std::fs::create_dir_all(dir.join(".muxa")).unwrap();
    std::fs::write(dir.join(".muxa/brief.md"), "Read.\n").unwrap();
    let ws = |extra: &str, routine: &str| {
        std::fs::write(
            dir.join(".muxa/routines.toml"),
            format!(
                "[[routine]]\nname = \"r\"\ninstructions = \"brief.md\"\nenabled = true\n{routine}"
            ),
        )
        .unwrap();
        config(
            &dir,
            &format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.routines]\nenabled = true\n\
                 [workspace.agent]\nenv = {{ CLAUDE_CONFIG_DIR = \"/c\" }}\n{extra}",
                dir.display()
            ),
        )
    };
    let two = "[[workspace.connection]]\nname = \"linear\"\nenv = \"LINEAR_API_KEY\"\naccess = \"read\"\n\
               account = \"me@acme\"\n[[workspace.connection]]\nname = \"docs\"\nenv = \"DOCS\"\ndefault = true\n";
    let loaded = ws(two, "connections = [\"linear\"]\n").unwrap();
    let w = &loaded.all()[0];
    assert_eq!(w.connections.len(), 2);
    assert_eq!(w.connections[0].access, super::connections::Access::Read);
    assert_eq!(w.connections[1].name, "docs", "default is read and ignored");
    assert!(loaded.routine("w", "r").is_some());
    // An old choice of another connection neither fails nor hides the Routine (ADR 0075).
    let other = ws(two, "connections = [\"github\"]\n").unwrap();
    assert!(other.routine("w", "r").is_some());
    assert!(other.routine_problems().is_empty());
    for bad in [
        "[[workspace.connection]]\nname = \"Linear\"\nenv = \"X\"\n",
        "[[workspace.connection]]\nname = \"a\"\nenv = \"1X\"\n",
        "[[workspace.connection]]\nname = \"a\"\nenv = \"CLAUDE_CONFIG_DIR\"\n",
        "[[workspace.connection]]\nname = \"a\"\nenv = \"X\"\n[[workspace.connection]]\nname = \"b\"\nenv = \"X\"\n",
        "[[workspace.connection]]\nname = \"a\"\nenv = \"X\"\naccess = \"admin\"\n",
        "[[workspace.connection]]\nname = \"a\"\nenv = \"X\"\nsecret = \"t\"\n",
    ] {
        assert!(ws(bad, "").is_err(), "{bad}");
    }
}

#[test]
fn mcp_servers_load_and_a_routines_old_choice_is_ignored() {
    let dir = temp("mcp");
    std::fs::create_dir_all(dir.join(".muxa")).unwrap();
    std::fs::write(dir.join(".muxa/brief.md"), "Read.\n").unwrap();
    let ws = |extra: &str, routine: &str| {
        std::fs::write(
            dir.join(".muxa/routines.toml"),
            format!(
                "[[routine]]\nname = \"r\"\ninstructions = \"brief.md\"\nenabled = true\n{routine}"
            ),
        )
        .unwrap();
        config(
            &dir,
            &format!(
                "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n[workspace.routines]\nenabled = true\n\
                 [[workspace.connection]]\nname = \"linear\"\nenv = \"LINEAR_API_KEY\"\n{extra}",
                dir.display()
            ),
        )
    };
    let two = "[[workspace.mcp]]\nname = \"linear\"\ncommand = \"npx\"\nargs = [\"-y\", \"linear-mcp\"]\n\
               env = { LOG = \"info\" }\nconnections = [\"linear\"]\n\
               [[workspace.mcp]]\nname = \"docs\"\nurl = \"https://docs.example/mcp\"\ntype = \"sse\"\n\
               headers = { Authorization = \"Bearer ${LINEAR_API_KEY}\" }\nconnections = [\"linear\"]\ndefault = true\n";
    let loaded = ws(two, "mcp = [\"linear\"]\n").unwrap();
    let w = &loaded.all()[0];
    assert_eq!(w.mcp.len(), 2);
    assert_eq!(
        w.mcp[0].transport,
        super::mcp::Transport::Stdio {
            command: "npx".into(),
            args: vec!["-y".into(), "linear-mcp".into()]
        }
    );
    assert_eq!(w.mcp[0].connections, ["linear"]);
    assert_eq!(w.mcp[1].name, "docs", "default is read and ignored");
    assert!(matches!(
        w.mcp[1].transport,
        super::mcp::Transport::Sse { .. }
    ));
    assert!(loaded.routine("w", "r").is_some());
    // An old choice of another server neither fails nor hides the Routine (ADR 0075).
    let other = ws(two, "mcp = [\"github\"]\n").unwrap();
    assert!(other.routine("w", "r").is_some());
    assert!(other.routine_problems().is_empty());
    let unknown = ws(
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nconnections = [\"slack\"]\n",
        "",
    )
    .unwrap_err()
    .to_string();
    assert!(
        unknown.contains("connection \"slack\" is not a [[workspace.connection]]"),
        "{unknown}"
    );
    for bad in [
        "[[workspace.mcp]]\nname = \"A\"\ncommand = \"x\"\n",
        "[[workspace.mcp]]\nname = \"a\"\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nurl = \"https://x\"\n",
        "[[workspace.mcp]]\nname = \"a\"\nurl = \"ftp://x\"\n",
        "[[workspace.mcp]]\nname = \"a\"\nurl = \"https://x\"\ntype = \"ws\"\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\ntype = \"http\"\n",
        "[[workspace.mcp]]\nname = \"a\"\nurl = \"https://x\"\nargs = [\"y\"]\n",
        "[[workspace.mcp]]\nname = \"a\"\nurl = \"https://x\"\nenv = { A = \"1\" }\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nheaders = { A = \"1\" }\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nenv = { \"1A\" = \"1\" }\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nenv = { LINEAR_API_KEY = \"t\" }\nconnections = [\"linear\"]\n",
        "[[workspace.mcp]]\nname = \"a\"\nurl = \"https://x\"\nheaders = { A = \"${OTHER}\" }\nconnections = [\"linear\"]\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nconnections = [\"linear\", \"linear\"]\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\n[[workspace.mcp]]\nname = \"a\"\ncommand = \"y\"\n",
        "[[workspace.mcp]]\nname = \"a\"\ncommand = \"x\"\nsecret = \"t\"\n",
    ] {
        assert!(ws(bad, "").is_err(), "{bad}");
    }
}
