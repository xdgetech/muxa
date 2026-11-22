use std::path::Path;

use super::*;

/// A v1 record as v1 wrote it.
pub(crate) fn record(dir: &Path, id: &str, status: &str, title: &str, place: &Path) {
    let json = serde_json::json!({
        "version": 1, "id": id, "rev": 3, "workspace": "w", "title": title,
        "goal": "", "status": status, "createdAt": 100, "updatedAt": 200,
        "dir": place, "session": "w-fix", "runs": [{"id": "r1"}],
    });
    std::fs::write(dir.join(format!("{id}.json")), json.to_string()).unwrap();
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("muxa-v1-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[test]
fn only_open_task_records_are_read_and_nothing_changes() {
    let base = temp("read");
    let place = base.join("place");
    record(&base, "t-000003", "open", "Fix", &place);
    record(&base, "t-000001", "review", "Ship", &place);
    record(&base, "t-000002", "done", "Old", &place);
    record(&base, "t-000004", "dropped", "Gone", &place);
    let run = serde_json::json!({"id": "t-000005", "workspace": "w", "title": "digest",
        "status": "open", "kind": "routine-run"});
    std::fs::write(base.join("t-000005.json"), run.to_string()).unwrap();
    std::fs::write(base.join("t-000006.json"), "{not json").unwrap();
    std::fs::write(base.join("notes.json"), "{}").unwrap();
    let before: Vec<_> = std::fs::read_dir(&base)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (p.clone(), std::fs::read(&p).unwrap())
        })
        .collect();

    let ids: Vec<String> = open_records(&base)
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(ids, ["t-000001", "t-000003"]);
    for (p, bytes) in before {
        assert_eq!(
            std::fs::read(&p).unwrap(),
            bytes,
            "{} unchanged",
            p.display()
        );
    }
    assert!(open_records(&base.join("none")).unwrap().is_empty());
    std::fs::remove_dir_all(base).unwrap();
}

#[tokio::test]
async fn each_task_says_whether_its_place_can_be_adopted() {
    let base = temp("tasks");
    let (state, root) = (base.join("state"), base.join("root"));
    std::fs::create_dir_all(&state).unwrap();
    let here = root.join("tasks/fix-t-000001");
    std::fs::create_dir_all(&here).unwrap();
    std::fs::create_dir_all(base.join("outside")).unwrap();
    record(&state, "t-000001", "open", "Fix", &here);
    record(
        &state,
        "t-000002",
        "open",
        "Lost",
        &root.join("tasks/lost-t-000002"),
    );
    record(&state, "t-000003", "open", "Away", &base.join("outside"));
    let other = serde_json::json!({"version": 1, "id": "t-000004", "workspace": "gone",
        "title": "Elsewhere", "status": "open", "dir": here});
    std::fs::write(state.join("t-000004.json"), other.to_string()).unwrap();
    let toml = format!(
        "[[workspace]]\nname = \"w\"\nroot = \"{}\"\n",
        root.display()
    );
    let workspaces = Workspaces::parse(&toml, Path::new("w.toml"), None).unwrap();

    let found = tasks(Some(&state), &workspaces).await.unwrap();
    let got: Vec<(&str, bool, V1Status)> = found
        .iter()
        .map(|t| (t.id.as_str(), t.exists, t.status))
        .collect();
    assert_eq!(
        got,
        [
            ("t-000001", true, V1Status::New),
            ("t-000002", false, V1Status::Missing),
            ("t-000003", true, V1Status::Outside),
            ("t-000004", true, V1Status::NoWorkspace),
        ]
    );
    assert_eq!(found[0].path, here.to_string_lossy());
    assert_eq!(found[0].title, "Fix");
    assert!(tasks(None, &workspaces).await.unwrap().is_empty());
    std::fs::remove_dir_all(base).unwrap();
}
