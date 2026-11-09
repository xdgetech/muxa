//! When the overall assistant plans (ADR 0047 §5): each schedule in
//! `plans` is a Routine's (ADR 0036 §3), with one catch-up and a DST fold
//! run once (`routines::schedule`). The last occurrence handled per schedule
//! is kept in `<state dir>/assistant/schedule.json`. Logs carry schedule times.
use std::collections::BTreeMap;
use std::os::unix::fs::DirBuilderExt;
use std::path::Path;

use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::start::How;
use crate::records::routines::Trigger;
use crate::records::store::Lock;
use crate::routines::schedule::{decide, occurrence_id, occurrence_on};
use crate::routines::store::{StateError, read_json, write_json};
use crate::task_records::{Config, Context};

const DIR: &str = "assistant";
const FILE: &str = "schedule";
const FORMAT: u32 = 1;
const MAX_FILE: u64 = 64 << 10;

/// The last occurrence handled per schedule, keyed by its schedule and zone.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    version: u32,
    #[serde(default)]
    last: BTreeMap<String, String>,
}

/// One due plan: the latest due occurrence across the schedules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Due {
    /// Epoch seconds.
    pub at: u64,
    /// `YYYY-MM-DDTHH:MM`, the schedule's local time.
    pub occurrence: String,
}

/// The daemon's tick: starts the due plan in the background.
pub async fn tick(cx: &Context) {
    let config = std::sync::Arc::clone(&cx.records);
    let now = crate::records::now();
    let due = tokio::task::spawn_blocking(move || due_now(&config, now))
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, "the plan schedule stopped");
            None
        });
    let Some(due) = due else { return };
    let cx2 = cx.clone();
    cx.pending.spawn(async move {
        let how = How::Scheduled {
            at: due.at,
            occurrence: due.occurrence.clone(),
        };
        match super::start::start(&cx2, how).await {
            Ok((plan, None)) => {
                tracing::info!(plan = %plan.id, occurrence = %due.occurrence, delivery = ?plan.delivery, "scheduled plan")
            }
            Ok((plan, Some(e))) => {
                tracing::info!(plan = %plan.id, occurrence = %due.occurrence, error = %e.message, "scheduled plan did not start")
            }
            Err(e) => {
                tracing::info!(occurrence = %due.occurrence, error = %e.message, "scheduled plan not made")
            }
        }
    });
}

/// Blocking. The plan due now, after recording every handled occurrence.
/// Nothing is recorded while a plan is being made: its occurrence stays due.
fn due_now(config: &Config, now: u64) -> Option<Due> {
    let state_dir = config.state_dir.as_ref().filter(|d| d.is_dir())?;
    let workspaces = config.load_workspaces().ok()?;
    let triggers = workspaces.overall()?.assistant.as_ref()?.plans.clone();
    if triggers.is_empty() || super::start::busy(config, now) {
        return None;
    }
    let now_ts = Timestamp::from_second(i64::try_from(now).ok()?).ok()?;
    match advance(state_dir, &triggers, now_ts) {
        Ok(due) => due,
        Err(e) => {
            tracing::warn!(error = %e, "cannot keep the plan schedule");
            None
        }
    }
}

/// Under the state's lock: each schedule's decision recorded; the latest due
/// occurrence returned. Several due at once make one plan.
fn advance(
    state_dir: &Path,
    triggers: &[Trigger],
    now: Timestamp,
) -> Result<Option<Due>, StateError> {
    let dir = state_dir.join(DIR);
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|source| StateError::Io {
            action: "create",
            path: dir.clone(),
            source,
        })?;
    let _lock = Lock::acquire(&dir, FILE, true)?;
    let path = dir.join(format!("{FILE}.json"));
    let mut state: State = read_json(&path, FILE, MAX_FILE)?.unwrap_or_default();
    let mut due: Option<(Timestamp, Due)> = None;
    let mut changed = state.version != FORMAT;
    for t in triggers {
        let key = format!("{t:?}");
        let last = state.last.get(&key).and_then(|d| d.parse::<Date>().ok());
        let decision = match decide(t, now, last) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(schedule = %key, error = %e, "cannot schedule a plan");
                continue;
            }
        };
        if last != Some(decision.last) {
            state.last.insert(key.clone(), decision.last.to_string());
            changed = true;
        }
        let Some(date) = decision.run else { continue };
        let Ok(at) = occurrence_on(t, date) else {
            continue;
        };
        tracing::info!(schedule = %key, occurrence = %occurrence_id(t, date), "plan due");
        if due.as_ref().is_none_or(|(prev, _)| at > *prev) {
            let seconds = u64::try_from(at.as_second()).unwrap_or(0);
            let occurrence = occurrence_id(t, date);
            due = Some((
                at,
                Due {
                    at: seconds,
                    occurrence,
                },
            ));
        }
    }
    if changed {
        state.version = FORMAT;
        write_json(&dir, FILE, &state, MAX_FILE)?;
    }
    Ok(due.map(|(_, d)| d))
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::tz::TimeZone;

    fn temp() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "muxa-plan-schedule-{}-{:08x}",
            std::process::id(),
            getrandom::u32().unwrap()
        ));
        std::fs::create_dir_all(dir.join(DIR)).unwrap();
        dir
    }

    /// Berlin's rule without a tz database: DST ends 2026-10-25 03:00 CEST.
    fn berlin(schedule: &str) -> Trigger {
        let mut t = crate::records::routines::scheduled(schedule, Some("UTC")).unwrap();
        t.timezone = "Europe/Berlin".into();
        t.tz = TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap();
        t
    }

    fn utc(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn the_first_look_runs_nothing_then_one_catch_up_then_the_next_occurrence() {
        let dir = temp();
        let t = [berlin("30 7 * * 1-5")];
        // Tuesday 2026-11-10, 09:00 Berlin: the baseline is today's 07:30.
        assert_eq!(
            advance(&dir, &t, utc("2026-11-10T08:00:00Z")).unwrap(),
            None
        );
        // Asleep until Friday 10:00: one plan, for Friday's 07:30.
        let due = advance(&dir, &t, utc("2026-11-13T09:00:00Z"))
            .unwrap()
            .unwrap();
        assert_eq!(due.occurrence, "2026-11-13T07:30");
        assert_eq!(due.at, utc("2026-11-13T06:30:00Z").as_second() as u64);
        assert_eq!(
            advance(&dir, &t, utc("2026-11-13T09:01:00Z")).unwrap(),
            None
        );
        // The weekend has none; Monday's comes at its time.
        assert_eq!(
            advance(&dir, &t, utc("2026-11-16T06:29:00Z")).unwrap(),
            None
        );
        let monday = advance(&dir, &t, utc("2026-11-16T06:30:00Z"))
            .unwrap()
            .unwrap();
        assert_eq!(monday.occurrence, "2026-11-16T07:30");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_repeated_hour_in_the_dst_fold_plans_once() {
        let dir = temp();
        let t = [berlin("30 2 * * *")];
        assert_eq!(
            advance(&dir, &t, utc("2026-10-24T12:00:00Z")).unwrap(),
            None
        );
        // 2026-10-25 02:30 happens at 00:30 and again at 01:30 UTC.
        let first = advance(&dir, &t, utc("2026-10-25T00:30:00Z")).unwrap();
        assert_eq!(first.unwrap().occurrence, "2026-10-25T02:30");
        assert_eq!(
            advance(&dir, &t, utc("2026-10-25T01:30:00Z")).unwrap(),
            None
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn schedules_due_together_make_one_plan_for_the_latest() {
        let dir = temp();
        let t = [berlin("30 7 * * *"), berlin("0 13 * * *")];
        assert_eq!(
            advance(&dir, &t, utc("2026-11-09T20:00:00Z")).unwrap(),
            None
        );
        let due = advance(&dir, &t, utc("2026-11-10T15:00:00Z"))
            .unwrap()
            .unwrap();
        assert_eq!(due.occurrence, "2026-11-10T13:00");
        assert_eq!(
            advance(&dir, &t, utc("2026-11-10T15:00:01Z")).unwrap(),
            None
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
