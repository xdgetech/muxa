use muxa_protocol::{AssistResult, AssistScope, Due, Minutes};

use super::*;

fn work(id: &str, updated: u64) -> WorkItem {
    WorkItem {
        id: id.into(),
        rev: 1,
        title: "t".into(),
        state: WorkState::Open,
        created_at: 1,
        updated_at: updated,
        workspace: Some("w".into()),
        later_at: None,
        merged_into: None,
        pinned_at: None,
        due: None,
        captures: Vec::new(),
        requests: Vec::new(),
        tasks: Vec::new(),
    }
}

fn result(version: u64, at: u64) -> AssistResult {
    AssistResult {
        worker: None,
        step: None,
        version,
        at,
        summary: "s".into(),
        stale: false,
        findings: Vec::new(),
        open_questions: Vec::new(),
        next: Vec::new(),
        files: Vec::new(),
        possible_duplicates: Vec::new(),
        notes: None,
    }
}

fn request(id: &str, work: &str, version: u64, results: Vec<AssistResult>) -> AssistantRequest {
    AssistantRequest {
        steps: Vec::new(),
        id: id.into(),
        work: work.into(),
        version,
        workspace: "w".into(),
        scope: AssistScope::Research,
        delivery: Delivery::Processing,
        delivery_at: 1,
        created_at: 1,
        updated_at: 1,
        purpose: None,
        run: None,
        results,
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
    }
}

fn plan(id: &str, delivery: Delivery, rows: &[&str]) -> Plan {
    Plan {
        id: id.into(),
        rev: 1,
        at: 1,
        inputs_at: 1,
        delivery,
        delivery_at: 1,
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
    }
}

fn change(id: &str, change: ChangeKind) -> PlanChange {
    PlanChange {
        id: id.into(),
        change,
    }
}

#[test]
fn changed_lists_entered_and_moved_rows_then_the_ones_that_left() {
    let before = plan("p-00000001", Delivery::Resulted, &["w-a", "w-b", "i-c"]);
    let after = plan("p-00000002", Delivery::Resulted, &["w-b", "w-a", "w-d"]);
    assert_eq!(
        changed(&after, Some(&before)),
        [
            change("w-b", ChangeKind::Moved),
            change("w-a", ChangeKind::Moved),
            change("w-d", ChangeKind::Entered),
            change("i-c", ChangeKind::Left),
        ]
    );
    let same = plan("p-00000003", Delivery::Resulted, &["w-a", "w-b"]);
    let kept = plan("p-00000004", Delivery::Resulted, &["w-a", "w-b"]);
    assert!(changed(&kept, Some(&same)).is_empty());
    assert_eq!(
        changed(&kept, None),
        [
            change("w-a", ChangeKind::Entered),
            change("w-b", ChangeKind::Entered)
        ]
    );
}

#[test]
fn unanswered_and_stale_come_from_results_and_the_work_items_last_change() {
    let mut closed = work("w-00000004", 1);
    closed.state = WorkState::Done;
    let work = [work("w-00000001", 100), work("w-00000002", 100), closed];
    let mut cancelled = request("q-00000005", "w-00000001", 1, Vec::new());
    cancelled.delivery = Delivery::Cancelled;
    let requests = [
        // Version 2 asked; only version 1 answered, and before the item changed.
        request("q-00000001", "w-00000001", 2, vec![result(1, 50)]),
        // Answered after the item's last change.
        request("q-00000002", "w-00000002", 1, vec![result(1, 150)]),
        // Never answered.
        request("q-00000003", "w-00000002", 1, Vec::new()),
        // Its Work item is closed: nobody waits for it.
        request("q-00000004", "w-00000004", 1, Vec::new()),
        cancelled,
    ];
    let (unanswered, stale) = unanswered_and_stale(&work, &requests);
    assert_eq!(unanswered, ["q-00000001", "q-00000003"]);
    assert_eq!(stale, ["q-00000001"]);
}

#[test]
fn due_soon_lists_later_items_due_within_a_day_soonest_first() {
    let due = |id: &str, later: Option<u64>, at: u64| WorkItem {
        later_at: later,
        due: Some(Due {
            at,
            timezone: "UTC".into(),
        }),
        ..work(id, 1)
    };
    let now = 1_000_000;
    let mut done = due("w-00000005", Some(1), now + 10);
    done.state = WorkState::Withdrawn;
    let items = [
        due("w-00000001", Some(1), now + 20 * 3600),
        due("w-00000002", Some(1), now + 25 * 3600),
        due("w-00000003", None, now + 3600),
        due("w-00000004", Some(1), now - 3600),
        done,
    ];
    assert_eq!(due_soon(&items, now), ["w-00000004", "w-00000001"]);
}

#[test]
fn marks_go_on_resulted_plans_and_now_only_on_the_newest() {
    let mut plans = [
        plan("p-00000004", Delivery::Pending, &[]),
        plan("p-00000003", Delivery::Resulted, &["w-00000001"]),
        plan("p-00000002", Delivery::Failed, &[]),
        plan("p-00000001", Delivery::Resulted, &["w-00000002"]),
    ];
    let work = [work("w-00000001", 100)];
    let requests = [request("q-00000001", "w-00000001", 1, Vec::new())];
    marks(&mut plans, &work, &requests);
    assert!(plans[0].changed.is_empty() && plans[0].unanswered.is_empty());
    assert_eq!(
        plans[1].changed,
        [
            change("w-00000001", ChangeKind::Entered),
            change("w-00000002", ChangeKind::Left)
        ]
    );
    assert_eq!(plans[1].unanswered, ["q-00000001"]);
    assert!(
        plans[3].unanswered.is_empty(),
        "an older plan describes its own time"
    );
    assert_eq!(
        plans[3].changed,
        [change("w-00000002", ChangeKind::Entered)]
    );
}
