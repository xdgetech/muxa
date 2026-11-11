use std::path::PathBuf;

use muxa_protocol::{Minutes, PlanRow, WorkState};

use super::*;
use crate::assistant::plan::WORKSPACE_FIELD;
use crate::assistant::store::Extra;
use crate::routines::store::StateError;

pub(crate) const NOW: u64 = 1_762_740_000;
const DAY: u64 = 86_400;

pub(crate) struct Host {
    base: PathBuf,
    pub config: Config,
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

pub(crate) fn host() -> Host {
    let base = std::env::temp_dir().join(format!(
        "muxa-corrections-{}-{:08x}",
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
            "[assistant]\nenabled = true\n\
             [[workspace]]\nname = \"secretary\"\nroot = \"{0}\"\n[workspace.assistant]\noverall = true\n\
             [[workspace]]\nname = \"work\"\nroot = \"{0}\"\n",
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

pub(crate) fn row(id: &str) -> PlanRow {
    let (work, item) = if id.starts_with("w-") {
        (Some(id.to_owned()), None)
    } else {
        (None, Some(id.to_owned()))
    };
    PlanRow {
        work,
        item,
        why: "why".into(),
        next: "next".into(),
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

impl Host {
    /// A stored plan whose `top` lists `rows`.
    pub fn plan(&self, inputs_at: u64, delivery: Delivery, rows: &[&str]) -> Plan {
        super::super::plan::shelf(&self.config)
            .unwrap()
            .create_with_extra::<StateError>(|id| {
                let mut extra = Extra::new();
                extra.insert(WORKSPACE_FIELD.into(), "secretary".into());
                Ok((
                    Plan {
                        id,
                        rev: 2,
                        at: inputs_at,
                        inputs_at,
                        delivery,
                        delivery_at: inputs_at,
                        minutes: None,
                        run: None,
                        workspaces: Vec::new(),
                        top: rows.iter().map(|r| row(r)).collect(),
                        due_soon: Vec::new(),
                        unanswered: Vec::new(),
                        stale: Vec::new(),
                        changed: Vec::new(),
                        user_order: Vec::new(),
                        dismissed: Vec::new(),
                        notes: None,
                        reason: None,
                    },
                    extra,
                ))
            })
            .unwrap()
    }

    pub fn work(&self, workspace: Option<&str>) -> WorkItem {
        super::super::shelf::<WorkItem>(&self.config)
            .unwrap()
            .create::<StateError>(|id| {
                Ok(WorkItem {
                    id,
                    rev: 1,
                    title: "t".into(),
                    state: WorkState::Open,
                    created_at: 1,
                    updated_at: 1,
                    workspace: workspace.map(str::to_owned),
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

    pub fn corrections(&self) -> Vec<Correction> {
        live(&self.config, NOW).unwrap()
    }

    fn stored(&self, id: &str, at: u64) {
        let c = Correction {
            id: id.into(),
            kind: CorrectionKind::Pinned,
            at,
            plan: "p-00000001".into(),
            row: Some("w-00000001".into()),
            in_top: false,
            workspace: None,
            before: Vec::new(),
            after: Vec::new(),
            reason: None,
        };
        let dir = self
            .config
            .state_dir
            .as_ref()
            .unwrap()
            .join("assistant/corrections");
        std::fs::create_dir_all(&dir).unwrap();
        let mut file = serde_json::to_value(&c).unwrap();
        file["version"] = 1.into();
        std::fs::write(dir.join(format!("{id}.json")), file.to_string()).unwrap();
    }
}

fn item(id: &str, kind: &str) -> InboxItem {
    serde_json::from_value(serde_json::json!({
        "id": id, "kind": kind, "state": "open", "question": "q", "workspace": "secretary",
        "title": "t", "createdAt": 1,
    }))
    .unwrap()
}

#[test]
fn pin_unpin_and_later_each_record_one_correction_against_the_current_plan() {
    let h = host();
    let listed = h.work(Some("work"));
    let other = h.work(None);
    let plan = h.plan(100, Delivery::Resulted, &[&listed.id]);
    after_work(&h.config, &listed, WorkAction::Pin, true, NOW);
    after_work(&h.config, &other, WorkAction::Later, true, NOW + 1);
    after_work(&h.config, &listed, WorkAction::Unpin, true, NOW + 2);
    // A pin of a pinned item, and an edit, are no corrections.
    after_work(&h.config, &listed, WorkAction::Pin, false, NOW + 3);
    after_work(&h.config, &listed, WorkAction::Edit, true, NOW + 4);
    let got: Vec<_> = h
        .corrections()
        .into_iter()
        .map(|c| {
            (
                c.kind,
                c.row.unwrap(),
                c.in_top,
                c.workspace,
                c.plan,
                c.reason,
            )
        })
        .collect();
    let p = plan.id;
    assert_eq!(
        got,
        [
            (
                CorrectionKind::Unpinned,
                listed.id.clone(),
                true,
                Some("work".into()),
                p.clone(),
                None
            ),
            (
                CorrectionKind::Deferred,
                other.id.clone(),
                false,
                None,
                p.clone(),
                None
            ),
            (
                CorrectionKind::Pinned,
                listed.id.clone(),
                true,
                Some("work".into()),
                p,
                None
            ),
        ]
    );
}

#[test]
fn corrections_are_against_the_newest_resulted_plan_only() {
    let h = host();
    let w = h.work(None);
    h.plan(50, Delivery::Pending, &[]);
    after_work(&h.config, &w, WorkAction::Pin, true, NOW);
    assert!(h.corrections().is_empty(), "no plan is current");
    h.plan(100, Delivery::Resulted, &[]);
    let newest = h.plan(200, Delivery::Resulted, &[&w.id]);
    h.plan(300, Delivery::Processing, &[]);
    after_work(&h.config, &w, WorkAction::Pin, true, NOW);
    let got = h.corrections();
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].plan.as_str(), got[0].in_top),
        (newest.id.as_str(), true)
    );
}

#[test]
fn a_rejected_proposal_keeps_its_reason_and_later_counts_only_on_a_listed_item() {
    let h = host();
    let plan = h.plan(100, Delivery::Resulted, &["i-0000000a"]);
    let reason = Some("Sales still matters".to_owned());
    after_answer(
        &h.config,
        &item("i-0000000b", "sort"),
        InboxAction::Reject,
        reason.clone(),
        NOW,
    );
    // Reject on an agent's menu is no correction; neither is Later on an unlisted item.
    after_answer(
        &h.config,
        &item("i-0000000c", "ask"),
        InboxAction::Reject,
        None,
        NOW + 1,
    );
    after_answer(
        &h.config,
        &item("i-0000000d", "review"),
        InboxAction::Later,
        None,
        NOW + 2,
    );
    after_answer(
        &h.config,
        &item("i-0000000a", "review"),
        InboxAction::Later,
        None,
        NOW + 3,
    );
    let got = h.corrections();
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(
        (
            got[0].kind,
            got[0].row.as_deref(),
            got[0].in_top,
            got[0].reason.as_deref()
        ),
        (CorrectionKind::Deferred, Some("i-0000000a"), true, None),
        "deferred carries no reason unless given"
    );
    assert_eq!(
        (got[1].kind, got[1].in_top, got[1].reason.as_ref()),
        (CorrectionKind::Rejected, false, reason.as_ref())
    );
    assert_eq!(got[1].workspace.as_deref(), Some("secretary"));
    assert_eq!(got[1].plan, plan.id);
}

#[test]
fn a_reason_is_optional_short_and_without_control_characters() {
    assert_eq!(check_reason(None), Ok(None));
    assert_eq!(check_reason(Some("  ")), Ok(None));
    let max = "x".repeat(MAX_REASON);
    assert_eq!(check_reason(Some(&max)), Ok(Some(max.clone())));
    assert_eq!(check_reason(Some("a\nb\tc")), Ok(Some("a\nb\tc".into())));
    for bad in [
        "x".repeat(MAX_REASON + 1),
        "a\u{0}b".into(),
        "a\u{1b}[2J".into(),
    ] {
        let e = check_reason(Some(&bad)).unwrap_err();
        assert_eq!(e.code, ErrorCode::BadRequest);
        assert!(!e.message.contains(&bad));
    }
}

#[test]
fn two_hundred_are_kept_for_ninety_days_and_fifty_are_listed() {
    let h = host();
    for n in 0..205u64 {
        h.stored(&format!("x-{n:08x}"), NOW - n * 60);
    }
    h.stored("x-ffffff00", NOW - 91 * DAY);
    h.stored("x-ffffff01", NOW - 89 * DAY);
    let listed = list(&h.config, NOW).unwrap();
    assert_eq!(listed.len(), MAX_LISTED);
    assert_eq!(listed[0].id, "x-00000000", "newest first");
    prune(&h.config, NOW);
    let all = Shelf::<Correction>::open(h.config.state_dir.as_ref().unwrap())
        .unwrap()
        .all()
        .unwrap();
    assert_eq!(all.len(), MAX_KEPT);
    let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
    assert!(!ids.contains(&"x-ffffff00"), "past 90 days");
    assert!(!ids.contains(&"x-ffffff01"), "the oldest beyond 200");
    assert!(ids.contains(&"x-000000c7") && !ids.contains(&"x-000000c8"));
}

#[test]
fn a_rows_workspace_is_its_work_items() {
    let h = host();
    let w = h.work(Some("work"));
    assert_eq!(row_workspace(&h.config, &w.id).as_deref(), Some("work"));
    assert_eq!(row_workspace(&h.config, "w-0000ffff"), None);
    assert_eq!(row_workspace(&h.config, "i-0000ffff"), None);
}
