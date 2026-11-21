//! Reads one Capture line into fields (ADR 0066 decision 6). The client
//! shows them as chips, and the user can change them before adding.

use jiff::civil::{Date, Weekday};
use muxa_protocol::v2::{Capture, ItemKind};

use crate::book::InboxError;

/// - A first word `reply`, `read` or `decide` sets the kind; else To do.
/// - `#name` sets the Workspace only when one has that name; else it is text.
/// - `due <day>`: `today`, `tomorrow`, a weekday (the next one, today
///   included), `M/D` (the next one) or `YYYY-MM-DD`. `today` is the
///   device's local date.
pub fn parse(text: &str, workspaces: &[String], today: &str) -> Result<Capture, InboxError> {
    let today: Date = today
        .parse()
        .map_err(|_| InboxError::Invalid(format!("{today:?} is not a date (YYYY-MM-DD)")))?;
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let kind = match words.first().map(|w| w.to_lowercase()).as_deref() {
        Some("reply") => Some(ItemKind::Reply),
        Some("read") => Some(ItemKind::Read),
        Some("decide") => Some(ItemKind::Decide),
        _ => None,
    };
    if kind.is_some() {
        words.remove(0);
    }
    let mut due = None;
    if let Some(at) = words
        .iter()
        .position(|w| w.eq_ignore_ascii_case("due"))
        .filter(|at| at + 1 < words.len())
        && let Some(date) = day(words[at + 1], today)
    {
        due = Some(date.to_string());
        words.drain(at..=at + 1);
    }
    let mut workspace = None;
    words.retain(|w| {
        let Some(name) = w.strip_prefix('#') else {
            return true;
        };
        match workspaces.iter().find(|ws| ws.eq_ignore_ascii_case(name)) {
            Some(ws) if workspace.is_none() => {
                workspace = Some(ws.clone());
                false
            }
            _ => true,
        }
    });
    let title = words.join(" ");
    if title.is_empty() {
        return Err(InboxError::Invalid("the Capture line has no title".into()));
    }
    Ok(Capture {
        kind: kind.unwrap_or(ItemKind::Todo),
        title,
        workspace,
        due,
    })
}

fn day(word: &str, today: Date) -> Option<Date> {
    let w = word.to_lowercase();
    match w.as_str() {
        "today" => return Some(today),
        "tomorrow" => return today.tomorrow().ok(),
        _ => {}
    }
    if let Some(wd) = weekday(&w) {
        let ahead =
            (wd.to_monday_zero_offset() - today.weekday().to_monday_zero_offset()).rem_euclid(7);
        return today
            .checked_add(jiff::Span::new().days(i64::from(ahead)))
            .ok();
    }
    if let Ok(date) = w.parse::<Date>() {
        return Some(date);
    }
    let (m, d) = w.split_once('/')?;
    let (m, d) = (m.parse::<i8>().ok()?, d.parse::<i8>().ok()?);
    let this_year = Date::new(today.year(), m, d).ok()?;
    if this_year >= today {
        Some(this_year)
    } else {
        Date::new(today.year() + 1, m, d).ok()
    }
}

fn weekday(w: &str) -> Option<Weekday> {
    const DAYS: [(&str, Weekday); 7] = [
        ("mon", Weekday::Monday),
        ("tue", Weekday::Tuesday),
        ("wed", Weekday::Wednesday),
        ("thu", Weekday::Thursday),
        ("fri", Weekday::Friday),
        ("sat", Weekday::Saturday),
        ("sun", Weekday::Sunday),
    ];
    DAYS.iter()
        .find(|(short, wd)| w == *short || w == format!("{wd:?}").to_lowercase())
        .map(|(_, wd)| *wd)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A Wednesday.
    const TODAY: &str = "2026-11-25";

    fn p(text: &str) -> Capture {
        parse(text, &["Personal".into(), "lisk".into()], TODAY).unwrap()
    }

    #[test]
    fn the_first_word_sets_the_kind() {
        assert_eq!(p("Reply to Anna").kind, ItemKind::Reply);
        assert_eq!(p("Reply to Anna").title, "to Anna");
        assert_eq!(p("read the RFC").kind, ItemKind::Read);
        assert_eq!(p("decide on vendor").kind, ItemKind::Decide);
        // Only the first word.
        let c = p("buy milk, then read");
        assert_eq!(
            (c.kind, c.title.as_str()),
            (ItemKind::Todo, "buy milk, then read")
        );
    }

    #[test]
    fn a_tag_is_a_workspace_only_when_one_has_that_name() {
        let c = p("call bank #personal #urgent");
        assert_eq!(c.workspace.as_deref(), Some("Personal"));
        assert_eq!(c.title, "call bank #urgent");
    }

    #[test]
    fn due_days_are_read_in_the_devices_calendar() {
        assert_eq!(p("pay rent due today").due.as_deref(), Some("2026-11-25"));
        assert_eq!(
            p("pay rent due tomorrow").due.as_deref(),
            Some("2026-11-26")
        );
        assert_eq!(p("pay rent due fri").due.as_deref(), Some("2026-11-27"));
        assert_eq!(
            p("pay rent due Wednesday").due.as_deref(),
            Some("2026-11-25")
        );
        assert_eq!(p("pay rent due tue").due.as_deref(), Some("2026-12-01"));
        assert_eq!(p("pay rent due 12/1").due.as_deref(), Some("2026-12-01"));
        assert_eq!(p("pay rent due 1/5").due.as_deref(), Some("2027-01-05"));
        assert_eq!(
            p("pay rent due 2027-02-03").due.as_deref(),
            Some("2027-02-03")
        );
        assert_eq!(p("pay rent due 2027-02-03").title, "pay rent");
        // Not a day: stays text.
        let c = p("talk about due diligence");
        assert_eq!(
            (c.due, c.title.as_str()),
            (None, "talk about due diligence")
        );
    }

    #[test]
    fn a_line_with_only_fields_has_no_title() {
        assert!(parse("reply #lisk due fri", &["lisk".into()], TODAY).is_err());
    }
}
