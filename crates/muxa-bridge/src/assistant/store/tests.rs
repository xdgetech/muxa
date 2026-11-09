use super::*;
use muxa_protocol::{CaptureSource, CaptureState, WorkState};

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "muxa-assistant-store-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn capture(id: String, client_id: &str, text: &str) -> Capture {
    Capture {
        id,
        rev: 1,
        client_id: client_id.into(),
        text: text.into(),
        source: CaptureSource::App,
        state: CaptureState::Received,
        created_at: 1,
        updated_at: 1,
        workspace: None,
        work: Vec::new(),
    }
}

fn work(id: String) -> WorkItem {
    WorkItem {
        id,
        rev: 1,
        title: "Shorten onboarding".into(),
        state: WorkState::Open,
        created_at: 1,
        updated_at: 1,
        workspace: None,
        later_at: None,
        merged_into: None,
        captures: Vec::new(),
        requests: Vec::new(),
        tasks: Vec::new(),
    }
}

fn mode(path: &Path) -> u32 {
    std::os::unix::fs::PermissionsExt::mode(&fs::metadata(path).unwrap().permissions()) & 0o777
}

#[test]
fn the_same_client_id_returns_the_stored_capture_and_writes_nothing() {
    let dir = temp();
    let shelf = Shelf::<Capture>::open(&dir).unwrap();
    let Captured::New(first) = shelf
        .capture::<StateError>("client-1", |id| Ok(capture(id, "client-1", "first")))
        .unwrap()
    else {
        panic!("not new")
    };
    let file = dir
        .join("assistant/captures")
        .join(format!("{}.json", first.id));
    let before = fs::read(&file).unwrap();
    let again = shelf
        .capture::<StateError>("client-1", |_| panic!("made a second capture"))
        .unwrap();
    assert_eq!(again, Captured::Known(first.clone()));
    assert_eq!(fs::read(&file).unwrap(), before);
    let Captured::New(other) = shelf
        .capture::<StateError>("client-2", |id| Ok(capture(id, "client-2", "second")))
        .unwrap()
    else {
        panic!("not new")
    };
    assert_ne!(other.id, first.id);
    assert_eq!(shelf.all().unwrap().len(), 2);
    assert_eq!(mode(&dir.join("assistant/captures")), 0o700);
    assert_eq!(mode(&file), 0o600);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_hash_collision_takes_the_next_id() {
    let dir = temp();
    let shelf = Shelf::<Capture>::open(&dir).unwrap();
    // Another client's capture already sits on this client's first id.
    let taken = capture_id("client-1", 0);
    let squatter = capture(taken.clone(), "someone-else", "x");
    shelf.write_new::<StateError>(&taken, squatter).unwrap();
    let Captured::New(made) = shelf
        .capture::<StateError>("client-1", |id| Ok(capture(id, "client-1", "mine")))
        .unwrap()
    else {
        panic!("not new")
    };
    assert_eq!(made.id, capture_id("client-1", 1));
    let again = shelf
        .capture::<StateError>("client-1", |_| panic!())
        .unwrap();
    assert_eq!(again, Captured::Known(made));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_failed_change_writes_nothing_and_a_good_one_is_kept() {
    let dir = temp();
    let shelf = Shelf::<WorkItem>::open(&dir).unwrap();
    let item = shelf.create::<StateError>(|id| Ok(work(id))).unwrap();
    assert!(item.id.starts_with("w-"));
    let failed: Result<Option<()>, StateError> = shelf.update(&item.id, |w| {
        w.title = "changed".into();
        Err(StateError::TooLarge("x".into()))
    });
    assert!(failed.is_err());
    assert_eq!(shelf.get(&item.id).unwrap().unwrap().title, item.title);
    shelf
        .update::<_, StateError>(&item.id, |w| {
            w.rev += 1;
            Ok(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(shelf.get(&item.id).unwrap().unwrap().rev, 2);
    let missing = shelf
        .update::<_, StateError>("w-00000000", |_| -> Result<(), _> { panic!("ran") })
        .unwrap();
    assert_eq!(missing, None);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_record_under_another_id_is_refused() {
    let dir = temp();
    let shelf = Shelf::<WorkItem>::open(&dir).unwrap();
    let refused = shelf.create::<StateError>(|_| Ok(work("w-0000abcd".into())));
    assert!(refused.is_err());
    assert!(shelf.all().unwrap().is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn fields_of_a_newer_muxa_survive_a_rewrite() {
    let dir = temp();
    let shelf = Shelf::<WorkItem>::open(&dir).unwrap();
    let item = shelf.create::<StateError>(|id| Ok(work(id))).unwrap();
    let file = dir.join("assistant/work").join(format!("{}.json", item.id));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    value["dueAt"] = serde_json::json!(1_762_700_000);
    fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    shelf
        .update::<_, StateError>(&item.id, |w| {
            w.rev += 1;
            Ok(())
        })
        .unwrap();
    let after: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(after["dueAt"], 1_762_700_000);
    assert_eq!(after["rev"], 2);
    assert_eq!(after["version"], FORMAT_VERSION);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_request_keeps_its_own_version_beside_the_format() {
    let dir = temp();
    let shelf = Shelf::<AssistantRequest>::open(&dir).unwrap();
    let made = shelf
        .create::<StateError>(|id| {
            Ok(AssistantRequest {
                id,
                work: "w-0000000b".into(),
                version: 3,
                workspace: "w".into(),
                scope: muxa_protocol::AssistScope::Draft,
                delivery: muxa_protocol::Delivery::Pending,
                delivery_at: 1,
                created_at: 1,
                updated_at: 1,
                purpose: None,
                run: None,
                results: Vec::new(),
            })
        })
        .unwrap();
    let file = dir
        .join("assistant/requests")
        .join(format!("{}.json", made.id));
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        (value["format"].as_u64(), value["version"].as_u64()),
        (Some(1), Some(3))
    );
    assert_eq!(shelf.get(&made.id).unwrap(), Some(made));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_unknown_version_or_a_bad_name_is_not_read() {
    let dir = temp();
    let shelf = Shelf::<WorkItem>::open(&dir).unwrap();
    let item = shelf.create::<StateError>(|id| Ok(work(id))).unwrap();
    let folder = dir.join("assistant/work");
    let file = folder.join(format!("{}.json", item.id));
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    value["version"] = serde_json::json!(2);
    fs::write(&file, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(shelf.get(&item.id).is_err());
    assert!(shelf.all().unwrap().is_empty());
    fs::write(folder.join("../../w-1.json"), b"{}").unwrap();
    fs::write(folder.join("notes.json"), b"{}").unwrap();
    assert!(shelf.all().unwrap().is_empty());
    assert!(shelf.get("w-../../x").is_err());
    assert!(shelf.get("c-0000abcd").is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_file_past_the_limit_is_not_written() {
    let dir = temp();
    let shelf = Shelf::<WorkItem>::open(&dir).unwrap();
    let item = shelf.create::<StateError>(|id| Ok(work(id))).unwrap();
    let huge = "x".repeat(MAX_FILE as usize);
    let refused = shelf.update::<_, StateError>(&item.id, |w| {
        w.title = huge;
        Ok(())
    });
    assert!(matches!(refused, Err(StateError::TooLarge(_))));
    assert_eq!(shelf.get(&item.id).unwrap().unwrap().title, item.title);
    fs::remove_dir_all(dir).unwrap();
}
