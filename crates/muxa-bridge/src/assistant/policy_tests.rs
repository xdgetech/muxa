use std::path::PathBuf;

use super::*;
use crate::assistant::record::RequestRun;

const ON: &str = "[assistant]\nenabled = true\nmax_runs_per_day = 3\n";

fn workspaces(name: &str, host: &str, ws: &str) -> (PathBuf, Workspaces) {
    let root = std::env::temp_dir().join(format!(
        "muxa-a-policy-{}-{name}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let text = format!(
        "{host}[[workspace]]\nname = \"w\"\nroot = \"{}\"\n{ws}\n\
         [[worker]]\nname = \"Cl\"\nagent = \"claude-code\"\n\
         [[worker]]\nname = \"Cx\"\nagent = \"codex\"\n",
        root.display()
    );
    let file = root.join("workspaces.toml");
    std::fs::write(&file, text).unwrap();
    let w = Workspaces::load(&file, None).unwrap();
    (root, w)
}

fn refused(host: &str, ws: &str) -> ErrorBody {
    let (root, w) = workspaces("refused", host, ws);
    let e = delegated(&w, "w", None, "claude").unwrap_err();
    let _ = std::fs::remove_dir_all(root);
    e
}

#[test]
fn a_workspace_needs_the_host_switch_and_its_own_delegate() {
    let off = refused("", "[workspace.assistant]\ndelegate = true");
    assert_eq!(off.code, ErrorCode::Unsupported);
    assert!(
        off.message.contains("[assistant] enabled"),
        "{}",
        off.message
    );
    let none = refused(ON, "");
    assert_eq!(none.code, ErrorCode::Unsupported);
    assert!(
        none.message.contains("takes no assistant requests"),
        "{}",
        none.message
    );
    let (root, w) = workspaces("unknown", ON, "");
    assert_eq!(
        delegated(&w, "x", None, "claude").unwrap_err().code,
        ErrorCode::BadRequest
    );
    let _ = std::fs::remove_dir_all(root);
    let overall = refused(ON, "[workspace.assistant]\noverall = true");
    assert_eq!(overall.code, ErrorCode::Unsupported);
    assert!(
        overall.message.contains("overall assistant's"),
        "{}",
        overall.message
    );
}

#[test]
fn codex_and_an_agent_without_adapter_are_refused_by_name() {
    let codex = refused(
        ON,
        "[workspace.assistant]\ndelegate = true\nworker = \"Cx\"",
    );
    assert_eq!(codex.code, ErrorCode::Unsupported);
    assert!(codex.message.contains("Codex"), "{}", codex.message);
    let default_codex = refused(
        ON,
        "[workspace.agent]\ncommand = \"codex --yolo\"\n[workspace.assistant]\ndelegate = true",
    );
    assert!(
        default_codex.message.contains("Codex"),
        "{}",
        default_codex.message
    );
    let other = refused(
        ON,
        "[workspace.agent]\ncommand = \"aider --yes\"\n[workspace.assistant]\ndelegate = true",
    );
    assert_eq!(other.code, ErrorCode::Unsupported);
    assert!(
        other.message.contains("no muxa adapter"),
        "{}",
        other.message
    );
    assert!(
        !other.message.contains("aider"),
        "the command is not echoed"
    );
}

#[test]
fn codex_delegates_with_owned_threads_and_the_shared_daemon() {
    const OWNED: &str = "[codex]\nowned_threads = true\n";
    let host = format!("{ON}{OWNED}");
    for ws in [
        "[workspace.assistant]\ndelegate = true\nworker = \"Cx\"",
        "[workspace.agent]\ncommand = \"codex --yolo\"\n[workspace.assistant]\ndelegate = true",
    ] {
        let (root, w) = workspaces("owned", &host, ws);
        assert!(delegated(&w, "w", None, "claude").is_ok(), "{ws}");
        let _ = std::fs::remove_dir_all(root);
    }
    let unguarded = refused(
        &host,
        "[workspace.agent]\ncommand = \"codex --no-daemon\"\n[workspace.assistant]\ndelegate = true",
    );
    assert_eq!(unguarded.code, ErrorCode::Unsupported);
    assert!(
        unguarded.message.contains("owned_threads"),
        "{}",
        unguarded.message
    );
    let plans = "[workspace.agent]\ncommand = \"codex\"\n[workspace.assistant]\noverall = true";
    let (root, w) = workspaces("overall", &host, plans);
    assert!(overall(&w, "claude").is_ok());
    let _ = std::fs::remove_dir_all(root);
    let (root, w) = workspaces("overall-off", ON, plans);
    let off = overall(&w, "claude").unwrap_err();
    assert!(off.message.contains("Codex"), "{}", off.message);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn claude_code_by_worker_or_default_command_is_the_delegate() {
    let (root, w) = workspaces(
        "ok",
        ON,
        "[workspace.agent]\ncommand = \"env A=1 /opt/bin/claude --x\"\n\
         [workspace.assistant]\ndelegate = true\nmax_open_runs = 2",
    );
    let d = delegated(&w, "w", None, "codex").unwrap();
    assert_eq!((d.worker, d.delegate.max_open_runs), (None, 2));
    assert_eq!(d.host.max_runs_per_day, 3);
    let _ = std::fs::remove_dir_all(root);
    let (root, w) = workspaces(
        "worker",
        ON,
        "[workspace.assistant]\ndelegate = true\nworker = \"Cl\"",
    );
    assert_eq!(
        delegated(&w, "w", None, "aider").unwrap().worker.as_deref(),
        Some("Cl")
    );
    let _ = std::fs::remove_dir_all(root);
}

fn run(id: &str, ws: &str, request: &str, status: Status, created: u64) -> Record {
    let mut r: Record = serde_json::from_value(serde_json::json!({
        "version": 1, "id": id, "rev": 1, "workspace": ws, "title": "t",
        "goal": "g", "status": "open", "createdAt": created, "updatedAt": created,
    }))
    .unwrap();
    r.status = status;
    r.extra.extend(
        RequestRun {
            request: request.into(),
            version: 1,
            work: "w-00000001".into(),
            notes_base: None,
            step: None,
            member: None,
            shared_base: None,
        }
        .fields(),
    );
    r
}

#[test]
fn usage_counts_open_runs_claims_and_today_and_skips_a_replaced_request() {
    let task: Record = serde_json::from_value(serde_json::json!({
        "version": 1, "id": "t-0000a0", "rev": 1, "workspace": "w", "title": "t",
        "goal": "g", "status": "open", "createdAt": 500, "updatedAt": 500,
    }))
    .unwrap();
    let records = vec![
        task,
        run("t-0000a1", "w", "q-00000001", Status::Open, 500),
        run("t-0000a2", "w", "q-00000002", Status::Done, 50),
        run("t-0000a3", "v", "q-00000003", Status::Review, 600),
        run("t-0000a4", "w", "q-00000004", Status::Dropped, 700),
    ];
    let starting = vec![Starting {
        request: "q-00000005".into(),
        workspace: "w".into(),
        since: 800,
    }];
    let u = usage(&records, &starting, "w", &[], 100);
    assert_eq!(
        u,
        Usage {
            open: 2,
            today: 3,
            today_host: 4
        }
    );
    let replacing = usage(&records, &starting, "w", &["q-00000001"], 100);
    assert_eq!((replacing.open, replacing.today), (1, 3));
}

#[test]
fn each_limit_is_a_conflict_that_names_it_and_its_reset() {
    let (root, w) = workspaces(
        "limits",
        ON,
        "[workspace.assistant]\ndelegate = true\nmax_runs_per_day = 2",
    );
    let d = delegated(&w, "w", None, "claude").unwrap();
    let _ = std::fs::remove_dir_all(root);
    assert!(check(Usage::default(), &d, "R").is_ok());
    let cases = [
        (
            Usage {
                open: 1,
                ..Usage::default()
            },
            "max_open_runs",
        ),
        (
            Usage {
                today: 2,
                ..Usage::default()
            },
            "max_runs_per_day. The count resets at R",
        ),
        (
            Usage {
                today_host: 3,
                ..Usage::default()
            },
            "[assistant] max_runs_per_day",
        ),
    ];
    for (u, words) in cases {
        let e = check(u, &d, "R").unwrap_err();
        assert_eq!(e.code, ErrorCode::Conflict);
        assert!(e.message.contains(words), "{}", e.message);
    }
}

#[test]
fn the_day_starts_at_local_midnight_and_resets_at_the_next() {
    let now = 1_762_700_000;
    let (start, reset) = day(now);
    assert!(start <= now && now - start < 86_400 + 3600);
    assert!(reset.contains("00:00"), "{reset}");
}
