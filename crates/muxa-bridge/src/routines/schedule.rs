//! When a Routine is due (ADR 0026 section 2), as pure functions of the
//! trigger, the time and the last occurrence the host handled. One
//! occurrence per local date: a repeated wall-clock time (DST fold) runs
//! once, at its earlier instant; a time in a DST gap runs at the next valid
//! minute.
use jiff::civil::{Date, DateTime};
use jiff::tz::AmbiguousOffset;
use jiff::{Timestamp, ToSpan};

use crate::records::routines::Trigger;

/// No gap in the tz database is longer than a day.
const GAP_MINUTES: u32 = 24 * 60;

#[derive(Debug, thiserror::Error)]
#[error("cannot compute the routine's time: {0}")]
pub struct ScheduleError(String);

fn err(e: jiff::Error) -> ScheduleError {
    ScheduleError(e.to_string())
}

/// What a tick should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    /// The occurrence to run now, if one is due.
    pub run: Option<Date>,
    /// The last handled occurrence after this tick. A first look records
    /// the latest past occurrence without running it.
    pub last: Date,
}

/// The instant of the occurrence on `date`.
pub fn occurrence_on(trigger: &Trigger, date: Date) -> Result<Timestamp, ScheduleError> {
    let mut dt: DateTime = date.at(trigger.hour, trigger.minute, 0, 0);
    for _ in 0..GAP_MINUTES {
        let ambiguous = trigger.tz.to_ambiguous_timestamp(dt);
        match ambiguous.offset() {
            AmbiguousOffset::Gap { .. } => {
                dt = dt.checked_add(1.minute()).map_err(err)?;
            }
            _ => return ambiguous.earlier().map_err(err),
        }
    }
    Err(ScheduleError("no valid local time within a day".into()))
}

/// The latest date whose occurrence is at or before `now`.
pub fn latest_due(trigger: &Trigger, now: Timestamp) -> Result<Date, ScheduleError> {
    let today = now.to_zoned(trigger.tz.clone()).date();
    if occurrence_on(trigger, today)? <= now {
        Ok(today)
    } else {
        today.yesterday().map_err(err)
    }
}

/// At most one run per tick, for the latest due occurrence only: missed
/// occurrences before it never run (one catch-up).
pub fn decide(
    trigger: &Trigger,
    now: Timestamp,
    last: Option<Date>,
) -> Result<Decision, ScheduleError> {
    let latest = latest_due(trigger, now)?;
    Ok(match last {
        None => Decision {
            run: None,
            last: latest,
        },
        Some(last) if latest > last => Decision {
            run: Some(latest),
            last: latest,
        },
        Some(last) => Decision { run: None, last },
    })
}

/// The next occurrence after `last` (or after the latest past one, before
/// the first look). In the past while a run blocks a due one.
pub fn next_due(
    trigger: &Trigger,
    now: Timestamp,
    last: Option<Date>,
) -> Result<Timestamp, ScheduleError> {
    let after = match last {
        Some(last) => last,
        None => latest_due(trigger, now)?,
    };
    occurrence_on(trigger, after.tomorrow().map_err(err)?)
}

/// `2026-11-03T09:00`: the date and the configured time, also when a gap moved it.
pub fn occurrence_id(trigger: &Trigger, date: Date) -> String {
    format!("{date}T{}", trigger.at())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    /// Central Europe with the EU rule: DST from the last Sunday of March
    /// 02:00 to the last Sunday of October 03:00. No system tz database needed.
    fn berlin(at: &str) -> Trigger {
        let (h, m) = at.split_once(':').unwrap();
        Trigger {
            hour: h.parse().unwrap(),
            minute: m.parse().unwrap(),
            timezone: "Europe/Berlin".into(),
            tz: TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap(),
        }
    }

    fn utc(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    #[test]
    fn an_ordinary_day_runs_at_the_local_time() {
        let t = berlin("09:00");
        assert_eq!(
            occurrence_on(&t, date(2026, 11, 3)).unwrap(),
            utc("2026-11-03T08:00:00Z")
        );
        assert_eq!(
            occurrence_on(&t, date(2026, 7, 3)).unwrap(),
            utc("2026-07-03T07:00:00Z")
        );
        assert_eq!(occurrence_id(&t, date(2026, 11, 3)), "2026-11-03T09:00");
    }

    #[test]
    fn a_time_in_the_spring_gap_moves_to_the_next_valid_minute() {
        // 2026-03-29: 02:00 CET jumps to 03:00 CEST.
        let t = berlin("02:30");
        let at = occurrence_on(&t, date(2026, 3, 29)).unwrap();
        assert_eq!(at, utc("2026-03-29T01:00:00Z"));
        let local = at.to_zoned(t.tz.clone());
        assert_eq!((local.hour(), local.minute()), (3, 0));
        // The next day is ordinary again.
        assert_eq!(
            occurrence_on(&t, date(2026, 3, 30)).unwrap(),
            utc("2026-03-30T00:30:00Z")
        );
    }

    #[test]
    fn a_repeated_time_in_the_autumn_fold_is_one_occurrence() {
        // 2026-10-25: 03:00 CEST goes back to 02:00 CET; 02:30 happens twice.
        let t = berlin("02:30");
        let first = occurrence_on(&t, date(2026, 10, 25)).unwrap();
        assert_eq!(first, utc("2026-10-25T00:30:00Z"));
        let last = Some(date(2026, 10, 24));
        let at_first = decide(&t, first, last).unwrap();
        assert_eq!(at_first.run, Some(date(2026, 10, 25)));
        // An hour later the wall clock shows 02:30 again: nothing new is due.
        let again = decide(&t, utc("2026-10-25T01:30:00Z"), Some(at_first.last)).unwrap();
        assert_eq!(again.run, None);
        assert_eq!(
            next_due(&t, utc("2026-10-25T01:30:00Z"), Some(at_first.last)).unwrap(),
            utc("2026-10-26T01:30:00Z")
        );
    }

    #[test]
    fn the_first_look_records_a_baseline_and_runs_nothing() {
        let t = berlin("09:00");
        let before = decide(&t, utc("2026-11-03T07:59:00Z"), None).unwrap();
        assert_eq!(
            before,
            Decision {
                run: None,
                last: date(2026, 11, 2)
            }
        );
        assert_eq!(
            next_due(&t, utc("2026-11-03T07:59:00Z"), None).unwrap(),
            utc("2026-11-03T08:00:00Z")
        );
        let after = decide(&t, utc("2026-11-03T08:01:00Z"), None).unwrap();
        assert_eq!(
            after,
            Decision {
                run: None,
                last: date(2026, 11, 3)
            }
        );
    }

    #[test]
    fn a_due_time_runs_once() {
        let t = berlin("09:00");
        let last = Some(date(2026, 11, 2));
        assert_eq!(
            decide(&t, utc("2026-11-03T07:59:59Z"), last).unwrap().run,
            None
        );
        let due = decide(&t, utc("2026-11-03T08:00:00Z"), last).unwrap();
        assert_eq!(due.run, Some(date(2026, 11, 3)));
        assert_eq!(
            decide(&t, utc("2026-11-03T08:00:02Z"), Some(due.last))
                .unwrap()
                .run,
            None
        );
    }

    #[test]
    fn after_a_long_sleep_only_the_latest_missed_occurrence_runs() {
        let t = berlin("09:00");
        let woke = utc("2026-11-10T12:00:00Z");
        let d = decide(&t, woke, Some(date(2026, 11, 3))).unwrap();
        assert_eq!(d.run, Some(date(2026, 11, 10)));
        assert_eq!(decide(&t, woke, Some(d.last)).unwrap().run, None);
        // Woken before today's time: yesterday's occurrence is the one catch-up.
        let early = utc("2026-11-10T06:00:00Z");
        let d = decide(&t, early, Some(date(2026, 11, 3))).unwrap();
        assert_eq!(d.run, Some(date(2026, 11, 9)));
        assert_eq!(
            next_due(&t, early, Some(d.last)).unwrap(),
            utc("2026-11-10T08:00:00Z")
        );
    }

    #[test]
    fn a_blocked_occurrence_stays_due_and_reports_a_past_next_due() {
        let t = berlin("09:00");
        let now = utc("2026-11-04T09:00:00Z");
        let last = Some(date(2026, 11, 2));
        assert_eq!(
            next_due(&t, now, last).unwrap(),
            utc("2026-11-03T08:00:00Z")
        );
        assert_eq!(decide(&t, now, last).unwrap().run, Some(date(2026, 11, 4)));
    }
}
