use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::records::Workspaces;
use crate::records::workers::LaunchCommand;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-subs-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("company/.muxa")).unwrap();
    dir.canonicalize().unwrap()
}

const SUBS: &str = "[[subscription]]\nname = 'personal'\nagent = 'claude-code'\nhome = '~/.claude'\n\
     [[subscription]]\nname = 'work'\nagent = 'claude-code'\nhome = '~/.claude-work'\nplan = 'Max 20x'\n\
     [[subscription]]\nname = 'spare'\nagent = 'claude-code'\nhome = '~/.claude-spare'\n\
     [[subscription]]\nname = 'chatgpt'\nagent = 'codex'\nhome = '~/.codex-chatgpt'\n";

/// The host file with `SUBS`, one Workspace `Company` with `extra`, and
/// `routines` as its routines file.
fn load(home: &Path, extra: &str, routines: &str, tail: &str) -> Result<Workspaces, String> {
    fs::write(home.join("company/.muxa/routines.toml"), routines).unwrap();
    Workspaces::parse(
        &format!(
            "{SUBS}[[workspace]]\nname = 'Company'\nroot = '{}/company'\n{extra}\
             [workspace.routines]\nenabled = true\n{tail}",
            home.display()
        ),
        Path::new("workspaces.toml"),
        Some(home),
    )
    .map_err(|e| e.to_string())
}

fn shell(line: &str) -> LaunchCommand {
    LaunchCommand::Shell(line.to_owned())
}

#[test]
fn subscriptions_load_and_the_default_login_sets_no_variable() {
    let home = temp("load");
    let all = load(&home, "subscriptions = ['work', 'chatgpt']\n", "", "").unwrap();
    let subs = all.subscriptions();
    assert_eq!(subs.len(), 4);
    let personal = &subs[0];
    assert!(personal.default_login);
    assert_eq!(personal.env(), None);
    let work = &subs[1];
    assert_eq!(work.plan.as_deref(), Some("Max 20x"));
    assert_eq!(
        work.env(),
        Some((
            "CLAUDE_CONFIG_DIR".into(),
            home.join(".claude-work").to_string_lossy().into_owned()
        ))
    );
    assert_eq!(subs[3].env().unwrap().0, "CODEX_HOME");
    assert_eq!(
        all.get("Company").unwrap().subscriptions,
        vec!["work".to_owned(), "chatgpt".to_owned()]
    );
    // The adapters search every login folder that a variable names.
    let dirs = all.agent_dirs("CLAUDE_CONFIG_DIR");
    assert_eq!(
        dirs,
        vec![home.join(".claude-work"), home.join(".claude-spare")]
    );
    assert_eq!(
        all.agent_dirs("CODEX_HOME"),
        vec![home.join(".codex-chatgpt")]
    );
}

#[test]
fn the_loader_refuses_what_would_pick_the_wrong_login() {
    let home = temp("refuse");
    for (extra, why) in [
        ("subscriptions = ['nope']\n", "is not a [[subscription]]"),
        ("subscriptions = ['work', 'work']\n", "appears twice"),
        (
            "subscriptions = ['work']\n[workspace.agent]\nenv = { CLAUDE_CONFIG_DIR = '~/.other' }\n",
            "env CLAUDE_CONFIG_DIR and subscriptions both pick a login",
        ),
    ] {
        let err = load(&home, extra, "", "").unwrap_err();
        assert!(err.contains(why), "{extra}: {err}");
    }
    let bad = |sub: &str| {
        Workspaces::parse(sub, Path::new("workspaces.toml"), Some(&home))
            .unwrap_err()
            .to_string()
    };
    assert!(
        bad("[[subscription]]\nname = 'a'\nagent = 'gemini'\nhome = '~/.g'\n")
            .contains("agent must be")
    );
    assert!(
        bad("[[subscription]]\nname = 'A b'\nagent = 'codex'\nhome = '~/.c'\n")
            .contains("subscription name")
    );
    assert!(
        bad("[[subscription]]\nname = 'a'\nagent = 'codex'\nhome = 'rel'\n")
            .contains("home must be absolute")
    );
    assert!(
        bad(
            "[[subscription]]\nname = 'a'\nagent = 'codex'\nhome = '~/.c'\n\
             [[subscription]]\nname = 'b'\nagent = 'codex'\nhome = '~/.c'\n"
        )
        .contains("folder of another subscription")
    );
}

#[test]
fn a_bot_keeps_only_its_workspaces_logins_and_a_stale_name_never_blocks() {
    let home = temp("bot");
    let all = load(
        &home,
        "subscriptions = ['work', 'spare']\n",
        "[[bot]]\nname = 'triage'\nenabled = true\nsubscriptions = ['spare', 'personal', 'gone', 'spare']\n",
        "",
    )
    .unwrap();
    // `personal` is a host login outside Company's list; `gone` is unknown.
    assert_eq!(
        all.bot("Company", "triage").unwrap().subscriptions,
        vec!["spare".to_owned()]
    );
}

#[test]
fn the_assistant_takes_any_claude_login() {
    let home = temp("assistant");
    let all = load(
        &home,
        "",
        "",
        "[assistant]\nenabled = true\nsubscriptions = ['chatgpt', 'personal', 'gone']\n",
    )
    .unwrap();
    assert_eq!(all.assistant().subscriptions, vec!["personal".to_owned()]);
}

#[test]
fn an_agent_takes_the_first_login_of_its_kind_and_a_restart_keeps_its_own() {
    let home = temp("choose");
    let all = load(&home, "", "", "").unwrap();
    let subs = all.subscriptions();
    let names: Vec<String> = ["chatgpt", "work", "spare"].map(String::from).to_vec();
    let name = |line: &str, keep: Option<&str>| {
        choose(subs, &names, &shell(line), keep, &Snapshot::default()).map(|s| s.name.as_str())
    };
    assert_eq!(name("claude --model opus", None), Some("work"));
    assert_eq!(name("FOO=1 codex", None), Some("chatgpt"));
    assert_eq!(name("aider", None), None);
    assert_eq!(name("claude", Some("spare")), Some("spare"));
    // A kept login of another kind, or one that is gone, falls back to the order.
    assert_eq!(name("claude", Some("chatgpt")), Some("work"));
    assert_eq!(name("claude", Some("gone")), Some("work"));
    // No list: no login, so the agent starts as before.
    assert_eq!(
        choose(subs, &[], &shell("claude"), None, &Snapshot::default()),
        None
    );
}

#[test]
fn an_adopted_place_resumes_in_the_login_that_holds_its_conversation() {
    let home = temp("holding");
    let all = load(&home, "", "", "").unwrap();
    let place = home.join("company/notes");
    fs::create_dir_all(&place).unwrap();
    let project: String = place
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let names: Vec<String> = ["work", "spare"].map(String::from).to_vec();
    assert_eq!(holding(all.subscriptions(), &names, &place), None);
    let conversations = home.join(".claude-spare/projects").join(project);
    fs::create_dir_all(&conversations).unwrap();
    fs::write(conversations.join("c.jsonl"), "{}\n").unwrap();
    assert_eq!(
        holding(all.subscriptions(), &names, &place).as_deref(),
        Some("spare")
    );
}

/// ADR 0091 §9: a full login is skipped; when every login of the kind is
/// full, new work still gets the first, and unattended work waits for the
/// earliest reset.
#[test]
fn a_full_login_is_skipped_and_all_full_means_wait() {
    let home = temp("full");
    let all = load(&home, "", "", "").unwrap();
    let subs = all.subscriptions();
    let names: Vec<String> = ["chatgpt", "work", "spare"].map(String::from).to_vec();
    let now = 1_800_000_000;
    let full = |resets_at: u64| {
        crate::limits::merge(
            &Default::default(),
            &[crate::limits::Window {
                minutes: 300,
                used_percent: 97.0,
                resets_at,
            }],
            now,
        )
    };
    let snapshot = |records: Vec<(&str, u64)>| {
        crate::thresholds::Snapshot::new(
            records
                .into_iter()
                .map(|(n, r)| (n.to_owned(), full(r)))
                .collect(),
            now,
        )
    };
    let claude = WorkerAgent::ClaudeCode;
    let work_full = snapshot(vec![("work", now + 900)]);
    assert_eq!(
        pick(subs, &names, claude, &work_full).unwrap().name,
        "spare"
    );
    assert_eq!(wait_until(subs, &names, claude, &work_full), None);
    let both = snapshot(vec![("work", now + 900), ("spare", now + 300)]);
    assert_eq!(pick(subs, &names, claude, &both).unwrap().name, "work");
    assert_eq!(wait_until(subs, &names, claude, &both), Some(now + 300));
    // Codex has its own room; no list, nothing to wait for.
    assert_eq!(wait_until(subs, &names, WorkerAgent::Codex, &both), None);
    assert_eq!(wait_until(subs, &[], claude, &both), None);
    // A restart keeps its login, full or not.
    let kept = choose(subs, &names, &shell("claude"), Some("work"), &both).unwrap();
    assert_eq!(kept.name, "work");
}

/// ADR 0091 §4: with switching off, a Workspace keeps the first login of
/// each kind, also when it is full; moving another to the top switches.
#[test]
fn with_switching_off_the_first_login_of_its_kind_stays_until_the_user_moves_one_up() {
    let home = temp("noswitch");
    let all = load(
        &home,
        "subscriptions = ['work', 'chatgpt', 'spare']\nswitch_subscriptions = false\n",
        "",
        "",
    )
    .unwrap();
    let ws = all.get("Company").unwrap();
    assert!(!ws.switch_subscriptions);
    let subs = all.subscriptions();
    assert_eq!(
        order(subs, &ws.subscriptions, ws.switch_subscriptions),
        ["work", "chatgpt"]
    );
    let now = 1_800_000_000;
    let work_full = crate::thresholds::Snapshot::new(
        [(
            "work".to_owned(),
            crate::limits::merge(
                &Default::default(),
                &[crate::limits::Window {
                    minutes: 300,
                    used_percent: 97.0,
                    resets_at: now + 900,
                }],
                now,
            ),
        )]
        .into(),
        now,
    );
    let names = order(subs, &ws.subscriptions, false);
    let claude = WorkerAgent::ClaudeCode;
    assert_eq!(pick(subs, &names, claude, &work_full).unwrap().name, "work");
    assert_eq!(
        wait_until(subs, &names, claude, &work_full),
        Some(now + 900)
    );
    // The user puts spare first: the next agent takes it.
    let moved: Vec<String> = ["spare", "work", "chatgpt"].map(String::from).to_vec();
    let names = order(subs, &moved, false);
    assert_eq!(
        pick(subs, &names, claude, &work_full).unwrap().name,
        "spare"
    );
    // On by default.
    let on = load(&home, "subscriptions = ['work']\n", "", "").unwrap();
    assert!(on.get("Company").unwrap().switch_subscriptions);
}
