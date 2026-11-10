use std::path::PathBuf;

use muxa_protocol::WorkState;

use super::*;
use crate::assistant::record::RequestRun;
use crate::routines::store::StateError;

struct Host {
    base: PathBuf,
    config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host(delegate: &str) -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-assistant-ask-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::fs::create_dir_all(base.join("root")).unwrap();
    let base = base.canonicalize().unwrap();
    std::fs::write(
        base.join("workspaces.toml"),
        format!(
            "[assistant]\nenabled = true\n[[workspace]]\nname = \"w\"\nroot = \"{}\"\n\
             [workspace.assistant]\ndelegate = true\n{delegate}\n",
            base.join("root").display()
        ),
    )
    .unwrap();
    let config = Config {
        state_dir: Some(base.join("state")),
        workspaces: Some(base.join("workspaces.toml")),
        home: None,
    };
    Host { base, config }
}

impl Host {
    fn delegated(&self) -> Delegated {
        policy::delegated(&self.config.load_workspaces().unwrap(), "w", None, "claude").unwrap()
    }

    fn work(&self, state: WorkState) -> WorkItem {
        super::super::shelf::<WorkItem>(&self.config)
            .unwrap()
            .create::<StateError>(|id| {
                Ok(WorkItem {
                    id,
                    rev: 1,
                    title: "Hosting".into(),
                    state,
                    created_at: 1,
                    updated_at: 1,
                    workspace: Some("w".into()),
                    later_at: None,
                    merged_into: None,
                    pinned_at: None,
                    due: None,
                    captures: Vec::new(),
                    requests: Vec::new(),
                    tasks: Vec::new(),
                })
            })
            .unwrap()
    }

    fn get_work(&self, id: &str) -> WorkItem {
        super::super::shelf::<WorkItem>(&self.config)
            .unwrap()
            .get(id)
            .unwrap()
            .unwrap()
    }

    fn get(&self, id: &str) -> AssistantRequest {
        super::super::shelf::<AssistantRequest>(&self.config)
            .unwrap()
            .get(id)
            .unwrap()
            .unwrap()
    }

    fn requests(&self) -> usize {
        super::super::shelf::<AssistantRequest>(&self.config)
            .unwrap()
            .all()
            .unwrap()
            .len()
    }

    /// A run record of `request`'s `version`, as `create()` leaves it.
    fn record(&self, id: &str, request: &str, version: u64, kind: bool, status: &str) {
        let mut r: Record = serde_json::from_value(serde_json::json!({
            "version": 1, "id": id, "rev": 1, "workspace": "w", "title": "Hosting",
            "goal": format!("Brief.\n\n{}", crate::assistant::steps::marker(request, version, 1)),
            "status": status, "createdAt": crate::records::now(), "updatedAt": 1,
        }))
        .unwrap();
        if kind {
            r.extra.extend(
                RequestRun {
                    request: request.into(),
                    version,
                    work: "w-00000000".into(),
                    notes_base: None,
                    step: None,
                    member: None,
                    shared_base: None,
                }
                .fields(),
            );
        }
        let path = self.base.join(format!("state/{id}.json"));
        std::fs::write(path, serde_json::to_vec(&r).unwrap()).unwrap();
    }
}

fn ask_for(work: &WorkItem, rev: u64) -> Ask {
    Ask {
        work: work.id.clone(),
        rev,
        scope: AssistScope::Research,
        workspace: None,
        purpose: Some("Find a host".into()),
        worker: None,
        asker: muxa_protocol::StepBy::User,
    }
}

fn claimed(step: Step) -> AssistantRequest {
    match step {
        Step::Claimed(q) => *q,
        Step::Known(id, v) => panic!("known {id} v{v}"),
    }
}

#[test]
fn one_rev_makes_one_version_and_a_resend_returns_it() {
    let h = host("max_open_runs = 3");
    let d = h.delegated();
    let w = h.work(WorkState::Open);
    let now = crate::records::now();
    let q = claimed(begin(&h.config, &ask_for(&w, 1), &d, now).unwrap());
    assert_eq!(
        (q.version, q.delivery, q.run.as_deref()),
        (1, Delivery::Pending, None)
    );
    let item = h.get_work(&w.id);
    assert_eq!((item.rev, item.requests.clone()), (2, vec![q.id.clone()]));

    // A resend of rev 1 finds the version; its run is still being made.
    let Step::Known(id, 1) = begin(&h.config, &ask_for(&w, 1), &d, now).unwrap() else {
        panic!("not known")
    };
    assert_eq!(id, q.id);
    let busy = claim_run(&h.config, &d, &id, 1, now).unwrap_err();
    assert_eq!(busy.code, ErrorCode::Conflict);
    finish(&h.config, &id, 1, Some("t-00000a".into()), None).unwrap();
    assert_eq!(claim_run(&h.config, &d, &id, 1, now).unwrap(), None);
    assert_eq!(h.get(&id).run.as_deref(), Some("t-00000a"));
    assert_eq!((h.requests(), h.get_work(&w.id).rev), (1, 2));

    // The item's next rev is a new version of the same request.
    let v2 = claimed(begin(&h.config, &ask_for(&w, 2), &d, now).unwrap());
    assert_eq!(
        (v2.id.as_str(), v2.version, v2.run.as_deref()),
        (id.as_str(), 2, None)
    );
    assert_eq!(h.get_work(&w.id).rev, 3);
    let stale = begin(&h.config, &ask_for(&w, 1), &d, now).err().unwrap();
    assert_eq!(stale.code, ErrorCode::Conflict);
    assert_eq!(h.requests(), 1);
}

#[test]
fn a_crash_after_the_create_finds_the_run_by_its_marker_only() {
    let h = host("");
    let d = h.delegated();
    let w = h.work(WorkState::Open);
    let now = crate::records::now();
    let q = claimed(begin(&h.config, &ask_for(&w, 1), &d, now).unwrap());
    // A Task whose goal ends with the line is not the run.
    h.record("t-0000b1", &q.id, 1, false, "open");
    let later = now + CREATING_LIMIT + 1;
    h.record("t-0000b2", &q.id, 1, true, "open");
    assert_eq!(claim_run(&h.config, &d, &q.id, 1, later).unwrap(), None);
    assert_eq!(h.get(&q.id).run.as_deref(), Some("t-0000b2"));
}

#[test]
fn a_stale_claim_without_a_run_is_claimed_again() {
    let h = host("");
    let d = h.delegated();
    let w = h.work(WorkState::Open);
    let now = crate::records::now();
    let q = claimed(begin(&h.config, &ask_for(&w, 1), &d, now).unwrap());
    let later = now + CREATING_LIMIT + 1;
    let again = claim_run(&h.config, &d, &q.id, 1, later).unwrap().unwrap();
    assert_eq!((again.id, again.version), (q.id, 1));
}

#[test]
fn a_lost_link_and_rev_bump_are_repaired_by_the_resend() {
    let h = host("");
    let d = h.delegated();
    let w = h.work(WorkState::Open);
    let now = crate::records::now();
    let q = claimed(begin(&h.config, &ask_for(&w, 1), &d, now).unwrap());
    super::super::shelf::<WorkItem>(&h.config)
        .unwrap()
        .update::<_, StateError>(&w.id, |w| {
            w.requests.clear();
            w.rev = 1;
            Ok(())
        })
        .unwrap();
    assert!(matches!(
        begin(&h.config, &ask_for(&w, 1), &d, now).unwrap(),
        Step::Known(ref id, 1) if *id == q.id
    ));
    let item = h.get_work(&w.id);
    assert_eq!((item.rev, item.requests), (2, vec![q.id]));
}

#[test]
fn a_budget_or_a_closed_item_refuses_and_writes_nothing() {
    let h = host("max_open_runs = 1");
    let d = h.delegated();
    let w = h.work(WorkState::Open);
    h.record("t-0000c1", "q-0000000f", 1, true, "open");
    let full = begin(&h.config, &ask_for(&w, 1), &d, crate::records::now())
        .err()
        .unwrap();
    assert_eq!(full.code, ErrorCode::Conflict);
    assert!(full.message.contains("max_open_runs"), "{}", full.message);
    let done = h.work(WorkState::Done);
    let closed = begin(&h.config, &ask_for(&done, 1), &d, crate::records::now())
        .err()
        .unwrap();
    assert_eq!(closed.code, ErrorCode::BadRequest);
    assert_eq!(h.requests(), 0);
    assert_eq!(h.get_work(&w.id).rev, 1);
}

#[test]
fn finish_without_a_run_fails_the_version_and_ends_the_claim() {
    let h = host("");
    let d = h.delegated();
    let w = h.work(WorkState::Open);
    let now = crate::records::now();
    let q = claimed(begin(&h.config, &ask_for(&w, 1), &d, now).unwrap());
    finish(&h.config, &q.id, 1, None, Some("cannot reach tmux".into())).unwrap();
    let q = h.get(&q.id);
    assert_eq!((q.delivery, q.run), (Delivery::Failed, None));
    assert_eq!(claim_run(&h.config, &d, &q.id, 1, now).unwrap(), None);
}

#[test]
fn a_purpose_over_4_kib_is_refused() {
    assert!(check_purpose(Some(&"x".repeat(4096))).is_ok());
    assert_eq!(
        check_purpose(Some(&"x".repeat(4097))).unwrap_err().code,
        ErrorCode::BadRequest
    );
}
