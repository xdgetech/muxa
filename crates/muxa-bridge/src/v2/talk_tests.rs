use muxa_protocol::settings::{BotSpec, RoutineSpec, SettingsBot, SettingsRoutine};
use serde_json::json;

use super::*;

fn ws() -> SettingsWorkspace {
    let mut w: SettingsWorkspace = serde_json::from_value(json!({
        "name": "w", "root": "/w", "agent": {"command": null, "env": []}
    }))
    .unwrap();
    w.bots = vec![SettingsBot {
        spec: BotSpec {
            name: "b".into(),
            description: Some("Watches PRs".into()),
            worker: None,
            daily_items: None,
            enabled: true,
            members: Vec::new(),
            handoffs_per_day: None,
        },
        soul_text: Some("I watch PRs.".into()),
        memory_text: Some("PR 3 seen.".into()),
        memory_versions: 0,
        memory_history: Vec::new(),
    }];
    let routine = |name: &str, bot: Option<&str>| SettingsRoutine {
        spec: RoutineSpec {
            name: name.into(),
            at: None,
            schedule: Some("0 9 * * *".into()),
            timezone: None,
            worker: None,
            instructions: format!("routines/{name}.md"),
            project: None,
            enabled: true,
            items: None,
            direct_writes: false,
            bot: bot.map(str::to_owned),
            read_only: false,
            on: None,
        },
        instructions_text: Some("List PRs.".into()),
    };
    w.routines = vec![routine("scan", Some("b")), routine("other", None)];
    w
}

fn ops(v: serde_json::Value) -> Vec<SettingsOp> {
    serde_json::from_value(v).unwrap()
}

#[test]
fn a_talk_changes_only_its_own_bot_and_routines() {
    let w = ws();
    let fine = check_ops(
        &w,
        "b",
        ops(json!([
            {"op": "bot-update", "workspace": "w", "bot": {"name": "b", "enabled": true}},
            {"op": "routine-update", "workspace": "w",
             "routine": {"name": "scan", "instructions": "routines/scan.md", "enabled": true, "bot": "b"}},
            {"op": "routine-remove", "workspace": "w", "name": "scan"},
        ])),
    );
    assert!(fine.is_ok(), "{fine:?}");
    for bad in [
        json!([{"op": "routine-remove", "workspace": "w", "name": "other"}]),
        json!([{"op": "routine-update", "workspace": "w",
            "routine": {"name": "other", "instructions": "x.md", "enabled": true, "bot": "b"}}]),
        json!([{"op": "bot-update", "workspace": "w", "bot": {"name": "c", "enabled": true}}]),
        json!([{"op": "bot-add", "workspace": "w", "bot": {"name": "b", "enabled": true}}]),
        json!([{"op": "worker-remove", "name": "x"}]),
        json!([]),
    ] {
        assert!(check_ops(&w, "b", ops(bad.clone())).is_err(), "{bad}");
    }
}

#[test]
fn an_added_routine_is_turned_off_and_reads_as_one_line() {
    let checked = check_ops(
        &ws(),
        "b",
        ops(json!([{"op": "routine-add", "workspace": "w",
            "routine": {"name": "n", "instructions": "routines/n.md", "enabled": true,
                "bot": "b", "on": "review"}}])),
    )
    .unwrap();
    let SettingsOp::RoutineAdd { routine, .. } = &checked[0] else {
        panic!("{checked:?}");
    };
    assert!(!routine.enabled);
    assert_eq!(describe(&checked[0]), "Add Routine n (on review, off)");
}

#[test]
fn the_brief_carries_the_soul_memory_routines_and_the_output() {
    let text = brief(&ws(), "b");
    for part in [
        "I watch PRs.",
        "PR 3 seen.",
        "List PRs.",
        ".muxa/bot.json",
        "routine-add",
    ] {
        assert!(text.contains(part), "{part}");
    }
    assert!(
        !text.contains("routines/other.md"),
        "another Routine is not shown"
    );
    let new = brief(&ws(), "fresh");
    assert!(new.contains("You do not exist yet") && new.contains("- b: Watches PRs"));
}

#[test]
fn the_brief_says_what_runs_can_use_and_what_the_daily_limit_does() {
    let mut w = ws();
    w.mcp = serde_json::from_value(json!([{"name": "linear", "command": "npx"}])).unwrap();
    let text = brief(&w, "b");
    for part in [
        "not Claude cloud routines",
        "its MCP servers: linear.",
        "as environment variables: none;",
        "You are a muxa Bot",
        "you may propose",
        "claude.ai connectors do not",
        "\"N more\"",
        "One item per key",
    ] {
        assert!(text.contains(part), "{part}");
    }
}

#[test]
fn an_output_too_big_or_without_a_summary_is_refused() {
    assert!(parse(br#"{"memory": "ok"}"#).is_ok());
    assert!(parse(br#"{"proposal": {"summary": " ", "ops": []}}"#).is_err());
    assert!(parse(br#"{"other": 1}"#).is_err());
    let big = format!(r#"{{"memory": "{}"}}"#, "x".repeat(MAX_MEMORY + 1));
    assert!(parse(big.as_bytes()).is_err());
}
