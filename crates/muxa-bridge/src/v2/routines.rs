//! Routines on protocol v2: the v1 engine's list and Run now, in v2's
//! shape. The engine moves to herdr-core with the tmux removal (phase F).

use muxa_protocol::v2::{self, RoutineLast};
use muxa_protocol::{RoutineInfo, RoutineProblem};

/// A v1 Routine as v2 shows it: the newer of the last success and failure.
pub fn to_v2(r: RoutineInfo) -> v2::Routine {
    let at = |t: u64| i64::try_from(t).unwrap_or(i64::MAX);
    let ok = r.last_success.as_ref().map(|s| RoutineLast {
        at: at(s.at),
        ok: true,
        reason: None,
        published: Some(s.published),
    });
    let failed = r.last_failure.as_ref().map(|f| RoutineLast {
        at: at(f.at),
        ok: false,
        reason: Some(f.reason.clone()),
        published: None,
    });
    let last = match (ok, failed) {
        (Some(o), Some(f)) => Some(if f.at > o.at { f } else { o }),
        (o, f) => o.or(f),
    };
    v2::Routine {
        workspace: r.workspace,
        name: r.name,
        enabled: r.enabled,
        at: r.trigger.as_ref().map(|t| t.at.clone()),
        timezone: r.trigger.as_ref().map(|t| t.timezone.clone()),
        schedule: r.trigger.and_then(|t| t.schedule),
        next_due: r.next_due.map(at),
        running_since: r.running.map(|run| at(run.started_at)),
        last,
        bot: r.bot,
    }
}

pub fn problem(p: RoutineProblem) -> v2::RoutineProblem {
    v2::RoutineProblem {
        workspace: p.workspace,
        reason: p.reason,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn info(v: serde_json::Value) -> RoutineInfo {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_newer_run_is_the_last_one() {
        let r = to_v2(info(
            json!({"workspace": "lisk", "name": "digest", "enabled": true,
            "trigger": {"at": "09:00", "timezone": "Europe/Berlin", "schedule": "0 9 * * 1-5"},
            "nextDue": 500,
            "lastSuccess": {"at": 100, "occurrence": "o", "task": "t", "published": 2},
            "lastFailure": {"at": 200, "occurrence": "o", "reason": "gh login expired"}}),
        ));
        assert_eq!(r.at.as_deref(), Some("09:00"));
        assert_eq!(r.schedule.as_deref(), Some("0 9 * * 1-5"));
        assert_eq!(r.next_due, Some(500));
        let last = r.last.unwrap();
        assert_eq!(
            (last.ok, last.reason.as_deref()),
            (false, Some("gh login expired"))
        );
    }

    #[test]
    fn a_routine_on_request_has_no_schedule() {
        let r = to_v2(info(
            json!({"workspace": "w", "name": "n", "enabled": false,
            "running": {"occurrence": "now-1", "startedAt": 7}}),
        ));
        assert_eq!(
            (r.at, r.next_due, r.running_since, r.last),
            (None, None, Some(7), None)
        );
    }
}
