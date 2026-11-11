use std::path::PathBuf;

use muxa_protocol::{ErrorCode, Minutes, PlanRow, TargetState, WorkState};

use super::*;
use crate::assistant::plan::WORKSPACE_FIELD;
use crate::assistant::store::Extra;
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

fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-refill-{}-{:08x}",
        std::process::id(),
        getrandom::u32().unwrap()
    ));
    std::fs::create_dir_all(base.join("overall")).unwrap();
    std::fs::create_dir_all(base.join("state")).unwrap();
    let file = base.join("workspaces.toml");
    std::fs::write(
        &file,
        format!(
            "[assistant]\nenabled = true\n[[workspace]]\nname = \"Overall\"\nroot = \"{}\"\n\
             [workspace.assistant]\noverall = true\n",
            base.join("overall").canonicalize().unwrap().display()
        ),
    )
    .unwrap();
    Host {
        config: Config {
            state_dir: Some(base.join("state")),
            workspaces: Some(file),
            home: None,
        },
        base,
    }
}

fn row(id: &str) -> PlanRow {
    PlanRow {
        work: id.starts_with("w-").then(|| id.into()),
        item: id.starts_with("i-").then(|| id.into()),
        why: "w".into(),
        next: "n".into(),
        minutes: Minutes {
            low: 1,
            high: 2,
            estimated: 1,
        },
        basis: Vec::new(),
        missing: Vec::new(),
        title: None,
        target_state: None,
    }
}

fn plan(top: Vec<PlanRow>, dismissed: &[&str]) -> Plan {
    Plan {
        id: "p-00000001".into(),
        rev: 1,
        at: 100,
        inputs_at: 100,
        delivery: Delivery::Resulted,
        delivery_at: 100,
        minutes: None,
        run: None,
        workspaces: Vec::new(),
        top,
        due_soon: Vec::new(),
        unanswered: Vec::new(),
        stale: Vec::new(),
        changed: Vec::new(),
        user_order: Vec::new(),
        dismissed: dismissed.iter().map(|d| (*d).to_owned()).collect(),
        notes: None,
        reason: None,
    }
}

impl Host {
    fn store(&self, p: Plan) -> Plan {
        super::super::shelf(&self.config)
            .unwrap()
            .create_with_extra::<StateError>(|id| {
                let mut extra = Extra::new();
                extra.insert(WORKSPACE_FIELD.into(), "Overall".into());
                Ok((Plan { id, ..p }, extra))
            })
            .unwrap()
    }

    fn open_work(&self) -> WorkItem {
        crate::assistant::shelf::<WorkItem>(&self.config)
            .unwrap()
            .create_with_extra::<StateError>(|id| {
                let w = WorkItem {
                    id,
                    rev: 1,
                    title: "t".into(),
                    state: WorkState::Open,
                    created_at: 1,
                    updated_at: 1,
                    workspace: None,
                    later_at: None,
                    merged_into: None,
                    pinned_at: None,
                    due: None,
                    captures: Vec::new(),
                    requests: Vec::new(),
                    tasks: Vec::new(),
                };
                Ok((w, Extra::new()))
            })
            .unwrap()
    }
}

fn closed(id: &str) -> PlanRow {
    PlanRow {
        target_state: Some(TargetState::Done),
        ..row(id)
    }
}

#[test]
fn a_plan_is_emptied_when_each_row_is_closed_or_not_today() {
    assert!(emptied(&plan(
        vec![closed("w-00000001"), closed("i-00000002")],
        &[]
    )));
    assert!(emptied(&plan(
        vec![closed("w-00000001"), row("i-00000002")],
        &["i-00000002"]
    )));
    assert!(!emptied(&plan(
        vec![closed("w-00000001"), row("i-00000002")],
        &[]
    )));
    // A plan with nothing to rank never refills.
    assert!(!emptied(&plan(vec![], &[])));
}

#[test]
fn the_newest_emptied_plan_is_due_once() {
    let h = host();
    // Neither target is on the host any more: both rows are closed (gone).
    let p = h.store(plan(vec![row("w-0000000a"), row("i-0000000b")], &[]));
    assert_eq!(due(&h.config, 200), Some(p.id.clone()));
    let shelf = super::super::shelf(&h.config).unwrap();
    super::super::start::mark_refilled(&shelf, &p.id, 200).unwrap();
    assert_eq!(due(&h.config, 201), None);
    let again = super::super::start::mark_refilled(&shelf, &p.id, 202).unwrap_err();
    assert_eq!(again.code, ErrorCode::Conflict);
}

#[test]
fn an_open_row_or_an_empty_plan_is_not_due() {
    let h = host();
    let w = h.open_work();
    h.store(plan(vec![row(&w.id), row("i-0000000b")], &[]));
    assert_eq!(due(&h.config, 200), None);
    // A newer plan with nothing to rank hides the older one.
    let h = host();
    h.store(plan(vec![row("w-0000000a")], &[]));
    h.store(Plan {
        inputs_at: 300,
        ..plan(vec![], &[])
    });
    assert_eq!(due(&h.config, 400), None);
}

#[test]
fn a_dismissed_open_row_counts_as_done() {
    let h = host();
    let w = h.open_work();
    let p = h.store(plan(vec![row(&w.id)], &[w.id.as_str()]));
    assert_eq!(due(&h.config, 200), Some(p.id));
}
