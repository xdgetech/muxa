use std::path::PathBuf;

use muxa_protocol::{AssistScope, WorkState};
use serde_json::json;

use super::*;
use crate::records::{LoopSpec, Status};

struct Host {
    base: PathBuf,
    config: Config,
    run_dir: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-assistant-check-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let run_dir = base.join("root/assistant/hosting-t-00000a");
    crate::assistant::record::prepare(&run_dir).unwrap();
    std::fs::create_dir_all(base.join("state")).unwrap();
    let config = Config {
        state_dir: Some(base.join("state")),
        workspaces: None,
        home: None,
    };
    Host {
        base,
        config,
        run_dir,
    }
}

impl Host {
    fn state(&self) -> &Path {
        self.config.state_dir.as_deref().unwrap()
    }

    fn requests(&self) -> Shelf<AssistantRequest> {
        Shelf::open(self.state()).unwrap()
    }

    fn work(&self) -> WorkItem {
        Shelf::<WorkItem>::open(self.state())
            .unwrap()
            .create::<StateError>(|id| {
                Ok(WorkItem {
                    id,
                    rev: 3,
                    title: "Hosting".into(),
                    state: WorkState::Open,
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

    fn request(&self, work: &str, version: u64) -> AssistantRequest {
        self.requests()
            .create::<StateError>(|id| {
                Ok(AssistantRequest {
                    id,
                    work: work.into(),
                    version,
                    workspace: "w".into(),
                    scope: AssistScope::Research,
                    delivery: Delivery::Processing,
                    delivery_at: 1,
                    created_at: 1,
                    updated_at: 1,
                    purpose: None,
                    run: Some("t-00000a".into()),
                    results: Vec::new(),
                })
            })
            .unwrap()
    }

    fn record(&self, run: &RunFor) -> Record {
        Record {
            version: 1,
            id: "t-00000a".into(),
            rev: 1,
            workspace: "w".into(),
            title: "Hosting".into(),
            goal: "g".into(),
            status: Status::Open,
            created_at: 1,
            updated_at: 1,
            project: None,
            repo: None,
            branch: None,
            dir: Some(self.run_dir.clone()),
            session: None,
            note: None,
            runs: Vec::new(),
            extra: run.fields(),
        }
    }

    fn write(&self, value: &serde_json::Value) {
        std::fs::write(
            self.run_dir.join(result::RESULT),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }

    fn log(&self, attempt: u32) -> String {
        std::fs::read_to_string(crate::loops::check::log_path(&self.run_dir, attempt)).unwrap()
    }

    fn get(&self, id: &str) -> AssistantRequest {
        self.requests().get(id).unwrap().unwrap()
    }
}

fn good(run: &RunFor, duplicate: Option<&str>) -> serde_json::Value {
    let mut v = json!({
        "version": 1,
        "request": run.request,
        "requestVersion": run.version,
        "summary": "Plan A fits.",
        "findings": [{"text": "A is cheaper.", "sources": [{"url": "https://example.com/a"}]}]
    });
    if let Some(work) = duplicate {
        v["possibleDuplicates"] = json!([{"work": work, "reason": "Same vendor."}]);
    }
    v
}

fn exit(end: CheckEnd) -> i32 {
    match end {
        CheckEnd::Exited { code, .. } => code,
        CheckEnd::TimedOut { .. } => panic!("an in-process check never times out"),
    }
}

fn run_for(work: &WorkItem, q: &AssistantRequest) -> RunFor {
    RunFor {
        request: q.id.clone(),
        version: q.version,
        work: work.id.clone(),
    }
}

#[test]
fn a_good_result_is_stored_once_and_the_work_item_stays() {
    let h = host();
    let work = h.work();
    let other = h.work();
    let q = h.request(&work.id, 1);
    let run = run_for(&work, &q);
    let record = h.record(&run);
    h.write(&good(&run, Some(&other.id)));
    assert_eq!(exit(super::run(&h.config, &record, 1)), 0);
    assert!(h.log(1).starts_with("passed"));
    let report = std::fs::read_to_string(h.run_dir.join(".muxa/CHECK.md")).unwrap();
    assert!(report.contains("Check: the assistant result"), "{report}");
    let stored = h.get(&q.id);
    assert_eq!(stored.delivery, Delivery::Resulted);
    assert_eq!(stored.results.len(), 1);
    assert_eq!(stored.results[0].possible_duplicates[0].work, other.id);

    assert_eq!(exit(super::run(&h.config, &record, 2)), 0);
    assert!(h.log(2).contains("stored before"));
    assert_eq!(h.get(&q.id), stored);
    let after = Shelf::<WorkItem>::open(h.state())
        .unwrap()
        .get(&work.id)
        .unwrap();
    assert_eq!(after, Some(work));
}

#[test]
fn an_invalid_result_or_an_unknown_duplicate_is_refused_and_not_stored() {
    let h = host();
    let work = h.work();
    let q = h.request(&work.id, 1);
    let run = run_for(&work, &q);
    let record = h.record(&run);
    assert_eq!(exit(super::run(&h.config, &record, 1)), 1);
    assert!(h.log(1).contains("does not exist"));
    let mut wrong = good(&run, None);
    wrong["requestVersion"] = json!(9);
    h.write(&wrong);
    assert_eq!(exit(super::run(&h.config, &record, 2)), 1);
    assert!(h.log(2).contains("requestVersion must be 1"));
    h.write(&good(&run, Some("w-0badbad0")));
    assert_eq!(exit(super::run(&h.config, &record, 3)), 1);
    assert!(
        h.log(3)
            .contains("possibleDuplicates[0].work is not a Work item")
    );
    let report = std::fs::read_to_string(h.run_dir.join(".muxa/CHECK.md")).unwrap();
    assert!(report.contains("Result: failed"));
    assert_eq!(h.get(&q.id), q);
}

#[test]
fn a_run_of_an_older_version_is_not_stored() {
    let h = host();
    let work = h.work();
    let q = h.request(&work.id, 1);
    let run = run_for(&work, &q);
    h.write(&good(&run, None));
    h.requests()
        .update::<_, StateError>(&q.id, |q| {
            q.version = 2;
            Ok(())
        })
        .unwrap();
    assert_eq!(exit(super::run(&h.config, &h.record(&run), 1)), 1);
    assert!(h.log(1).contains("no longer the request's current version"));
    assert!(h.get(&q.id).results.is_empty());
}

fn stopped_loop(reason: Reason) -> Loop {
    let spec = LoopSpec {
        check: crate::assistant::record::CHECK.into(),
        max_attempts: Some(3),
        timeout: Some(60),
    };
    Loop::start(spec, None, false, 1).stopped(reason, 2)
}

#[test]
fn a_loop_stopped_after_its_attempts_fails_the_request() {
    let h = host();
    let work = h.work();
    let q = h.request(&work.id, 1);
    let run = run_for(&work, &q);
    let record = h.record(&run);
    stopped(h.state(), &record, &stopped_loop(Reason::User));
    stopped(h.state(), &record, &stopped_loop(Reason::Status));
    assert_eq!(h.get(&q.id).delivery, Delivery::Processing);
    stopped(h.state(), &record, &stopped_loop(Reason::Limit));
    let failed = h.get(&q.id);
    assert_eq!(failed.delivery, Delivery::Failed);
    assert!(failed.delivery_at >= q.delivery_at);
}

#[test]
fn fail_leaves_a_moved_on_resulted_or_cancelled_request_alone() {
    let h = host();
    let work = h.work();
    let shelf = h.requests();
    let set = |id: &str, f: &dyn Fn(&mut AssistantRequest)| {
        shelf
            .update::<_, StateError>(id, |q| {
                f(q);
                Ok(())
            })
            .unwrap();
    };
    let newer = h.request(&work.id, 2);
    let mut run = run_for(&work, &newer);
    run.version = 1;
    assert!(!fail(&shelf, &run, 5).unwrap());

    let resulted = h.request(&work.id, 1);
    let run = run_for(&work, &resulted);
    h.write(&good(&run, None));
    let found = result::read(&h.run_dir, &run).unwrap();
    set(&resulted.id, &|q| {
        result::store(q, found.clone(), 3).unwrap();
        q.delivery = Delivery::Processing;
    });
    assert!(!fail(&shelf, &run, 5).unwrap());

    let cancelled = h.request(&work.id, 1);
    set(&cancelled.id, &|q| q.delivery = Delivery::CancelRequested);
    assert!(!fail(&shelf, &run_for(&work, &cancelled), 5).unwrap());
    assert_eq!(h.get(&cancelled.id).delivery, Delivery::CancelRequested);
}
