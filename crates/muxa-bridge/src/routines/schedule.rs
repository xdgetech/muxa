//! When a Routine is due (ADR 0026 section 2, ADR 0082 section 4), as pure
//! functions of the trigger, the time and the last occurrence the host
//! handled. An occurrence is a local date and time of the schedule: a
//! repeated wall-clock time (DST fold) runs once, at its earlier instant; a
//! time in a DST gap runs at the next valid minute.
use jiff::civil::{Date, DateTime};
use jiff::tz::AmbiguousOffset;
use jiff::{Timestamp, ToSpan};

use crate::records::routines::Trigger;

/// No gap in the tz database is longer than a day.
const GAP_MINUTES: u32 = 24 * 60;
/// Four years and a day: a schedule that parses matches a day within it
/// (`records::cron`), February 29 included.
const SEARCH_DAYS: u32 = 4 * 366 + 1;

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
    pub run: Option<DateTime>,
    /// The last handled occurrence after this tick. A first look records
    /// the latest past occurrence without running it.
    pub last: DateTime,
}

/// The instant of the occurrence `slot`, a local date and time of the schedule.
pub fn occurrence_at(trigger: &Trigger, slot: DateTime) -> Result<Timestamp, ScheduleError> {
    let mut dt = slot;
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

fn slots(trigger: &Trigger, date: Date) -> impl DoubleEndedIterator<Item = DateTime> + '_ {
    trigger
        .times
        .each()
        .map(move |(hour, minute)| date.at(hour, minute, 0, 0))
}

/// The latest occurrence at or before `now`.
pub fn latest_due(trigger: &Trigger, now: Timestamp) -> Result<DateTime, ScheduleError> {
    let mut date = now.to_zoned(trigger.tz.clone()).date();
    for _ in 0..SEARCH_DAYS {
        if trigger.days.matches(date) {
            for slot in slots(trigger, date).rev() {
                if occurrence_at(trigger, slot)? <= now {
                    return Ok(slot);
                }
            }
        }
        date = date.yesterday().map_err(err)?;
    }
    Err(ScheduleError("no scheduled day within four years".into()))
}

/// The first occurrence after `after`.
fn next_slot(trigger: &Trigger, after: DateTime) -> Result<DateTime, ScheduleError> {
    let mut date = after.date();
    for _ in 0..SEARCH_DAYS {
        if trigger.days.matches(date) {
            if let Some(slot) = slots(trigger, date).find(|s| *s > after) {
                return Ok(slot);
            }
        }
        date = date.tomorrow().map_err(err)?;
    }
    Err(ScheduleError("no scheduled day within four years".into()))
}

/// At most one run per tick, for the latest due occurrence only: missed
/// occurrences before it never run (one catch-up).
pub fn decide(
    trigger: &Trigger,
    now: Timestamp,
    last: Option<DateTime>,
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
    last: Option<DateTime>,
) -> Result<Timestamp, ScheduleError> {
    let after = match last {
        Some(last) => last,
        None => latest_due(trigger, now)?,
    };
    occurrence_at(trigger, next_slot(trigger, after)?)
}

/// `2026-11-03T09:00`: the scheduled local time, also when a gap moved it.
pub fn occurrence_id(slot: DateTime) -> String {
    slot.strftime("%Y-%m-%dT%H:%M").to_string()
}

/// An occurrence as `occurrence_id` wrote it. A bare date, as hosts before
/// ADR 0082 kept it, handled that whole day.
pub fn parse_occurrence(text: &str) -> Option<DateTime> {
    if let Ok(slot) = DateTime::strptime("%Y-%m-%dT%H:%M", text) {
        return Some(slot);
    }
    let date: Date = text.parse().ok()?;
    Some(date.at(23, 59, 59, 0))
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
        berlin_schedule(&format!("{m} {h} * * *"))
    }

    fn berlin_schedule(schedule: &str) -> Trigger {
        let parsed = crate::records::cron::parse(schedule).unwrap();
        Trigger {
            hour: parsed.hour,
            minute: parsed.minute,
            times: parsed.times,
            days: parsed.days,
            schedule: schedule.into(),
            timezone: "Europe/Berlin".into(),
            tz: TimeZone::posix("CET-1CEST,M3.5.0,M10.5.0/3").unwrap(),
        }
    }

    fn utc(s: &str) -> Timestamp {
        s.parse().unwrap()
    }

    fn at(y: i16, m: i8, d: i8, h: i8, min: i8) -> DateTime {
        date(y, m, d).at(h, min, 0, 0)
    }

    #[test]
    fn an_ordinary_day_runs_at_the_local_time() {
        let t = berlin("09:00");
        assert_eq!(
            occurrence_at(&t, at(2026, 11, 3, 9, 0)).unwrap(),
            utc("2026-11-03T08:00:00Z")
        );
        assert_eq!(
            occurrence_at(&t, at(2026, 7, 3, 9, 0)).unwrap(),
            utc("2026-07-03T07:00:00Z")
        );
        assert_eq!(occurrence_id(at(2026, 11, 3, 9, 0)), "2026-11-03T09:00");
    }

    #[test]
    fn a_time_in_the_spring_gap_moves_to_the_next_valid_minute() {
        // 2026-03-29: 02:00 CET jumps to 03:00 CEST.
        let t = berlin("02:30");
        let instant = occurrence_at(&t, at(2026, 3, 29, 2, 30)).unwrap();
        assert_eq!(instant, utc("2026-03-29T01:00:00Z"));
        let local = instant.to_zoned(t.tz.clone());
        assert_eq!((local.hour(), local.minute()), (3, 0));
        assert_eq!(
            occurrence_at(&t, at(2026, 3, 30, 2, 30)).unwrap(),
            utc("2026-03-30T00:30:00Z")
        );
    }

    #[test]
    fn a_repeated_time_in_the_autumn_fold_is_one_occurrence() {
        // 2026-10-25: 03:00 CEST goes back to 02:00 CET; 02:30 happens twice.
        let t = berlin("02:30");
        let first = occurrence_at(&t, at(2026, 10, 25, 2, 30)).unwrap();
        assert_eq!(first, utc("2026-10-25T00:30:00Z"));
        let last = Some(at(2026, 10, 24, 2, 30));
        let at_first = decide(&t, first, last).unwrap();
        assert_eq!(at_first.run, Some(at(2026, 10, 25, 2, 30)));
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
                last: at(2026, 11, 2, 9, 0)
            }
        );
        assert_eq!(
            next_due(&t, utc("2026-11-03T07:59:00Z"), None).unwrap(),
            utc("2026-11-03T08:00:00Z")
        );
        let after = decide(&t, utc("2026-11-03T08:01:00Z"), None).unwrap();
        assert_eq!(after.last, at(2026, 11, 3, 9, 0));
        assert_eq!(after.run, None);
    }

    #[test]
    fn a_due_time_runs_once() {
        let t = berlin("09:00");
        let last = Some(at(2026, 11, 2, 9, 0));
        assert_eq!(
            decide(&t, utc("2026-11-03T07:59:59Z"), last).unwrap().run,
            None
        );
        let due = decide(&t, utc("2026-11-03T08:00:00Z"), last).unwrap();
        assert_eq!(due.run, Some(at(2026, 11, 3, 9, 0)));
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
        let d = decide(&t, woke, Some(at(2026, 11, 3, 9, 0))).unwrap();
        assert_eq!(d.run, Some(at(2026, 11, 10, 9, 0)));
        assert_eq!(decide(&t, woke, Some(d.last)).unwrap().run, None);
        // Woken before today's time: yesterday's occurrence is the one catch-up.
        let early = utc("2026-11-10T06:00:00Z");
        let d = decide(&t, early, Some(at(2026, 11, 3, 9, 0))).unwrap();
        assert_eq!(d.run, Some(at(2026, 11, 9, 9, 0)));
        assert_eq!(
            next_due(&t, early, Some(d.last)).unwrap(),
            utc("2026-11-10T08:00:00Z")
        );
    }

    #[test]
    fn a_blocked_occurrence_stays_due_and_reports_a_past_next_due() {
        let t = berlin("09:00");
        let now = utc("2026-11-04T09:00:00Z");
        let last = Some(at(2026, 11, 2, 9, 0));
        assert_eq!(
            next_due(&t, now, last).unwrap(),
            utc("2026-11-03T08:00:00Z")
        );
        assert_eq!(
            decide(&t, now, last).unwrap().run,
            Some(at(2026, 11, 4, 9, 0))
        );
    }

    #[test]
    fn weekdays_skip_the_weekend() {
        let t = berlin_schedule("0 9 * * 1-5");
        // Saturday 2026-11-07 noon: the latest due day is Friday.
        let sat = utc("2026-11-07T11:00:00Z");
        assert_eq!(latest_due(&t, sat).unwrap(), at(2026, 11, 6, 9, 0));
        assert_eq!(
            next_due(&t, sat, Some(at(2026, 11, 6, 9, 0))).unwrap(),
            utc("2026-11-09T08:00:00Z")
        );
        // Monday before 09:00 local: Friday is still the latest.
        let mon = utc("2026-11-09T07:00:00Z");
        assert_eq!(latest_due(&t, mon).unwrap(), at(2026, 11, 6, 9, 0));
        let d = decide(&t, utc("2026-11-09T08:30:00Z"), Some(at(2026, 11, 6, 9, 0))).unwrap();
        assert_eq!(d.run, Some(at(2026, 11, 9, 9, 0)));
    }

    #[test]
    fn a_quarterly_schedule_looks_back_months() {
        let t = berlin_schedule("45 5 1 1,4,7,10 *");
        let now = utc("2026-11-04T12:00:00Z");
        assert_eq!(latest_due(&t, now).unwrap(), at(2026, 10, 1, 5, 45));
        assert_eq!(
            next_due(&t, now, Some(at(2026, 10, 1, 5, 45))).unwrap(),
            utc("2027-01-01T04:45:00Z")
        );
        let leap = berlin_schedule("0 9 29 2 *");
        assert_eq!(latest_due(&leap, now).unwrap(), at(2024, 2, 29, 9, 0));
    }

    #[test]
    fn every_half_hour_in_working_hours_runs_each_slot_once() {
        let t = berlin_schedule("*/30 9-18 * * 1-5");
        // Tuesday 2026-11-03 10:40 local (09:40Z): 10:30 is the latest.
        let now = utc("2026-11-03T09:40:00Z");
        let d = decide(&t, now, Some(at(2026, 11, 3, 10, 0))).unwrap();
        assert_eq!(d.run, Some(at(2026, 11, 3, 10, 30)));
        assert_eq!(decide(&t, now, Some(d.last)).unwrap().run, None);
        assert_eq!(
            next_due(&t, now, Some(d.last)).unwrap(),
            utc("2026-11-03T10:00:00Z")
        );
        // After 18:30 the next is 09:00 the next weekday.
        let evening = utc("2026-11-06T19:00:00Z");
        assert_eq!(latest_due(&t, evening).unwrap(), at(2026, 11, 6, 18, 30));
        assert_eq!(
            next_due(&t, evening, Some(at(2026, 11, 6, 18, 30))).unwrap(),
            utc("2026-11-09T08:00:00Z")
        );
    }

    #[test]
    fn half_hours_through_the_autumn_fold_never_run_twice() {
        let t = berlin_schedule("*/30 * * * *");
        // 02:30 CEST (00:30Z) runs; at 02:30 CET (01:30Z) the wall clock repeats.
        let first = decide(
            &t,
            utc("2026-10-25T00:30:00Z"),
            Some(at(2026, 10, 25, 2, 0)),
        )
        .unwrap();
        assert_eq!(first.run, Some(at(2026, 10, 25, 2, 30)));
        let repeat = decide(&t, utc("2026-10-25T01:30:00Z"), Some(first.last)).unwrap();
        assert_eq!(repeat.run, None);
        // 03:00 CET is new.
        let next = decide(&t, utc("2026-10-25T02:00:00Z"), Some(first.last)).unwrap();
        assert_eq!(next.run, Some(at(2026, 10, 25, 3, 0)));
    }

    #[test]
    fn an_old_bare_date_counts_as_that_whole_day_handled() {
        let t = berlin("09:00");
        let last = parse_occurrence("2026-11-03");
        assert_eq!(
            last,
            Some(at(2026, 11, 3, 23, 59).with().second(59).build().unwrap())
        );
        assert_eq!(
            decide(&t, utc("2026-11-03T08:30:00Z"), last).unwrap().run,
            None
        );
        assert_eq!(
            decide(&t, utc("2026-11-04T08:30:00Z"), last).unwrap().run,
            Some(at(2026, 11, 4, 9, 0))
        );
        assert_eq!(
            parse_occurrence("2026-11-03T09:00"),
            Some(at(2026, 11, 3, 9, 0))
        );
        assert_eq!(parse_occurrence("now-5"), None);
    }
}
