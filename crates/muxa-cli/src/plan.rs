//! `plan` and `plan now`: the overall assistant's plans (ADR 0047 §5), behind
//! the host's `work` level 9. Plan texts go to stdout only.

use std::time::Duration;

use muxa_protocol::{
    ChangeKind, Delivery, InboxItem, Message, Plan, PlanRow, Request, WORK_PLANS, WorkItem,
    work_level,
};
use serde_json::json;

use crate::assist::plain;
use crate::conn::Conn;
use crate::items::wire;

/// `plan-now` starts the overall run's session before it answers.
const LAUNCH_WAIT: Duration = Duration::from_secs(120);

pub fn require(conn: &Conn) -> Result<(), String> {
    if work_level(conn.capabilities()) >= WORK_PLANS {
        Ok(())
    } else {
        Err("this host makes no plans; update muxa-bridge on the host".into())
    }
}

/// `plan`: the newest plan, and the rows of the newest one with a result.
pub async fn show(conn: &mut Conn, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let listing = crate::work::fetch(conn).await?;
    if as_json {
        println!("{}", json!(listing.plans.first()));
        return Ok(String::new());
    }
    print!(
        "{}",
        plan_text(&listing.plans, &listing.work, &listing.items)
    );
    Ok(String::new())
}

/// `plan now [--minutes N]`: sends `plan-now`.
pub async fn now(conn: &mut Conn, minutes: Option<u32>, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let messages = conn
        .call(|id| Request::PlanNow { id, minutes }, LAUNCH_WAIT)
        .await?;
    let plan = messages
        .into_iter()
        .find_map(|m| match m {
            Message::PlanSaved { entry, .. } => Some(entry),
            _ => None,
        })
        .ok_or("the bridge saved no plan")?;
    if as_json {
        println!("{}", json!(plan));
    } else {
        println!("{}", header(&plan));
    }
    Ok(String::new())
}

/// `p-… resulted 2026-11-10T07:30Z (45 min) [personal, work]`.
pub fn header(p: &Plan) -> String {
    let minutes = p
        .minutes
        .map(|m| format!("  ({m} min)"))
        .unwrap_or_default();
    let workspaces = if p.workspaces.is_empty() {
        String::new()
    } else {
        format!("  [{}]", p.workspaces.join(", "))
    };
    format!(
        "{}  {}  {}{minutes}{workspaces}",
        p.id,
        wire(json!(p.delivery)),
        crate::when::format(p.at)
    )
}

fn plan_text(plans: &[Plan], work: &[WorkItem], items: &[InboxItem]) -> String {
    let Some(newest) = plans.first() else {
        return "no plan yet; muxa-cli plan now makes one\n".into();
    };
    let shown = plans.iter().find(|p| p.delivery == Delivery::Resulted);
    let mut out = String::new();
    if shown.map(|p| &p.id) != Some(&newest.id) {
        out.push_str(&format!("{}\n", header(newest)));
        if let Some(reason) = &newest.reason {
            out.push_str(&format!("  {}\n", plain(reason)));
        }
        if shown.is_none() {
            return out;
        }
        out.push_str("\nLast plan with a result\n");
    }
    if let Some(p) = shown {
        out.push_str(&rows_text(p, work, items));
    }
    out
}

fn rows_text(p: &Plan, work: &[WorkItem], items: &[InboxItem]) -> String {
    let mut out = format!("{}\n", header(p));
    if p.top.is_empty() {
        out.push_str("  nothing to rank\n");
    }
    for (n, row) in p.top.iter().enumerate() {
        let id = row.work.as_deref().or(row.item.as_deref()).unwrap_or("?");
        let m = row.minutes;
        out.push_str(&format!(
            "{}. {id}  {}  {}-{} min, about {}{}\n",
            n + 1,
            title(row, work, items),
            m.low,
            m.high,
            m.estimated,
            mark(p, id)
        ));
        out.push_str(&format!("   Why: {}\n", plain(&row.why)));
        out.push_str(&format!("   Next: {}\n", plain(&row.next)));
    }
    if !p.due_soon.is_empty() {
        let due: Vec<String> = p
            .due_soon
            .iter()
            .map(|id| match work.iter().find(|w| &w.id == id) {
                Some(w) => format!("{id} {}", plain(&w.title)),
                None => id.clone(),
            })
            .collect();
        out.push_str(&format!(
            "Due soon, moved down by you: {}\n",
            due.join(", ")
        ));
    }
    let left: Vec<&str> = p
        .changed
        .iter()
        .filter(|c| c.change == ChangeKind::Left)
        .map(|c| c.id.as_str())
        .collect();
    for (label, ids) in [
        ("Left the plan", left),
        ("Unanswered requests", strs(&p.unanswered)),
        ("Stale results", strs(&p.stale)),
    ] {
        if !ids.is_empty() {
            out.push_str(&format!("{label}: {}\n", ids.join(" ")));
        }
    }
    out
}

fn strs(ids: &[String]) -> Vec<&str> {
    ids.iter().map(String::as_str).collect()
}

/// The Work item's or the Inbox item's title, when the list has it.
fn title(row: &PlanRow, work: &[WorkItem], items: &[InboxItem]) -> String {
    let found = match (&row.work, &row.item) {
        (Some(id), _) => work.iter().find(|w| &w.id == id).map(|w| w.title.clone()),
        (None, Some(id)) => items.iter().find(|i| &i.id == id).map(|i| i.title.clone()),
        (None, None) => None,
    };
    found.map_or_else(|| "(not listed)".into(), |t| plain(&t))
}

fn mark(p: &Plan, id: &str) -> &'static str {
    match p.changed.iter().find(|c| c.id == id).map(|c| c.change) {
        Some(ChangeKind::Entered) => "  new",
        Some(ChangeKind::Moved) => "  moved",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::{Minutes, PlanChange, WorkState};

    fn plan(id: &str, delivery: Delivery) -> Plan {
        Plan {
            id: id.into(),
            rev: 1,
            at: 1_762_687_800,
            inputs_at: 0,
            delivery,
            delivery_at: 0,
            minutes: None,
            run: None,
            workspaces: Vec::new(),
            top: Vec::new(),
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

    fn row(work: &str) -> PlanRow {
        PlanRow {
            work: Some(work.into()),
            item: None,
            why: "Due \u{1b}[31mWednesday".into(),
            next: "Fill in the table.".into(),
            minutes: Minutes {
                low: 60,
                high: 120,
                estimated: 90,
            },
            basis: Vec::new(),
            missing: Vec::new(),
        }
    }

    fn item(id: &str, title: &str) -> WorkItem {
        WorkItem {
            id: id.into(),
            rev: 1,
            title: title.into(),
            state: WorkState::Open,
            created_at: 0,
            updated_at: 0,
            workspace: None,
            later_at: None,
            merged_into: None,
            pinned_at: None,
            due: None,
            captures: Vec::new(),
            requests: Vec::new(),
            tasks: Vec::new(),
        }
    }

    #[test]
    fn rows_show_title_minutes_marks_and_the_derived_lists() {
        let mut p = plan("p-0a1b2c3d", Delivery::Resulted);
        p.workspaces = vec!["work".into()];
        p.top = vec![row("w-0b1c2d3e"), row("w-0000ffff")];
        p.due_soon = vec!["w-8c9d0e1f".into()];
        p.unanswered = vec!["q-2c3d4e5f".into()];
        p.changed = vec![
            PlanChange {
                id: "w-0b1c2d3e".into(),
                change: ChangeKind::Moved,
            },
            PlanChange {
                id: "w-5f6a7b8c".into(),
                change: ChangeKind::Left,
            },
        ];
        let work = [
            item("w-0b1c2d3e", "Prepare the report"),
            item("w-8c9d0e1f", "Renew the passport"),
        ];
        assert_eq!(
            plan_text(&[p], &work, &[]),
            "p-0a1b2c3d  resulted  2025-11-09T11:30Z  [work]\n\
             1. w-0b1c2d3e  Prepare the report  60-120 min, about 90  moved\n   \
             Why: Due [31mWednesday\n   Next: Fill in the table.\n\
             2. w-0000ffff  (not listed)  60-120 min, about 90\n   \
             Why: Due [31mWednesday\n   Next: Fill in the table.\n\
             Due soon, moved down by you: w-8c9d0e1f Renew the passport\n\
             Left the plan: w-5f6a7b8c\nUnanswered requests: q-2c3d4e5f\n"
        );
    }

    #[test]
    fn a_failed_newest_plan_shows_its_reason_above_the_last_result() {
        let mut failed = plan("p-2c3d4e5f", Delivery::Failed);
        failed.reason = Some("The run did not start".into());
        let done = plan("p-0a1b2c3d", Delivery::Resulted);
        let text = plan_text(&[failed.clone(), done], &[], &[]);
        assert!(
            text.starts_with(
                "p-2c3d4e5f  failed  2025-11-09T11:30Z\n  The run did not start\n\
                 \nLast plan with a result\np-0a1b2c3d  resulted"
            ),
            "{text}"
        );
        assert!(text.ends_with("  nothing to rank\n"), "{text}");
        assert_eq!(
            plan_text(&[failed], &[], &[]),
            "p-2c3d4e5f  failed  2025-11-09T11:30Z\n  The run did not start\n"
        );
        assert!(plan_text(&[], &[], &[]).starts_with("no plan yet"));
    }
}
