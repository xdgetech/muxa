use std::path::PathBuf;

use muxa_protocol::ItemState;

use super::*;
use crate::assistant::capture::{NewCapture, save};
use crate::items::store::{ItemStore, StoredItem, tests_item};
use crate::records::{NewRecord, Status, Store};

struct Host {
    base: PathBuf,
    config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-assistant-work-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let root = base.join("root");
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let workspaces = base.join("workspaces.toml");
    std::fs::write(
        &workspaces,
        format!(
            "[[workspace]]\nname = \"work\"\nroot = \"{}\"\n\n[[workspace]]\nname = \"home\"\nroot = \"{}\"\n",
            root.display(),
            root.display()
        ),
    )
    .unwrap();
    let config = Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(workspaces),
        home: None,
    };
    Host { base, config }
}

impl Host {
    fn capture(&self, client_id: &str, text: &str) -> Capture {
        let new = NewCapture {
            client_id: client_id.into(),
            text: text.into(),
            source: None,
            workspace: Some("work".into()),
            assist: None,
        };
        save(&self.config, new).unwrap().0
    }

    fn sort(
        &self,
        c: &Capture,
        action: SortAction,
        work: Option<&str>,
    ) -> Result<(Capture, Option<WorkItem>), ErrorBody> {
        sort(
            &self.config,
            Sort {
                capture: c.id.clone(),
                rev: c.rev,
                action,
                work: work.map(Into::into),
                title: None,
                workspace: None,
            },
        )
    }

    /// A Work item from a new Capture.
    fn item(&self, text: &str) -> WorkItem {
        let client = format!("item-{:08x}", getrandom::u32().unwrap());
        let c = self.capture(&client, text);
        self.sort(&c, SortAction::Work, None).unwrap().1.unwrap()
    }

    fn act(&self, w: &WorkItem, action: WorkAction) -> Result<WorkItem, ErrorBody> {
        update(&self.config, change(w, action))
    }

    fn listed(&self) -> Vec<WorkItem> {
        crate::assistant::list(&self.config).unwrap().work
    }
}

fn change(w: &WorkItem, action: WorkAction) -> Update {
    Update {
        work: w.id.clone(),
        rev: w.rev,
        action,
        title: None,
        workspace: None,
        into: None,
    }
}

fn code(r: Result<impl std::fmt::Debug, ErrorBody>) -> ErrorCode {
    r.unwrap_err().code
}

#[test]
fn sorting_into_work_takes_the_first_line_and_links_both_ways() {
    let h = host();
    let c = h.capture("client-1", "\n  Shorten onboarding  \nsecond line");
    let (sorted, item) = h.sort(&c, SortAction::Work, None).unwrap();
    let item = item.unwrap();
    assert_eq!(item.title, "Shorten onboarding");
    assert_eq!(item.workspace.as_deref(), Some("work"));
    assert_eq!(item.captures, vec![c.id.clone()]);
    assert_eq!((sorted.state, sorted.rev), (CaptureState::Sorted, 2));
    assert_eq!(sorted.work, vec![item.id.clone()]);
    assert_eq!(sorted.text, c.text);
    // The same request again carries a stale rev; nothing else is made.
    assert_eq!(
        code(h.sort(&c, SortAction::Work, None)),
        ErrorCode::Conflict
    );
    assert_eq!(h.listed().len(), 1);
    assert!(
        crate::assistant::list(&h.config)
            .unwrap()
            .captures
            .is_empty()
    );
}

#[test]
fn a_long_first_line_is_cut_to_200_characters() {
    let h = host();
    let item = h.item(&"é".repeat(300));
    assert_eq!(item.title.chars().count(), MAX_TITLE);
}

#[test]
fn link_and_note_sort_without_a_new_item() {
    let h = host();
    let item = h.item("Renew the visa");
    let c = h.capture("client-2", "Passport photo too");
    let (linked, to) = h.sort(&c, SortAction::Link, Some(&item.id)).unwrap();
    let to = to.unwrap();
    assert_eq!(linked.work, vec![item.id.clone()]);
    assert_eq!(to.captures.len(), 2);
    assert_eq!(to.rev, item.rev + 1);

    let n = h.capture("client-3", "Just a thought");
    let (noted, none) = h.sort(&n, SortAction::Note, None).unwrap();
    assert_eq!((noted.state, none), (CaptureState::Sorted, None));
    assert!(noted.work.is_empty());
    assert_eq!(h.listed().len(), 1);
}

#[test]
fn a_sort_with_the_wrong_fields_is_refused_and_changes_nothing() {
    let h = host();
    let c = h.capture("client-1", "x");
    assert_eq!(
        code(h.sort(&c, SortAction::Link, None)),
        ErrorCode::BadRequest
    );
    assert_eq!(
        code(h.sort(&c, SortAction::Link, Some("w-00000000"))),
        ErrorCode::BadRequest
    );
    assert_eq!(
        code(h.sort(&c, SortAction::Note, Some("w-00000000"))),
        ErrorCode::BadRequest
    );
    let mut bad_title = Sort {
        capture: c.id.clone(),
        rev: c.rev,
        action: SortAction::Work,
        work: None,
        title: Some("SECRET-1\nline two".into()),
        workspace: None,
    };
    let e = sort(&h.config, bad_title).unwrap_err();
    assert!(!e.message.contains("SECRET-1"), "{}", e.message);
    bad_title = Sort {
        capture: c.id.clone(),
        rev: c.rev,
        action: SortAction::Work,
        work: None,
        title: None,
        workspace: Some("nowhere".into()),
    };
    assert_eq!(code(sort(&h.config, bad_title)), ErrorCode::BadRequest);
    let listed = crate::assistant::list(&h.config).unwrap();
    assert_eq!(listed.captures, vec![c]);
    assert!(listed.work.is_empty());
}

#[test]
fn later_moves_an_item_below_the_others_until_reopened() {
    let h = host();
    let a = h.item("first");
    let b = h.item("second");
    let a = h.act(&a, WorkAction::Later).unwrap();
    assert!(a.later_at.is_some());
    let ids: Vec<String> = h.listed().into_iter().map(|w| w.id).collect();
    assert_eq!(ids, [b.id.clone(), a.id.clone()]);
    // Later is not closing: reopen is for done and withdrawn items.
    assert_eq!(code(h.act(&a, WorkAction::Reopen)), ErrorCode::BadRequest);
    let done = h.act(&a, WorkAction::Complete).unwrap();
    assert_eq!(done.state, WorkState::Done);
    assert_eq!(code(h.act(&done, WorkAction::Later)), ErrorCode::BadRequest);
    let back = h.act(&done, WorkAction::Reopen).unwrap();
    assert_eq!((back.state, back.later_at), (WorkState::Open, None));
}

#[test]
fn a_stale_rev_is_a_conflict_and_writes_nothing() {
    let h = host();
    let item = h.item("Plan the trip");
    let edited = update(
        &h.config,
        Update {
            title: Some("Plan the Kyoto trip".into()),
            ..change(&item, WorkAction::Edit)
        },
    )
    .unwrap();
    assert_eq!(
        (edited.title.as_str(), edited.rev),
        ("Plan the Kyoto trip", 2)
    );
    assert_eq!(
        code(h.act(&item, WorkAction::Complete)),
        ErrorCode::Conflict
    );
    assert_eq!(h.listed()[0], edited);
}

#[test]
fn of_two_edits_at_once_one_wins() {
    let h = host();
    let item = h.item("Plan the trip");
    let results: Vec<Result<WorkItem, ErrorBody>> = std::thread::scope(|s| {
        let runs: Vec<_> = ["one", "two"]
            .into_iter()
            .map(|title| {
                let (config, item) = (&h.config, &item);
                s.spawn(move || {
                    update(
                        config,
                        Update {
                            title: Some(title.into()),
                            ..change(item, WorkAction::Edit)
                        },
                    )
                })
            })
            .collect();
        runs.into_iter().map(|r| r.join().unwrap()).collect()
    });
    let won: Vec<&WorkItem> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
    assert_eq!(won.len(), 1, "{results:?}");
    let lost = results.iter().find_map(|r| r.as_ref().err()).unwrap();
    assert_eq!(lost.code, ErrorCode::Conflict);
    assert_eq!(h.listed()[0].title, won[0].title);
}

#[test]
fn edit_and_the_other_actions_take_only_their_fields() {
    let h = host();
    let item = h.item("x");
    assert_eq!(code(h.act(&item, WorkAction::Edit)), ErrorCode::BadRequest);
    let with_title = Update {
        title: Some("y".into()),
        ..change(&item, WorkAction::Complete)
    };
    assert_eq!(code(update(&h.config, with_title)), ErrorCode::BadRequest);
    let other_workspace = Update {
        workspace: Some("nowhere".into()),
        ..change(&item, WorkAction::Edit)
    };
    assert_eq!(
        code(update(&h.config, other_workspace)),
        ErrorCode::BadRequest
    );
    let moved = update(
        &h.config,
        Update {
            workspace: Some("home".into()),
            ..change(&item, WorkAction::Edit)
        },
    )
    .unwrap();
    assert_eq!(moved.workspace.as_deref(), Some("home"));
    assert_eq!(
        code(h.act(&moved, WorkAction::Merge)),
        ErrorCode::BadRequest
    );
}

#[test]
fn merge_withdraws_the_item_and_keeps_both_texts() {
    let h = host();
    let from = h.item("Visa photos");
    let into = h.item("Renew the visa");
    let merged = update(
        &h.config,
        Update {
            into: Some(into.id.clone()),
            ..change(&from, WorkAction::Merge)
        },
    )
    .unwrap();
    assert_eq!(merged.state, WorkState::Withdrawn);
    assert_eq!(merged.merged_into.as_deref(), Some(into.id.as_str()));
    let listed = h.listed();
    let target = listed.iter().find(|w| w.id == into.id).unwrap();
    assert_eq!(target.captures.len(), 2);
    assert!(target.captures.contains(&from.captures[0]));
    let captures = super::super::shelf::<Capture>(&h.config)
        .unwrap()
        .all()
        .unwrap();
    let moved = captures.iter().find(|c| c.id == from.captures[0]).unwrap();
    assert_eq!(moved.work, vec![from.id.clone(), into.id.clone()]);
    assert_eq!(moved.text, "Visa photos");

    // Into itself, or into a closed item, is refused.
    let target = target.clone();
    let to_self = Update {
        into: Some(target.id.clone()),
        ..change(&target, WorkAction::Merge)
    };
    assert_eq!(code(update(&h.config, to_self)), ErrorCode::BadRequest);
    let other = h.item("Other");
    let to_closed = Update {
        into: Some(from.id.clone()),
        ..change(&other, WorkAction::Merge)
    };
    assert_eq!(code(update(&h.config, to_closed)), ErrorCode::BadRequest);
}

#[test]
fn a_linked_task_makes_an_item_in_progress_or_waiting_on_a_decision() {
    let h = host();
    let item = h.item("Ship the release");
    let store = Store::open(h.config.state_dir.as_ref().unwrap()).unwrap();
    let task = store
        .create(NewRecord {
            workspace: "work".into(),
            title: "Ship".into(),
            goal: "g".into(),
            ..NewRecord::default()
        })
        .unwrap();
    let works = super::super::shelf::<WorkItem>(&h.config).unwrap();
    works
        .update::<_, StateError>(&item.id, |w| {
            w.tasks.push(task.id.clone());
            Ok(())
        })
        .unwrap();
    assert_eq!(h.listed()[0].state, WorkState::InProgress);

    let items = ItemStore::open(h.config.state_dir.as_ref().unwrap()).unwrap();
    items
        .update::<_, StateError>(&task.id, |t| {
            t.items.push(StoredItem {
                item: tests_item("i-00000001", ItemState::Open),
                key: "review:1".into(),
            });
            Ok(())
        })
        .unwrap();
    assert_eq!(h.listed()[0].state, WorkState::WaitingDecision);

    store
        .update(&task.id, task.rev, |r| r.status = Status::Done)
        .unwrap();
    // A Task's end never completes the Work item.
    assert_eq!(h.listed()[0].state, WorkState::Open);
}

#[test]
fn get_work_returns_every_linked_capture_in_the_items_order() {
    use crate::assistant::detail::detail;
    let h = host();
    let first = h.capture("client-1", "Shorten onboarding\nsteps 3 to 5");
    let (_, item) = h.sort(&first, SortAction::Work, None).unwrap();
    let item = item.unwrap();
    let second = h.capture("client-2", "Also the FAQ");
    h.sort(&second, SortAction::Link, Some(&item.id)).unwrap();
    // An id whose file is gone is left out.
    let works = super::super::shelf::<WorkItem>(&h.config).unwrap();
    works
        .update::<_, StateError>(&item.id, |w| {
            w.captures.insert(1, "c-0000dead".into());
            Ok(())
        })
        .unwrap();

    let got = detail(&h.config, &item.id).unwrap();
    assert_eq!(got.entry.id, item.id);
    assert_eq!(got.entry.state, WorkState::Open);
    let texts: Vec<&str> = got.captures.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["Shorten onboarding\nsteps 3 to 5", "Also the FAQ"]);
    assert!(got.captures.iter().all(|c| c.state == CaptureState::Sorted));
    assert!(got.requests.is_empty());
    // inbox-list has none of them: both are sorted.
    assert!(
        crate::assistant::list(&h.config)
            .unwrap()
            .captures
            .is_empty()
    );

    for unknown in ["w-0000ffff", "w-zz", "../w-00000001"] {
        let e = detail(&h.config, unknown).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadRequest, "{unknown}");
        assert_eq!(e.message, format!("no Work item {unknown}"));
    }
}
