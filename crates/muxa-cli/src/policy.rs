//! `policy` and `forget`: what the user told the overall assistant (ADR 0050),
//! behind the host's `work` level 11. Policy texts and reasons go to stdout only.

use muxa_protocol::{
    Correction, CorrectionKind, Hypothesis, Message, Policy, PolicyAction, Request, WORK_POLICIES,
    work_level,
};
use serde_json::json;

use crate::assist::plain;
use crate::conn::Conn;
use crate::items::wire;
use crate::work::WAIT;

/// The host's limits on a policy's text and on a reason, in bytes.
const TEXT_MAX: usize = 500;

pub fn require(conn: &Conn) -> Result<(), String> {
    if work_level(conn.capabilities()) >= WORK_POLICIES {
        Ok(())
    } else {
        Err("this host keeps no policies; update muxa-bridge on the host".into())
    }
}

/// What `policy add` and `policy edit` send. Times are epoch seconds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub text: Option<String>,
    pub scope: Option<String>,
    pub from: Option<u64>,
    pub until: Option<u64>,
}

/// The flags of `policy add` and `edit`, checked before any request.
/// `--until week` ends on this week's Sunday, 23:59:59 in the zone.
pub fn fields(
    text: Option<String>,
    scope: Option<String>,
    from: Option<&str>,
    until: Option<&str>,
    timezone: Option<&str>,
) -> Result<Fields, String> {
    if let Some(t) = &text {
        policy_text(t)?;
    }
    if scope.as_deref().is_some_and(|s| s.trim().is_empty()) {
        return Err("--scope needs all or a workspace's name".into());
    }
    let zone = match (from, until) {
        (None, None) if timezone.is_some() => {
            return Err("--timezone requires --from or --until".into());
        }
        (None, None) => String::new(),
        _ => crate::work::zone_name(timezone)?,
    };
    let from = from
        .map(|at| crate::work::epoch("--from", at, &zone))
        .transpose()?;
    let until = match until {
        Some("week") => Some(week_end(&zone, crate::when::now())?),
        Some(at) => Some(crate::work::epoch("--until", at, &zone)?),
        None => None,
    };
    if let (Some(f), Some(u)) = (from, until)
        && u <= f
    {
        return Err("--until must be after --from".into());
    }
    Ok(Fields {
        text,
        scope,
        from,
        until,
    })
}

/// One paragraph of 1 to 500 bytes, as the host takes it.
fn policy_text(text: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.len() > TEXT_MAX {
        return Err(format!("a policy is 1 to {TEXT_MAX} bytes"));
    }
    if text.chars().any(char::is_control) {
        return Err("a policy is one paragraph: no line breaks".into());
    }
    Ok(())
}

/// A reason: at most 500 bytes, line breaks and tabs the only control characters.
pub fn reason(text: &str) -> Result<(), String> {
    if text.len() > TEXT_MAX {
        return Err(format!("--reason has at most {TEXT_MAX} bytes"));
    }
    if text
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("--reason takes no control characters".into());
    }
    Ok(())
}

/// The end of the week that holds `now`: Sunday, 23:59:59 in `zone`.
fn week_end(zone: &str, now: u64) -> Result<u64, String> {
    let tz = crate::work::time_zone(zone)?;
    let now = i64::try_from(now)
        .ok()
        .and_then(|s| jiff::Timestamp::from_second(s).ok())
        .ok_or("the system clock is out of range")?;
    let today = now.to_zoned(tz.clone()).date();
    let days = (7 - i64::from(today.weekday().to_sunday_zero_offset())) % 7;
    today
        .checked_add(jiff::Span::new().days(days))
        .and_then(|sunday| sunday.at(23, 59, 59, 0).to_zoned(tz))
        .ok()
        .and_then(|end| u64::try_from(end.timestamp().as_second()).ok())
        .ok_or_else(|| "cannot find the end of this week".into())
}

/// `policy`: the policies, active then expired, the hypotheses and the
/// recent corrections.
pub async fn list(conn: &mut Conn, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let listing = crate::work::fetch(conn).await?;
    if as_json {
        println!(
            "{}",
            json!({"policies": listing.policies, "hypotheses": listing.hypotheses,
                "corrections": listing.corrections})
        );
        return Ok(String::new());
    }
    print!(
        "{}",
        memory_text(
            &listing.policies,
            &listing.hypotheses,
            &listing.corrections,
            crate::when::now()
        )
    );
    Ok(String::new())
}

pub async fn add(conn: &mut Conn, f: Fields, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let text = f.text.ok_or("policy add needs its text")?;
    let messages = conn
        .call(
            |id| Request::AddPolicy {
                id,
                text,
                scope: f.scope.unwrap_or_else(|| "all".into()),
                from: f.from,
                until: f.until,
            },
            WAIT,
        )
        .await?;
    print_saved(&messages, as_json)
}

pub async fn edit(
    conn: &mut Conn,
    policy: &str,
    f: Fields,
    as_json: bool,
) -> Result<String, String> {
    require(conn)?;
    if f == Fields::default() {
        return Err("policy edit needs --text, --scope, --from or --until".into());
    }
    let rev = rev_of(conn, policy).await?;
    let messages = conn
        .call(
            |id| Request::UpdatePolicy {
                id,
                policy: policy.to_owned(),
                rev,
                action: PolicyAction::Edit,
                text: f.text,
                scope: f.scope,
                from: f.from,
                until: f.until,
            },
            WAIT,
        )
        .await?;
    print_saved(&messages, as_json)
}

pub async fn delete(conn: &mut Conn, policy: &str, as_json: bool) -> Result<String, String> {
    require(conn)?;
    let rev = rev_of(conn, policy).await?;
    conn.call(
        |id| Request::UpdatePolicy {
            id,
            policy: policy.to_owned(),
            rev,
            action: PolicyAction::Delete,
            text: None,
            scope: None,
            from: None,
            until: None,
        },
        WAIT,
    )
    .await?;
    if as_json {
        println!("{}", json!({"deleted": policy}));
    } else {
        println!("deleted {policy}");
    }
    Ok(String::new())
}

/// `forget ID`: a correction `x-…` or a hypothesis `h-…`.
pub async fn forget(conn: &mut Conn, record: &str, as_json: bool) -> Result<String, String> {
    require(conn)?;
    conn.call(
        |id| Request::Forget {
            id,
            record: record.to_owned(),
        },
        WAIT,
    )
    .await?;
    if as_json {
        println!("{}", json!({"forgotten": record}));
    } else {
        println!("forgot {record}");
    }
    Ok(String::new())
}

async fn rev_of(conn: &mut Conn, policy: &str) -> Result<u64, String> {
    crate::work::fetch(conn)
        .await?
        .policies
        .iter()
        .find(|p| p.id == policy)
        .map(|p| p.rev)
        .ok_or_else(|| format!("no policy {policy}"))
}

fn print_saved(messages: &[Message], as_json: bool) -> Result<String, String> {
    let entry = messages
        .iter()
        .find_map(|m| match m {
            Message::PolicySaved { entry, .. } => Some(entry),
            _ => None,
        })
        .ok_or("the bridge saved no policy")?;
    if as_json {
        println!("{}", json!(entry));
    } else {
        println!("{}", policy_line(entry, crate::when::now()));
    }
    Ok(String::new())
}

/// `y-0a1b2c3d  all  until 2025-11-15T14:59Z  This week, recruiting comes first.`
pub fn policy_line(p: &Policy, now: u64) -> String {
    let mut when = String::new();
    if let Some(from) = p.from {
        when.push_str(&format!("  from {}", crate::when::format(from)));
    }
    if let Some(until) = p.until {
        when.push_str(&format!("  until {}", crate::when::format(until)));
    }
    let source = if p.source == "user" {
        String::new()
    } else {
        format!("  confirmed {}", p.source)
    };
    let expired = if expired(p, now) { "  expired" } else { "" };
    format!(
        "{}  {}{when}{source}{expired}  {}",
        p.id,
        plain(&p.scope),
        plain(&p.text)
    )
}

fn expired(p: &Policy, now: u64) -> bool {
    p.until.is_some_and(|u| u <= now)
}

/// `h-4e5f6a7b  proposed  low  all  2 corrections  Work due this week comes first.`
pub fn hypothesis_line(h: &Hypothesis) -> String {
    format!(
        "{}  {}  {}  {}  {} correction{}  {}",
        h.id,
        wire(json!(h.state)),
        wire(json!(h.confidence)),
        plain(&h.scope),
        h.evidence.len(),
        if h.evidence.len() == 1 { "" } else { "s" },
        plain(&h.text)
    )
}

/// `x-3f4a5b6c  dismissed  i-1a2b3c4d  2025-11-10T11:43Z  Waiting for the vendor.`
pub fn correction_line(c: &Correction) -> String {
    let row = match c.kind {
        CorrectionKind::Reordered => c.after.join(" "),
        _ => c.row.clone().unwrap_or_default(),
    };
    let reason = c
        .reason
        .as_deref()
        .map(|r| format!("  {}", plain(r).replace('\n', " ")))
        .unwrap_or_default();
    format!(
        "{}  {}  {row}  {}{reason}",
        c.id,
        wire(json!(c.kind)),
        crate::when::format(c.at)
    )
}

fn memory_text(
    policies: &[Policy],
    hypotheses: &[Hypothesis],
    corrections: &[Correction],
    now: u64,
) -> String {
    if policies.is_empty() && hypotheses.is_empty() && corrections.is_empty() {
        return "no policies, hypotheses or corrections; muxa-cli policy add TEXT keeps one\n"
            .into();
    }
    let (old, active): (Vec<&Policy>, Vec<&Policy>) =
        policies.iter().partition(|p| expired(p, now));
    let mut out = String::new();
    let mut section = |heading: &str, lines: Vec<String>| {
        if !lines.is_empty() {
            out.push_str(&format!("{heading}\n"));
            for line in lines {
                out.push_str(&format!("{line}\n"));
            }
        }
    };
    section(
        "Policies",
        active.iter().map(|p| policy_line(p, now)).collect(),
    );
    section("Expired", old.iter().map(|p| policy_line(p, now)).collect());
    section(
        "Hypotheses",
        hypotheses.iter().map(hypothesis_line).collect(),
    );
    section(
        "Recent corrections",
        corrections.iter().map(correction_line).collect(),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::{Confidence, HypothesisState};

    fn policy(id: &str, until: Option<u64>) -> Policy {
        Policy {
            id: id.into(),
            rev: 1,
            text: "Recruiting \u{1b}[31mfirst.".into(),
            scope: "all".into(),
            from: None,
            until,
            source: "user".into(),
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn the_list_shows_active_then_expired_then_hypotheses_and_corrections() {
        let hypothesis = Hypothesis {
            id: "h-4e5f6a7b".into(),
            text: "Work due this week comes first.".into(),
            scope: "all".into(),
            evidence: vec!["x-0a1b2c3d".into()],
            confidence: Confidence::Low,
            state: HypothesisState::Proposed,
            plan: "p-0a1b2c3d".into(),
            expires_at: 0,
            at: 0,
        };
        let correction = Correction {
            id: "x-3f4a5b6c".into(),
            kind: CorrectionKind::Dismissed,
            at: 1_762_775_000,
            plan: "p-0a1b2c3d".into(),
            row: Some("i-1a2b3c4d".into()),
            in_top: true,
            workspace: None,
            before: Vec::new(),
            after: Vec::new(),
            reason: Some("Waiting\nfor the vendor.".into()),
        };
        let mut sourced = policy("y-2c3d4e5f", None);
        sourced.source = "h-3c4d5e6f".into();
        let text = memory_text(
            &[
                policy("y-0a1b2c3d", Some(200)),
                sourced,
                policy("y-3d4e5f6a", Some(50)),
            ],
            &[hypothesis],
            &[correction],
            100,
        );
        assert_eq!(
            text,
            "Policies\n\
             y-0a1b2c3d  all  until 1970-01-01T00:03Z  Recruiting [31mfirst.\n\
             y-2c3d4e5f  all  confirmed h-3c4d5e6f  Recruiting [31mfirst.\n\
             Expired\n\
             y-3d4e5f6a  all  until 1970-01-01T00:00Z  expired  Recruiting [31mfirst.\n\
             Hypotheses\n\
             h-4e5f6a7b  proposed  low  all  1 correction  Work due this week comes first.\n\
             Recent corrections\n\
             x-3f4a5b6c  dismissed  i-1a2b3c4d  2025-11-10T11:43Z  Waiting for the vendor.\n"
        );
        assert!(memory_text(&[], &[], &[], 0).starts_with("no policies"));
    }

    #[test]
    fn fields_are_checked_before_any_request() {
        let f = |text: &str| fields(Some(text.into()), None, None, None, None);
        assert!(f("Recruiting first.").is_ok());
        assert!(f("").is_err());
        assert!(f("two\nlines").is_err());
        assert!(f(&"x".repeat(501)).is_err());
        let utc = Some("UTC");
        let span = |from, until| fields(None, None, from, until, utc);
        assert_eq!(
            span(Some("2025-11-10 09:00"), Some("2025-11-16T14:59:59Z")).unwrap(),
            Fields {
                from: Some(1_762_765_200),
                until: Some(1_763_305_199),
                ..Fields::default()
            }
        );
        assert!(span(Some("2025-11-16 09:00"), Some("2025-11-10 09:00")).is_err());
        assert!(span(None, Some("Sunday")).is_err());
        assert!(fields(None, None, None, None, utc).is_err());
        assert!(fields(None, Some(" ".into()), None, None, None).is_err());
        assert!(reason("Waiting\tfor the vendor.\n").is_ok());
        assert!(reason("\u{1b}[31m").is_err());
        assert!(reason(&"x".repeat(501)).is_err());
    }

    #[test]
    fn this_week_ends_on_sunday_night_in_the_zone() {
        // Monday 2025-11-10 00:30 in Tokyo is still Sunday in UTC.
        let monday = 1_762_702_200;
        assert_eq!(week_end("Asia/Tokyo", monday).unwrap(), 1_763_305_199);
        assert_eq!(week_end("UTC", monday).unwrap(), 1_762_732_799);
    }
}
