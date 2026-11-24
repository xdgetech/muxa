//! Settings of Bot talks, teams, review Routines and the Capture assistant
//! (ADR 0083 to 0085).
use muxa_inbox::Inbox;
use muxa_protocol::v2::ItemKind;

use super::*;
use crate::v2::talk::Talk;

fn add_bot(h: &Host, name: &str, extra: serde_json::Value) -> Settings {
    let rev = h.store.view().unwrap().revision;
    let mut bot = json!({"name": name, "enabled": true});
    bot.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    h.store
        .update(
            &rev,
            vec![op(
                json!({"op": "bot-add", "workspace": "Work", "bot": bot}),
            )],
        )
        .unwrap()
}

#[test]
fn the_assistant_has_its_table_and_its_soul_beside_the_file() {
    let h = host("assistant");
    let s = h.store.view().unwrap();
    assert_eq!(s.assistant.as_ref().map(|a| a.enabled), Some(false));
    let s = h
        .store
        .update(
            &s.revision,
            vec![op(json!({"op": "assistant-update", "enabled": true,
                "soul_text": "Sort my captures by Workspace.\n"}))],
        )
        .unwrap();
    let a = s.assistant.unwrap();
    assert!(a.enabled);
    assert_eq!(
        a.soul_text.as_deref(),
        Some("Sort my captures by Workspace.\n")
    );
    assert!(
        text(&h).contains("[assistant]\nenabled = true"),
        "{}",
        text(&h)
    );
    let soul = fs::read_to_string(h.dir.join("assistant/soul.md")).unwrap();
    assert_eq!(soul, "Sort my captures by Workspace.\n");
    let all = crate::records::Workspaces::load(&h.file, Some(&h.dir)).unwrap();
    assert!(all.assistant().enabled);
}

#[test]
fn a_lead_names_its_members_and_teams_stay_one_level_deep() {
    let h = host("team");
    add_bot(&h, "scout", json!({}));
    let s = add_bot(
        &h,
        "lead",
        json!({"members": ["scout"], "handoffs_per_day": 4}),
    );
    let lead = s.workspaces[0]
        .bots
        .iter()
        .find(|b| b.spec.name == "lead")
        .unwrap();
    assert_eq!(lead.spec.members, ["scout"]);
    assert_eq!(lead.spec.handoffs_per_day, Some(4));
    let nested = h.store.update(
        &s.revision,
        vec![op(json!({"op": "bot-update", "workspace": "Work",
            "bot": {"name": "scout", "enabled": true, "members": ["lead"]}}))],
    );
    assert!(
        matches!(nested, Err(SettingsError::Invalid(_))),
        "{nested:?}"
    );
    let missing = h.store.update(
        &s.revision,
        vec![op(json!({"op": "bot-update", "workspace": "Work",
            "bot": {"name": "lead", "enabled": true, "members": ["nobody"]}}))],
    );
    assert!(
        matches!(missing, Err(SettingsError::Invalid(_))),
        "{missing:?}"
    );
}

#[test]
fn a_review_routine_runs_on_review_and_has_no_schedule() {
    let h = host("on-review");
    let rev = h.store.view().unwrap().revision;
    let s = h
        .store
        .update(
            &rev,
            vec![op(json!({"op": "routine-add", "workspace": "Work",
                "routine": {"name": "check", "instructions": "routines/digest.md",
                    "enabled": true, "on": "review"}}))],
        )
        .unwrap();
    let check = s.workspaces[0]
        .routines
        .iter()
        .find(|r| r.spec.name == "check")
        .unwrap();
    assert_eq!(check.spec.on.as_deref(), Some("review"));
    let both = h.store.update(
        &s.revision,
        vec![op(json!({"op": "routine-update", "workspace": "Work",
            "routine": {"name": "check", "instructions": "routines/digest.md",
                "enabled": true, "on": "review", "at": "09:00", "timezone": "UTC"}}))],
    );
    assert!(matches!(both, Err(SettingsError::Invalid(_))), "{both:?}");
}

fn talk(h: &Host, bot: &str) -> Talk {
    let dir = h.dir.join("work/tasks/talk-1");
    fs::create_dir_all(dir.join(".muxa")).unwrap();
    Talk {
        workspace: "Work".into(),
        bot: bot.into(),
        terminal: "t1".into(),
        dir,
    }
}

fn records(h: &Host) -> crate::config::Config {
    crate::config::Config {
        state_dir: None,
        workspaces: Some(h.file.clone()),
        home: Some(h.dir.clone()),
    }
}

#[test]
fn a_talks_proposal_becomes_one_decide_item_that_applies() {
    let h = host("talk");
    add_bot(&h, "b", json!({}));
    let t = talk(&h, "b");
    let inbox = Inbox::memory();
    let output = json!({
        "memory": "The user wants PRs at 9.",
        "proposal": {"summary": "a morning scan", "ops": [
            {"op": "routine-add", "workspace": "Work",
             "routine": {"name": "scan", "instructions": "routines/scan.md", "enabled": true,
                 "bot": "b", "at": "09:00", "timezone": "Europe/Berlin"},
             "instructions_text": "List the PRs.\n"}
        ]}
    });
    fs::write(t.dir.join(".muxa/bot.json"), output.to_string()).unwrap();
    t.handle(&records(&h), &h.store, &inbox);
    t.handle(&records(&h), &h.store, &inbox);

    let items = inbox.view().items;
    assert_eq!(items.len(), 1, "handled once");
    let item = &items[0];
    assert_eq!(item.kind, ItemKind::Decide);
    assert_eq!(item.title, "b proposes: a morning scan");
    assert_eq!(item.bot.as_deref(), Some("b"));
    assert_eq!(item.terminal.as_deref(), Some("t1"));
    assert_eq!(
        item.note.as_deref(),
        Some("- Add Routine scan (daily at 09:00, off)\n")
    );
    let proposal = item.settings_proposal.clone().unwrap();
    let memory = fs::read_to_string(h.dir.join("work/.muxa/bots/b/memory.md")).unwrap();
    assert_eq!(memory, "The user wants PRs at 9.");
    let check = fs::read_to_string(t.dir.join(".muxa/CHECK.md")).unwrap();
    assert!(check.contains("passed"), "{check}");

    let s = h.store.update(&proposal.revision, proposal.ops).unwrap();
    let scan = s.workspaces[0]
        .routines
        .iter()
        .find(|r| r.spec.name == "scan")
        .unwrap();
    assert!(!scan.spec.enabled, "a proposed Routine is added turned off");
}

#[test]
fn an_old_daily_items_loads_is_ignored_and_goes_on_the_next_update() {
    let h = host("dailyitems");
    let file = h.dir.join("work/.muxa/routines.toml");
    let old = fs::read_to_string(&file).unwrap()
        + "\n[[bot]]\nname = \"old\"\nenabled = true\ndaily_items = 0\n";
    fs::write(&file, old).unwrap();
    let s = h.store.view().unwrap();
    assert!(s.workspaces[0].bots.iter().any(|b| b.spec.name == "old"));
    // An app from before ADR 0091 still sends the field; it is accepted.
    h.store
        .update(
            &s.revision,
            vec![op(json!({"op": "bot-update", "workspace": "Work",
                "bot": {"name": "old", "enabled": true, "daily_items": 5}}))],
        )
        .unwrap();
    let written = fs::read_to_string(&file).unwrap();
    assert!(!written.contains("daily_items"), "{written}");
}

#[test]
fn a_talk_adds_the_items_the_user_asked_for_once() {
    let h = host("talkitems");
    add_bot(&h, "b", json!({}));
    let t = talk(&h, "b");
    let inbox = Inbox::memory();
    let item = |key: &str, title: &str| {
        json!({"kind": "todo", "key": key, "title": title,
        "body": "From the 11-18 leads meeting.", "due": "2026-11-25",
        "links": [{"url": "https://example.com/notes", "title": "Meeting note"}]})
    };
    let output = json!({"items": [item("n:1", "Start the 0.8 code-freeze thread"), item("n:2", "Post the flag plan")]});
    fs::write(t.dir.join(".muxa/bot.json"), output.to_string()).unwrap();
    t.handle(&records(&h), &h.store, &inbox);
    t.handle(&records(&h), &h.store, &inbox);

    let items = inbox.view().items;
    assert_eq!(items.len(), 2, "both, handled once");
    let first = items
        .iter()
        .find(|i| i.title == "Start the 0.8 code-freeze thread")
        .unwrap();
    assert_eq!(first.kind, ItemKind::Todo);
    assert_eq!(first.bot.as_deref(), Some("b"));
    assert_eq!(first.terminal.as_deref(), Some("t1"));
    assert_eq!(first.due.as_deref(), Some("2026-11-25"));
    assert_eq!(first.links[0].url, "https://example.com/notes");

    // The same key again in a new output adds nothing.
    let again = json!({"items": [item("n:1", "Start the 0.8 code-freeze thread")], "memory": "x"});
    fs::write(t.dir.join(".muxa/bot.json"), again.to_string()).unwrap();
    t.handle(&records(&h), &h.store, &inbox);
    assert_eq!(inbox.view().items.len(), 2);

    // A bad item refuses the whole output: nothing is added or written.
    let bad = json!({"items": [{"kind": "nope", "key": "k", "title": "t"}], "memory": "never"});
    fs::write(t.dir.join(".muxa/bot.json"), bad.to_string()).unwrap();
    t.handle(&records(&h), &h.store, &inbox);
    assert_eq!(inbox.view().items.len(), 2);
    let check = fs::read_to_string(t.dir.join(".muxa/CHECK.md")).unwrap();
    assert!(
        check.contains("refused") && check.contains("kind must be"),
        "{check}"
    );
}

#[test]
fn a_talk_may_not_touch_connections_or_another_bots_routines() {
    let h = host("talk-refused");
    add_bot(&h, "b", json!({}));
    let t = talk(&h, "b");
    let inbox = Inbox::memory();
    for ops in [
        json!([{"op": "connection-remove", "workspace": "Work", "name": "linear"}]),
        json!([{"op": "routine-remove", "workspace": "Work", "name": "digest"}]),
        json!([{"op": "bot-update", "workspace": "Home", "bot": {"name": "b", "enabled": true}}]),
    ] {
        let output = json!({"memory": "x", "proposal": {"summary": "s", "ops": ops}});
        fs::write(t.dir.join(".muxa/bot.json"), output.to_string()).unwrap();
        t.handle(&records(&h), &h.store, &inbox);
        let check = fs::read_to_string(t.dir.join(".muxa/CHECK.md")).unwrap();
        assert!(check.contains("refused"), "{check}");
    }
    assert!(inbox.view().items.is_empty());
    assert!(
        !h.dir.join("work/.muxa/bots/b/memory.md").exists(),
        "a refused output writes no memory"
    );
}

#[test]
fn a_talk_about_a_new_bot_may_propose_adding_it() {
    let h = host("talk-new");
    let t = talk(&h, "fresh");
    let inbox = Inbox::memory();
    let output = json!({"proposal": {"summary": "me", "ops": [
        {"op": "bot-add", "workspace": "Work", "bot": {"name": "fresh", "enabled": true},
         "soul_text": "I am new.\n"}
    ]}});
    fs::write(t.dir.join(".muxa/bot.json"), output.to_string()).unwrap();
    t.handle(&records(&h), &h.store, &inbox);
    let item = &inbox.view().items[0];
    assert_eq!(item.note.as_deref(), Some("- Add Bot fresh, with a Soul\n"));
}

#[test]
fn a_talks_output_that_is_a_symlink_is_refused() {
    let h = host("talk-link");
    add_bot(&h, "b", json!({}));
    let t = talk(&h, "b");
    let elsewhere = h.dir.join("elsewhere.json");
    fs::write(&elsewhere, r#"{"memory": "x"}"#).unwrap();
    std::os::unix::fs::symlink(&elsewhere, t.dir.join(".muxa/bot.json")).unwrap();
    t.handle(&records(&h), &h.store, &Inbox::memory());
    let check = fs::read_to_string(t.dir.join(".muxa/CHECK.md")).unwrap();
    assert!(
        check.contains("refused") && check.contains("symlink"),
        "{check}"
    );
    assert!(!h.dir.join("work/.muxa/bots/b/memory.md").exists());
}

#[test]
fn a_renamed_bot_keeps_its_routines_team_and_files() {
    let h = host("rename");
    let s = add_bot(&h, "lisk-cheif", json!({}));
    let s = h
        .store
        .update(
            &s.revision,
            vec![
                op(json!({"op": "bot-update", "workspace": "Work", "bot": {"name": "lisk-cheif", "enabled": true}, "soul_text": "I help.\n"})),
                op(json!({"op": "routine-add", "workspace": "Work",
                    "routine": {"name": "todo", "instructions": "routines/todo.md", "enabled": false, "bot": "lisk-cheif", "at": "09:00", "timezone": "UTC"},
                    "instructions_text": "List to-dos.\n"})),
                op(json!({"op": "bot-add", "workspace": "Work", "bot": {"name": "lead", "enabled": true, "members": ["lisk-cheif"]}})),
            ],
        )
        .unwrap();
    let folder = |n: &str| h.dir.join("work/.muxa/bots").join(n);
    assert!(folder("lisk-cheif").join("soul.md").is_file());

    let s = h
        .store
        .update(
            &s.revision,
            vec![op(json!({"op": "bot-rename", "workspace": "Work", "from": "lisk-cheif", "to": "lisk-chief"}))],
        )
        .unwrap();
    let ws = &s.workspaces[0];
    assert!(ws.bots.iter().any(|b| b.spec.name == "lisk-chief"));
    assert!(!ws.bots.iter().any(|b| b.spec.name == "lisk-cheif"));
    let todo = ws.routines.iter().find(|r| r.spec.name == "todo").unwrap();
    assert_eq!(todo.spec.bot.as_deref(), Some("lisk-chief"));
    let lead = ws.bots.iter().find(|b| b.spec.name == "lead").unwrap();
    assert_eq!(lead.spec.members, ["lisk-chief"]);
    assert_eq!(
        fs::read_to_string(folder("lisk-chief").join("soul.md")).unwrap(),
        "I help.\n"
    );
    assert!(!folder("lisk-cheif").exists(), "the folder moved");

    let taken = h.store.update(
        &s.revision,
        vec![op(
            json!({"op": "bot-rename", "workspace": "Work", "from": "lisk-chief", "to": "lead"}),
        )],
    );
    assert!(matches!(taken, Err(SettingsError::Invalid(_))), "{taken:?}");
    let bad = h.store.update(
        &s.revision,
        vec![op(json!({"op": "bot-rename", "workspace": "Work", "from": "lisk-chief", "to": "Lisk Chief"}))],
    );
    assert!(bad.is_err(), "names are lowercase");
}
