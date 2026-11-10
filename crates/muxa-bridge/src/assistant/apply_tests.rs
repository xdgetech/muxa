use std::path::{Path, PathBuf};

use muxa_protocol::{AssistScope, Delivery, WorkState};
use serde_json::json;

use super::*;
use crate::assistant::record::RequestRun;
use crate::assistant::store::Shelf;
use crate::loops::step::CheckEnd;
use crate::records::Status;
use crate::routines::store::StateError;

const SENTINEL: &str = "notes-mark-8d2e";

struct Host {
    base: PathBuf,
    root: PathBuf,
    config: Config,
    run_dir: PathBuf,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Workspace `w` delegates with `notes = mode`.
fn host(mode: &str) -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-assistant-apply-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    let root = base.join("root");
    std::fs::create_dir_all(base.join("state")).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let workspaces = base.join("workspaces.toml");
    std::fs::write(
        &workspaces,
        format!(
            "[assistant]\nenabled = true\n[[workspace]]\nname = \"w\"\nroot = \"{}\"\n\
             [workspace.assistant]\ndelegate = true\nnotes = \"{mode}\"\n",
            root.display()
        ),
    )
    .unwrap();
    let run_dir = root.join("assistant/research-t-00000a");
    crate::assistant::record::prepare(&run_dir).unwrap();
    Host {
        config: Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(workspaces),
            home: None,
        },
        base,
        root,
        run_dir,
    }
}

impl Host {
    fn state(&self) -> &Path {
        self.config.state_dir.as_deref().unwrap()
    }

    fn live(&self) -> String {
        notes::read(&self.root).unwrap().text
    }

    fn set_live(&self, text: &str) {
        std::fs::create_dir_all(self.root.join(".muxa/assistant")).unwrap();
        std::fs::write(self.root.join(notes::NOTES), text).unwrap();
    }

    /// [`Host::asked`], then the check of its result.
    fn published(&self, proposal: Option<&str>) -> (AssistantRequest, i32) {
        let (q, record) = self.asked(proposal);
        self.check(&q, &record)
    }

    /// A request whose run read the live notes, and its result with `notes`.
    fn asked(&self, proposal: Option<&str>) -> (AssistantRequest, Record) {
        let work = Shelf::<WorkItem>::open(self.state())
            .unwrap()
            .create::<StateError>(|id| {
                Ok(WorkItem {
                    id,
                    rev: 1,
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
            .unwrap();
        let requests = Shelf::<AssistantRequest>::open(self.state()).unwrap();
        let q = requests
            .create::<StateError>(|id| {
                Ok(AssistantRequest {
                    steps: Vec::new(),
                    id,
                    work: work.id.clone(),
                    version: 1,
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
            .unwrap();
        Shelf::<WorkItem>::open(self.state())
            .unwrap()
            .update::<_, StateError>(&work.id, |w| {
                w.requests.push(q.id.clone());
                Ok(())
            })
            .unwrap();
        let run = RequestRun {
            request: q.id.clone(),
            version: 1,
            work: work.id.clone(),
            notes_base: Some(notes::read(&self.root).unwrap().base),
            step: None,
            member: None,
            shared_base: None,
        };
        let mut result = json!({"version": 1, "request": q.id, "requestVersion": 1,
                                "summary": "Plan A fits."});
        if let Some(text) = proposal {
            result["notes"] = json!(text);
        }
        std::fs::write(
            self.run_dir.join(crate::assistant::result::RESULT),
            result.to_string(),
        )
        .unwrap();
        let record = Record {
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
        };
        (q, record)
    }

    fn check(&self, q: &AssistantRequest, record: &Record) -> (AssistantRequest, i32) {
        let code = match crate::assistant::check::run(&self.config, record, 1) {
            CheckEnd::Exited { code, .. } => code,
            CheckEnd::TimedOut { .. } => panic!("an in-process check never times out"),
        };
        let requests = Shelf::<AssistantRequest>::open(self.state()).unwrap();
        (requests.get(&q.id).unwrap().unwrap(), code)
    }
}

#[test]
fn review_stores_the_proposal_and_apply_writes_it_while_the_base_holds() {
    let h = host("review");
    h.set_live("# Old\n");
    let text = format!("# New\n- {SENTINEL}\n");
    let (q, code) = h.published(Some(&text));
    assert_eq!(code, 0);
    let notes = q.results[0].notes.clone().unwrap();
    assert_eq!(notes.text, text);
    assert_eq!(notes.base, notes::base_of(b"# Old\n"));
    assert_eq!(h.live(), "# Old\n", "review never writes at publish");

    let wrong_base = apply_notes(&h.config, &q.id, 1, &"0".repeat(64), None).unwrap_err();
    assert_eq!(wrong_base.code, ErrorCode::BadRequest);
    let no_version = apply_notes(&h.config, &q.id, 2, &notes.base, None).unwrap_err();
    assert_eq!(no_version.code, ErrorCode::BadRequest);
    let plan = apply_notes(&h.config, "p-00000001", 1, &notes.base, None).unwrap_err();
    assert_eq!(plan.code, ErrorCode::BadRequest, "no such plan");

    let Applied::Request(item, request) =
        apply_notes(&h.config, &q.id, 1, &notes.base, None).unwrap()
    else {
        panic!("a request's notes reply with the request")
    };
    assert_eq!(
        (item.id.as_str(), request.id.as_str()),
        (q.work.as_str(), q.id.as_str())
    );
    assert_eq!(h.live(), text);
    // Applied once, the live file is no longer the proposal's base.
    let again = apply_notes(&h.config, &q.id, 1, &notes.base, None).unwrap_err();
    assert_eq!(again.code, ErrorCode::Conflict);
    assert!(!again.message.contains(SENTINEL), "{}", again.message);
}

#[test]
fn a_changed_base_is_a_conflict_and_keeps_the_users_edit() {
    let h = host("review");
    let (q, _) = h.published(Some("# Proposed\n"));
    h.set_live("# Edited by the user\n");
    let base = q.results[0].notes.as_ref().unwrap().base.clone();
    let e = apply_notes(&h.config, &q.id, 1, &base, None).unwrap_err();
    assert_eq!(e.code, ErrorCode::Conflict);
    assert!(e.message.contains("ask again"), "{}", e.message);
    assert_eq!(h.live(), "# Edited by the user\n");
}

#[test]
fn auto_applies_at_publish_unless_the_base_changed() {
    let h = host("auto");
    let (q, code) = h.published(Some("# Auto\n"));
    assert_eq!(code, 0);
    assert_eq!(h.live(), "# Auto\n");
    assert!(
        q.results[0].notes.is_some(),
        "the result keeps its notes as history"
    );

    // The user edits the file while the run works: the proposal waits.
    let h = host("auto");
    h.set_live("# Before\n");
    let (q, record) = h.asked(Some("# Late\n"));
    h.set_live("# Edited\n");
    let (q, code) = h.check(&q, &record);
    assert_eq!(code, 0);
    assert_eq!(h.live(), "# Edited\n");
    assert_eq!(q.results[0].notes.as_ref().unwrap().text, "# Late\n");
}

#[test]
fn the_copys_own_text_proposes_nothing_and_oversized_notes_fail_the_check() {
    let h = host("review");
    h.set_live("# Same\n");
    let (q, code) = h.published(Some("# Same\n"));
    assert_eq!(code, 0);
    assert_eq!(q.results[0].notes, None);

    let h = host("review");
    let big = format!("{SENTINEL}{}", "x".repeat(notes::MAX_NOTES));
    let (q, code) = h.published(Some(&big));
    assert_eq!(code, 1);
    assert!(q.results.is_empty());
    let log = std::fs::read_to_string(crate::loops::check::log_path(&h.run_dir, 1)).unwrap();
    assert!(log.contains("notes must be at most 16 KiB"), "{log}");
    assert!(!log.contains(SENTINEL), "{log}");
}
