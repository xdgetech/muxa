use super::*;

fn proposal(task: &str, text: &str) -> MemoryProposal {
    MemoryProposal {
        task: task.into(),
        title: "Move CI to arm runners".into(),
        text: text.into(),
        hash: hash(task, "Move CI to arm runners", text),
    }
}

#[test]
fn the_hash_is_task_nul_title_nul_text_as_the_fixture_has_it() {
    let text = "- CI runs on arm64 runners since November 2026. x86 images are no longer built.\n";
    assert_eq!(
        hash("t-00b004", "Move CI to arm runners", text),
        "sha256:a3610dd52492f10423a18af4eaff467b99d92a5afb62fa72c1897c68747de24c"
    );
    assert_ne!(hash("t-1", "a", "bc"), hash("t-1", "ab", "c"));
}

#[test]
fn approve_takes_up_to_4_kib_without_replaced_characters() {
    assert!(approvable("- One decision."));
    assert!(approvable(&"a".repeat(MAX_SECTION)));
    assert!(!approvable(&"a".repeat(MAX_SECTION + 1)));
    assert!(!approvable("bell \u{fffd} here"));
    assert!(!approvable(""));
}

#[test]
fn the_block_carries_the_task_and_the_day_and_marks_it_appended() {
    let p = proposal("t-00b004", "- arm64 only.\n");
    // 2025-11-11 00:00 UTC.
    let block = block(&p, 1_762_819_200);
    assert_eq!(
        block,
        "## Move CI to arm runners (t-00b004, 2025-11-11)\n\n- arm64 only.\n"
    );
    assert!(appended(&format!("# Lisk\n\n{block}"), "t-00b004"));
    assert!(!appended(&block, "t-00b00"), "another task");
    assert!(
        !appended("- see (t-00b004, x)\n", "t-00b004"),
        "not a heading"
    );
}

#[test]
fn append_writes_once_under_the_lock_and_names_the_sizes_past_the_cap() {
    let base = std::env::temp_dir().join(format!(
        "muxa-wsmem-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let (root, state) = (base.join("root"), base.join("state"));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let config = Config {
        state_dir: Some(state.clone()),
        workspaces: None,
        home: None,
    };
    let p = proposal("t-00b004", "- arm64 only.");
    let first = append(&config, &root, &p, 1_762_819_200).unwrap();
    assert!(matches!(first, Appended::Added { .. }));
    assert_eq!(
        append(&config, &root, &p, 1_762_905_600).unwrap(),
        Appended::Present,
        "a second approve, also on another day, adds nothing"
    );
    let live = std::fs::read_to_string(root.join(".muxa/WORKSPACE.md")).unwrap();
    assert_eq!(live.matches("(t-00b004, ").count(), 1);
    assert!(state.join("workspaces").is_dir());

    std::fs::write(root.join(".muxa/WORKSPACE.md"), "x".repeat(16 << 10)).unwrap();
    let e = append(&config, &root, &proposal("t-00b005", "- more"), 1).unwrap_err();
    assert!(e.to_string().contains("the cap is 16 KiB"), "{e}");

    // A Routine or assistant run never proposes, and `off` reads nothing.
    let toml = |mode: &str| {
        format!(
            "[[workspace]]\nname = \"w\"\nroot = \"{}\"\nmemory = \"{mode}\"\n",
            root.display()
        )
    };
    let task = crate::task_records::brief::render(&record(&root, false));
    let at = task.rfind("Nothing yet.").unwrap();
    let dir = root.join("t");
    std::fs::create_dir_all(dir.join(".muxa")).unwrap();
    std::fs::write(
        dir.join(".muxa/TASK.md"),
        format!("{}- Kept.\n", &task[..at]),
    )
    .unwrap();
    let ws = |mode: &str| {
        crate::records::Workspaces::parse(&toml(mode), Path::new("w.toml"), None).unwrap()
    };
    let end = closeout::read(&ws("review"), &record(&dir, false)).expect("a proposal");
    assert_eq!(end.section.text, "- Kept.");
    assert!(closeout::read(&ws("off"), &record(&dir, false)).is_none());
    assert!(closeout::read(&ws("auto"), &record(&dir, true)).is_none());
    let _ = std::fs::remove_dir_all(&base);
}

fn record(dir: &Path, background: bool) -> crate::records::Record {
    let mut r: crate::records::Record = serde_json::from_value(serde_json::json!({
        "version": 1, "id": "t-00b006", "rev": 1, "workspace": "w", "title": "Keep it",
        "goal": "g", "status": "done", "createdAt": 1, "updatedAt": 1,
        "dir": dir.display().to_string(),
    }))
    .unwrap();
    if background {
        r.extra.insert(
            crate::records::KIND_FIELD.into(),
            serde_json::json!("routine-run"),
        );
    }
    r
}
