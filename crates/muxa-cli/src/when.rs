//! `task later --until`: a UTC time, read and shown without a time zone
//! database. Epoch seconds throughout.

use std::time::{SystemTime, UNIX_EPOCH};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `2026-11-04T09:00Z`, `2026-11-04T09:00:30Z`, `+30m`, `+2h`, `+1d`, or epoch seconds.
pub fn parse(text: &str, now: u64) -> Result<u64, String> {
    let bad =
        || format!("--until {text:?}: use 2026-11-04T09:00Z (UTC), +2h, +1d or epoch seconds");
    if let Some(rel) = text.strip_prefix('+') {
        let (n, unit) = rel.split_at(rel.len().saturating_sub(1));
        let n: u64 = n.parse().map_err(|_| bad())?;
        let unit = match unit {
            "m" => MINUTE,
            "h" => HOUR,
            "d" => DAY,
            _ => return Err(bad()),
        };
        return n
            .checked_mul(unit)
            .and_then(|s| now.checked_add(s))
            .ok_or_else(bad);
    }
    if !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()) {
        return text.parse().map_err(|_| bad());
    }
    utc(text).ok_or_else(bad)
}

/// `YYYY-MM-DDTHH:MM[:SS]Z`.
fn utc(text: &str) -> Option<u64> {
    let (date, time) = text.strip_suffix('Z')?.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<u64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.split(':').map(|p| p.parse::<u64>().ok());
    let (h, min, s) = (t.next()??, t.next()??, t.next().unwrap_or(Some(0))?);
    if d.next().is_some() || t.next().is_some() || y < 1970 || !(1..=12).contains(&m) {
        return None;
    }
    if !(1..=days_in_month(y, m)).contains(&day) || h > 23 || min > 59 || s > 59 {
        return None;
    }
    Some(days_from_civil(y, m, day) * DAY + h * HOUR + min * MINUTE + s)
}

/// `2026-11-04T09:00Z`, seconds left out.
pub fn format(epoch: u64) -> String {
    let (y, m, d) = civil_from_days(epoch / DAY);
    let rest = epoch % DAY;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}Z",
        rest / HOUR,
        rest % HOUR / MINUTE
    )
}

fn is_leap(y: u64) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

fn days_in_month(y: u64, m: u64) -> u64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01; Howard Hinnant's algorithm, for years from 1970.
fn days_from_civil(y: u64, m: u64, d: u64) -> u64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + u64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_times_relative_times_and_epoch_seconds() {
        let now = 1_767_300_000;
        assert_eq!(parse("1970-01-01T00:00Z", now), Ok(0));
        assert_eq!(parse("2026-01-01T00:00:00Z", now), Ok(1_767_225_600));
        assert_eq!(parse("2024-02-29T12:30Z", now), Ok(1_709_209_800));
        assert_eq!(parse("+30m", now), Ok(now + 1800));
        assert_eq!(parse("+2h", now), Ok(now + 7200));
        assert_eq!(parse("+1d", now), Ok(now + 86_400));
        assert_eq!(parse("1767340800", now), Ok(1_767_340_800));
        for bad in [
            "",
            "+",
            "+2w",
            "+h",
            "tomorrow",
            "2026-11-04 09:00",
            "2026-11-04T09:00",
            "2026-02-29T09:00Z",
            "2026-13-01T09:00Z",
            "2026-11-04T24:00Z",
            "2026-11-04T09:00:00:00Z",
        ] {
            assert!(parse(bad, now).is_err(), "{bad}");
        }
    }

    #[test]
    fn format_is_the_inverse_to_the_minute() {
        for t in [0, 1_709_209_800, 1_767_225_600, 4_102_444_800] {
            assert_eq!(parse(&format(t), 0), Ok(t));
        }
        assert_eq!(format(1_767_340_830), "2026-01-02T08:00Z");
    }
}
