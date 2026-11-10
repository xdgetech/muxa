//! A workspace's staff and a request's chain of steps (ADR 0051), at `work`
//! level 12: `assist staff WORKSPACE`, the check before `assist --worker`,
//! and the step rows of `assist show` and `work show`. Descriptions and
//! purposes go to stdout only.

use muxa_protocol::{AssistantRequest, StaffMember, StepBy, WORK_STAFF, work_level};
use serde_json::json;

use crate::assist::plain;
use crate::conn::Conn;
use crate::items::wire;

pub const NEEDS_STAFF: &str =
    "this host has no staff members to ask; update muxa-bridge on the host";

pub fn require(conn: &Conn) -> Result<(), String> {
    if work_level(conn.capabilities()) >= WORK_STAFF {
        Ok(())
    } else {
        Err(NEEDS_STAFF.into())
    }
}

/// `assist staff WORKSPACE`: each member, the lead marked, in configuration order.
pub async fn list(conn: &mut Conn, workspace: &str, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let listing = crate::work::fetch(conn).await?;
    let delegate = listing
        .assistant
        .as_ref()
        .and_then(|a| a.delegates.iter().find(|d| d.workspace == workspace))
        .ok_or_else(|| format!("workspace {workspace} takes no assistant requests"))?;
    if as_json {
        println!("{}", json!(delegate.staff));
        return Ok(String::new());
    }
    if delegate.staff.is_empty() {
        println!("{workspace} has no staff: its assistant answers every request");
        return Ok(String::new());
    }
    print!("{}", staff_text(&delegate.staff, crate::when::now()));
    Ok(String::new())
}

fn staff_text(staff: &[StaffMember], now: u64) -> String {
    let mut out = String::new();
    for m in staff {
        let lead = if m.lead { "  lead" } else { "" };
        let notes = m.notes_at.map_or_else(
            || "  no notes".to_owned(),
            |at| format!("  notes {} ago", crate::items::age(now.saturating_sub(at))),
        );
        out.push_str(&format!("{}{lead}{notes}\n", plain(&m.worker)));
        for line in plain(&m.description).lines() {
            out.push_str(&format!("  {line}\n"));
        }
    }
    out
}

/// One row per step of the current version, and the hand-off that waits
/// after the last one. Empty for a request without staff.
pub fn steps_text(q: &AssistantRequest) -> String {
    let mut out = String::new();
    for s in &q.steps {
        let by = match s.by {
            StepBy::User => "asked by you",
            StepBy::Overall => "asked by the overall assistant",
            StepBy::Lead => "the lead took it",
            StepBy::Handoff => "handed on",
            StepBy::Unknown => "unknown",
        };
        out.push_str(&format!(
            "  {}. {}  {}  {by}\n",
            s.step,
            plain(&s.worker),
            wire(json!(s.delivery))
        ));
        if let Some(h) = &s.handoff {
            out.push_str(&format!(
                "     hand-off to {}: {}\n",
                plain(&h.worker),
                plain(&h.purpose)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::{Delivery, Handoff, RequestStep};

    fn step(n: u32, worker: &str, by: StepBy, delivery: Delivery) -> RequestStep {
        RequestStep {
            step: n,
            worker: worker.into(),
            by,
            delivery,
            delivery_at: 0,
            run: None,
            handoff: None,
        }
    }

    #[test]
    fn steps_show_who_why_and_the_waiting_hand_off() {
        let mut q: AssistantRequest = serde_json::from_value(json!({
            "id": "q-0000abcd", "work": "w-0000abcd", "version": 2, "workspace": "lisk",
            "scope": "compare", "delivery": "resulted", "deliveryAt": 0,
            "createdAt": 0, "updatedAt": 0,
        }))
        .unwrap();
        assert_eq!(steps_text(&q), "");
        q.steps = vec![
            step(1, "lisk-chief", StepBy::Lead, Delivery::Resulted),
            step(2, "lisk-strategist", StepBy::Handoff, Delivery::Resulted),
        ];
        q.steps[1].handoff = Some(Handoff {
            worker: "lisk-architect".into(),
            purpose: "Check the \u{1b}[31mgateway\u{1b}[0m.".into(),
        });
        assert_eq!(
            steps_text(&q),
            "  1. lisk-chief  resulted  the lead took it\n\
             \x20 2. lisk-strategist  resulted  handed on\n\
             \x20    hand-off to lisk-architect: Check the [31mgateway[0m.\n"
        );
    }

    #[test]
    fn the_staff_lists_the_lead_its_notes_and_each_description() {
        let staff = vec![
            StaffMember {
                worker: "lisk-chief".into(),
                description: "Chief of staff.\nKeeps the plan.".into(),
                lead: true,
                notes_at: Some(40),
            },
            StaffMember {
                worker: "lisk-architect".into(),
                description: "Architect.".into(),
                lead: false,
                notes_at: None,
            },
        ];
        assert_eq!(
            staff_text(&staff, 100),
            "lisk-chief  lead  notes 1m ago\n  Chief of staff.\n  Keeps the plan.\n\
             lisk-architect  no notes\n  Architect.\n"
        );
    }
}
