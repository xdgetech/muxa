use muxa_protocol::v2::InboxItem;

use super::*;

fn workspaces(name: &str) -> (std::path::PathBuf, Workspaces) {
    let dir = std::env::temp_dir().join(format!("muxa-assistant-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("home/.muxa")).unwrap();
    let dir = dir.canonicalize().unwrap();
    std::fs::write(
        dir.join("home/.muxa/routines.toml"),
        "[[bot]]\nname = \"chores\"\ndescription = \"House things\"\nenabled = true\n",
    )
    .unwrap();
    let file = dir.join("workspaces.toml");
    std::fs::write(
        &file,
        format!(
            "[assistant]\nenabled = true\n[[workspace]]\nname = \"home\"\nroot = \"{}\"\n[workspace.routines]\nenabled = true\n",
            dir.join("home").display()
        ),
    )
    .unwrap();
    let all = Workspaces::load(&file, None).unwrap();
    (dir, all)
}

fn item() -> InboxItem {
    serde_json::from_value(serde_json::json!({
        "id": "i1", "kind": "todo", "title": "fix the sink", "created": 1, "source": "capture"
    }))
    .unwrap()
}

fn raw(v: serde_json::Value) -> RawSuggestion {
    serde_json::from_value(v).unwrap()
}

#[test]
fn a_suggestion_keeps_only_workspaces_and_bots_that_exist() {
    let (dir, all) = workspaces("check");
    assert!(all.assistant().enabled);
    let (s, memory) = checked(
        raw(
            serde_json::json!({"kind": "todo", "workspace": "home", "bot": "chores",
            "due": "2026-12-01", "memory": "Sinks go to home."}),
        ),
        &all,
        &item(),
    )
    .unwrap();
    assert_eq!(s.kind, None, "the same kind is no suggestion");
    assert_eq!(s.workspace.as_deref(), Some("home"));
    assert_eq!(s.bot.as_deref(), Some("chores"));
    assert_eq!(memory.as_deref(), Some("Sinks go to home."));

    let (s, _) = checked(
        raw(serde_json::json!({"workspace": "nowhere", "bot": "chores"})),
        &all,
        &item(),
    )
    .unwrap();
    assert_eq!(
        (s.workspace, s.bot),
        (None, None),
        "no Workspace, so no Bot"
    );
    assert!(checked(raw(serde_json::json!({"kind": "review"})), &all, &item()).is_err());
    let files = crate::records::assistant::files(&dir.join("workspaces.toml")).unwrap();
    let brief = brief(&all, &files, &item(), &[]);
    assert!(brief.contains("fix the sink") && brief.contains("Bot chores: House things"));
    let _ = std::fs::remove_dir_all(dir);
}
