//! A Routine's `schedule` (ADR 0035 section 3): five cron fields with one
//! time a day. Parsing and day matching only; `routines/schedule.rs` turns
//! a matching day into an instant.
use jiff::civil::Date;

/// Day-of-month, month and day-of-week sets. A field written `*` has every bit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Days {
    /// Bit `d` for day `d`, 1 to 31.
    dom: u32,
    /// Bit `m` for month `m`, 1 to 12.
    months: u16,
    /// Bit `d` for days since Sunday, 0 to 6.
    dow: u8,
}

const ALL_DOM: u32 = 0xFFFF_FFFE;
const ALL_DOW: u8 = 0x7F;
/// Days in each month, February in a leap year.
const MONTH_DAYS: [u32; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

impl Days {
    /// Day-of-month and day-of-week are never both restricted, so AND is
    /// what cron's OR rule would give.
    pub fn matches(&self, date: Date) -> bool {
        let dow = date.weekday().to_sunday_zero_offset();
        self.dom & (1 << date.day()) != 0
            && self.months & (1 << date.month()) != 0
            && self.dow & (1 << dow) != 0
    }

    /// Some day matches within any four years.
    fn can_match(&self) -> bool {
        if self.dom == ALL_DOM {
            return self.months != 0 && self.dow != 0;
        }
        MONTH_DAYS.iter().enumerate().any(|(i, &last)| {
            self.months & (1 << (i + 1)) != 0 && (1..=last).any(|d| self.dom & (1 << d) != 0)
        })
    }
}

/// A parsed `schedule`: the daily time and the days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub hour: i8,
    pub minute: i8,
    pub days: Days,
}

/// `minute hour day-of-month month day-of-week`. Errors never quote the value.
pub fn parse(text: &str) -> Result<Schedule, String> {
    let fields: Vec<&str> = text.split_whitespace().collect();
    let [minute, hour, dom, month, dow] = fields[..] else {
        return Err(
            "schedule must have five fields: minute hour day-of-month month day-of-week".into(),
        );
    };
    let minute = number(minute, 59).ok_or("schedule minute must be one number, 0 to 59")?;
    let hour = number(hour, 23).ok_or("schedule hour must be one number, 0 to 23")?;
    let dom_bits =
        set(dom, 1, 31).ok_or("schedule day-of-month must be *, n, a-b or a list, 1 to 31")?;
    let month_bits =
        set(month, 1, 12).ok_or("schedule month must be *, n, a-b or a list, 1 to 12")?;
    let dow_bits =
        set(dow, 0, 7).ok_or("schedule day-of-week must be *, n, a-b or a list, 0 to 7")?;
    // 7 is Sunday too.
    let dow_bits = ((dow_bits | (dow_bits >> 7)) & u64::from(ALL_DOW)) as u8;
    if dom != "*" && dow != "*" {
        return Err(
            "schedule cannot restrict both day-of-month and day-of-week; use two routines".into(),
        );
    }
    let days = Days {
        dom: dom_bits as u32,
        months: month_bits as u16,
        dow: dow_bits,
    };
    if !days.can_match() {
        return Err("schedule matches no day".into());
    }
    Ok(Schedule {
        hour: hour as i8,
        minute: minute as i8,
        days,
    })
}

/// The schedule that `at = "HH:MM"` stands for.
pub fn daily(hour: i8, minute: i8) -> String {
    format!("{minute} {hour} * * *")
}

fn number(s: &str, max: u32) -> Option<u32> {
    if s.is_empty() || s.len() > 2 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok().filter(|n| *n <= max)
}

/// Bit `n` set for each `n` in the field.
fn set(field: &str, min: u32, max: u32) -> Option<u64> {
    if field == "*" {
        return Some((min..=max).fold(0, |bits, n| bits | 1 << n));
    }
    let mut bits = 0u64;
    for item in field.split(',') {
        let (a, b) = match item.split_once('-') {
            Some((a, b)) => (number(a, max)?, number(b, max)?),
            None => {
                let n = number(item, max)?;
                (n, n)
            }
        };
        if a < min || a > b {
            return None;
        }
        bits |= (a..=b).fold(0, |acc, n| acc | 1 << n);
    }
    Some(bits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn orch_schedules_parse() {
        for s in [
            "0 8 * * 0,2-6",
            "15 5 1 * *",
            "45 5 1 1,4,7,10 *",
            "30 7 * * 1-5",
            "0 20 * * 7",
        ] {
            assert!(parse(s).is_ok(), "refused {s}");
        }
    }

    #[test]
    fn days_match_as_cron_does() {
        let weekdays = parse("0 8 * * 1-5").unwrap();
        assert_eq!((weekdays.hour, weekdays.minute), (8, 0));
        // 2026-11-02 is a Monday.
        assert!(weekdays.days.matches(date(2026, 11, 2)));
        assert!(weekdays.days.matches(date(2026, 11, 6)));
        assert!(!weekdays.days.matches(date(2026, 11, 7)));
        let sunday = parse("0 20 * * 7").unwrap();
        assert!(sunday.days.matches(date(2026, 11, 8)));
        assert_eq!(sunday.days, parse("0 20 * * 0").unwrap().days);
        let quarterly = parse("45 5 1 1,4,7,10 *").unwrap();
        assert!(quarterly.days.matches(date(2027, 1, 1)));
        assert!(!quarterly.days.matches(date(2026, 12, 1)));
        assert!(!quarterly.days.matches(date(2027, 1, 2)));
        assert!(parse("0 8 * * *").unwrap().days.matches(date(2028, 2, 29)));
    }

    #[test]
    fn bad_schedules_are_refused() {
        for s in [
            "",
            "0 8 * *",
            "0 8 * * * *",
            "0,30 8 * * *",
            "*/5 8 * * *",
            "0 8-9 * * *",
            "60 8 * * *",
            "0 24 * * *",
            "0 8 0 * *",
            "0 8 32 * *",
            "0 8 * 13 *",
            "0 8 * 0 *",
            "0 8 * * 8",
            "0 8 * * mon",
            "0 8 5-3 * *",
            "0 8 1 * 1",
            "0 8 31 2 *",
            "0 8 30,31 2 *",
            "0 8 1,,2 * *",
            "0 8 -1 * *",
            "0 8 001 * *",
        ] {
            assert!(parse(s).is_err(), "accepted {s:?}");
        }
        assert!(parse("0 8 29 2 *").is_ok());
    }

    #[test]
    fn at_is_a_daily_schedule() {
        let s = parse(&daily(9, 5)).unwrap();
        assert_eq!((s.hour, s.minute), (9, 5));
        assert_eq!(s.days, parse("0 0 * * *").unwrap().days);
    }
}
