//! Epoch seconds as UTC calendar dates, without a time zone database.

/// (year, month, day) of `secs` in UTC.
pub fn date(secs: u64) -> (i64, u32, u32) {
    let days = (secs / 86_400) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// `2026-11-02 14:05 UTC`.
pub fn stamp(secs: u64) -> String {
    let (y, m, d) = date(secs);
    let minutes = secs % 86_400 / 60;
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02} UTC",
        minutes / 60,
        minutes % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_stamps_are_utc() {
        assert_eq!(date(0), (1970, 1, 1));
        assert_eq!(date(951_782_400), (2000, 2, 29));
        assert_eq!(stamp(1_767_225_600 + 3_723), "2026-01-01 01:02 UTC");
    }
}
