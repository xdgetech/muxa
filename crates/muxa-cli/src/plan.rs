//! `plan` and `plan now`: the overall assistant's plans (ADR 0047 §5), behind
//! the host's `work` level 9; `plan reorder` and `plan dismiss` at level 10
//! (ADR 0048). Plan texts and reasons go to stdout only.

use std::time::Duration;

use muxa_protocol::{
    Basis, BasisKind, ChangeKind, Delivery, Message, Plan, PlanAction, PlanRow, Request,
    WORK_PLANS, work_level,
};
use serde_json::json;

use crate::assist::plain;
use crate::conn::Conn;
use crate::items::wire;
use crate::work::{Listing, WAIT};

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
    print!("{}", plan_text(&listing));
    Ok(String::new())
}

/// `plan reorder ID…`: every row of the newest plan with a result, in the
/// user's order.
pub async fn reorder(conn: &mut Conn, order: Vec<String>, as_json: bool) -> Result<String, String> {
    crate::policy::require(conn)?;
    let (plan, rev, rows) = current(conn).await?;
    let mut given = order.clone();
    given.sort();
    let mut wanted = rows.clone();
    wanted.sort();
    if given != wanted {
        return Err(format!(
            "plan reorder names each row of {plan} once: {}",
            rows.join(" ")
        ));
    }
    let messages = conn
        .call(
            |id| Request::UpdatePlan {
                id,
                plan,
                rev,
                action: PlanAction::Reorder,
                order,
                row: None,
                text: None,
            },
            WAIT,
        )
        .await?;
    print_saved(messages, as_json)
}

/// `plan dismiss ROW [--reason TEXT]`: Not today, on the newest plan with a result.
pub async fn dismiss(
    conn: &mut Conn,
    row: String,
    reason: Option<String>,
    as_json: bool,
) -> Result<String, String> {
    crate::policy::require(conn)?;
    let (plan, rev, rows) = current(conn).await?;
    if !rows.contains(&row) {
        return Err(format!("{row} is not a row of {plan}: {}", rows.join(" ")));
    }
    let messages = conn
        .call(
            |id| Request::UpdatePlan {
                id,
                plan,
                rev,
                action: PlanAction::Dismiss,
                order: Vec::new(),
                row: Some(row),
                text: reason,
            },
            WAIT,
        )
        .await?;
    print_saved(messages, as_json)
}

/// The newest plan with a result: its id, `rev` and rows, in the agent's order.
async fn current(conn: &mut Conn) -> Result<(String, u64, Vec<String>), String> {
    let listing = crate::work::fetch(conn).await?;
    let p = listing
        .plans
        .iter()
        .find(|p| p.delivery == Delivery::Resulted)
        .ok_or("no plan with a result yet; muxa-cli plan now makes one")?;
    let rows = p.top.iter().filter_map(row_id).map(str::to_owned).collect();
    Ok((p.id.clone(), p.rev, rows))
}

fn row_id(row: &PlanRow) -> Option<&str> {
    row.work.as_deref().or(row.item.as_deref())
}

fn print_saved(messages: Vec<Message>, as_json: bool) -> Result<String, String> {
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

fn plan_text(listing: &Listing) -> String {
    let plans = &listing.plans;
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
        out.push_str(&rows_text(p, listing));
    }
    out
}

/// The rows in the user's order when there is one; `top` keeps the agent's.
fn ordered(p: &Plan) -> Vec<&PlanRow> {
    let mut rows: Vec<&PlanRow> = p
        .user_order
        .iter()
        .filter_map(|id| p.top.iter().find(|r| row_id(r) == Some(id.as_str())))
        .collect();
    for r in &p.top {
        if !rows.iter().any(|o| std::ptr::eq(*o, r)) {
            rows.push(r);
        }
    }
    rows
}

fn rows_text(p: &Plan, listing: &Listing) -> String {
    let work = &listing.work;
    let mut out = format!("{}\n", header(p));
    if p.top.is_empty() {
        out.push_str("  nothing to rank\n");
    }
    for (n, row) in ordered(p).into_iter().enumerate() {
        let id = row_id(row).unwrap_or("?");
        if p.dismissed.iter().any(|d| d == id) {
            out.push_str(&format!(
                "{}. {id}  {}  not today\n",
                n + 1,
                title(row, listing)
            ));
            continue;
        }
        let m = row.minutes;
        out.push_str(&format!(
            "{}. {id}  {}  {}-{} min, about {}{}\n",
            n + 1,
            title(row, listing),
            m.low,
            m.high,
            m.estimated,
            mark(p, id)
        ));
        out.push_str(&format!("   Why: {}\n", plain(&row.why)));
        out.push_str(&format!("   Next: {}\n", plain(&row.next)));
        for b in &row.basis {
            if let Some(cited) = citation(b, listing) {
                out.push_str(&format!("   Based on: {cited}\n"));
            }
        }
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
fn title(row: &PlanRow, listing: &Listing) -> String {
    let found = match (&row.work, &row.item) {
        (Some(id), _) => listing
            .work
            .iter()
            .find(|w| &w.id == id)
            .map(|w| w.title.clone()),
        (None, Some(id)) => listing
            .items
            .iter()
            .find(|i| &i.id == id)
            .map(|i| i.title.clone()),
        (None, None) => None,
    };
    found.map_or_else(|| "(not listed)".into(), |t| plain(&t))
}

/// A basis that cites what the user told the assistant: the record's id and
/// its text from the list, or "forgotten" when the record is gone.
fn citation(b: &Basis, listing: &Listing) -> Option<String> {
    let id = b.source.as_deref().unwrap_or("?");
    let text = match b.kind {
        BasisKind::Policy => listing
            .policies
            .iter()
            .find(|p| p.id == id)
            .map(|p| plain(&p.text)),
        BasisKind::Hypothesis => listing
            .hypotheses
            .iter()
            .find(|h| h.id == id)
            .map(|h| plain(&h.text)),
        BasisKind::Correction => listing.corrections.iter().find(|c| c.id == id).map(|c| {
            let row = c.row.as_deref().unwrap_or_default();
            format!("{} {row}", wire(json!(c.kind)))
                .trim_end()
                .to_owned()
        }),
        _ => return None,
    };
    Some(format!(
        "{} {id}  {}",
        wire(json!(b.kind)),
        text.unwrap_or_else(|| "forgotten".into())
    ))
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
    use muxa_protocol::{Minutes, PlanChange, Policy, WorkItem, WorkState};

    fn listing(plans: Vec<Plan>, work: Vec<WorkItem>) -> Listing {
        Listing {
            plans,
            work,
            ..Listing::default()
        }
    }

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
            plan_text(&listing(vec![p], work.to_vec())),
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
        let text = plan_text(&listing(vec![failed.clone(), done], Vec::new()));
        assert!(
            text.starts_with(
                "p-2c3d4e5f  failed  2025-11-09T11:30Z\n  The run did not start\n\
                 \nLast plan with a result\np-0a1b2c3d  resulted"
            ),
            "{text}"
        );
        assert!(text.ends_with("  nothing to rank\n"), "{text}");
        assert_eq!(
            plan_text(&listing(vec![failed], Vec::new())),
            "p-2c3d4e5f  failed  2025-11-09T11:30Z\n  The run did not start\n"
        );
        assert!(plan_text(&Listing::default()).starts_with("no plan yet"));
    }

    #[test]
    fn rows_follow_the_users_order_and_cite_what_the_user_told() {
        let mut p = plan("p-3d4e5f6a", Delivery::Resulted);
        let mut first = row("w-0b1c2d3e");
        first.basis = vec![
            Basis {
                kind: BasisKind::Policy,
                text: "Recruiting first.".into(),
                source: Some("y-0a1b2c3d".into()),
                at: None,
            },
            Basis {
                kind: BasisKind::Hypothesis,
                text: "Due work first.".into(),
                source: Some("h-4e5f6a7b".into()),
                at: None,
            },
            Basis {
                kind: BasisKind::Manual,
                text: "Pinned".into(),
                source: None,
                at: None,
            },
        ];
        p.top = vec![first, row("w-7b8c9d0e"), row("w-8c9d0e1f")];
        p.user_order = vec!["w-7b8c9d0e".into(), "w-0b1c2d3e".into()];
        p.dismissed = vec!["w-8c9d0e1f".into()];
        let mut l = listing(vec![p], vec![item("w-8c9d0e1f", "Renew the passport")]);
        l.policies = vec![Policy {
            id: "y-0a1b2c3d".into(),
            rev: 1,
            text: "This week, recruiting comes before sales.".into(),
            scope: "all".into(),
            from: None,
            until: None,
            source: "user".into(),
            created_at: 0,
            updated_at: 0,
        }];
        let text = plan_text(&l);
        let first = text.find("1. w-7b8c9d0e").expect(&text);
        let second = text.find("2. w-0b1c2d3e").expect(&text);
        assert!(first < second, "{text}");
        assert!(
            text.contains(
                "   Based on: policy y-0a1b2c3d  This week, recruiting comes before sales.\n   \
                 Based on: hypothesis h-4e5f6a7b  forgotten\n3. w-8c9d0e1f  Renew the passport  not today\n"
            ),
            "{text}"
        );
        assert!(!text.contains("Pinned"), "{text}");
    }
}
