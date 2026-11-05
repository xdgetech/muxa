//! A UTC time, shown without a time zone database. Epoch seconds throughout.

use std::time::{SystemTime, UNIX_EPOCH};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
    fn format_shows_utc_to_the_minute() {
        assert_eq!(format(0), "1970-01-01T00:00Z");
        assert_eq!(format(1_709_209_800), "2024-02-29T12:30Z");
        assert_eq!(format(1_767_340_830), "2026-01-02T08:00Z");
    }
}
