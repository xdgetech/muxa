//! `assist`: ask a workspace's assistant about a Work item, cancel a request,
//! and show its latest Result (ADR 0046). Purposes and Results go to stdout
//! only, never to stderr or a log.

use muxa_protocol::{AssistResult, AssistScope, AssistantRequest, Message, Request, WorkItem};
use serde_json::json;

use crate::conn::Conn;
use crate::work::{self, WAIT};

pub fn scope(name: &str) -> Option<AssistScope> {
    serde_json::from_value(json!(name)).ok()
}

/// `assist WORK …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub work: String,
    pub scope: AssistScope,
    pub workspace: Option<String>,
    pub purpose: Option<String>,
}

pub async fn ask(conn: &mut Conn, a: Ask, as_json: bool) -> Result<String, String> {
    let rev = work::rev_of(conn, &a.work).await?;
    let messages = conn
        .call(
            |id| Request::RequestAssistant {
                id,
                work: a.work,
                rev,
                scope: a.scope,
                workspace: a.workspace,
                purpose: a.purpose,
            },
            WAIT,
        )
        .await?;
    print_saved(&messages, as_json)
}

/// `assist cancel REQUEST`: cancels the request's current version.
pub async fn cancel(conn: &mut Conn, request: &str, as_json: bool) -> Result<String, String> {
    let version = work::fetch(conn)
        .await?
        .requests
        .iter()
        .find(|q| q.id == request)
        .map(|q| q.version)
        .ok_or_else(|| format!("no assistant request {request}"))?;
    let messages = conn
        .call(
            |id| Request::CancelRequest {
                id,
                request: request.to_owned(),
                version,
            },
            WAIT,
        )
        .await?;
    print_saved(&messages, as_json)
}

fn print_saved(messages: &[Message], as_json: bool) -> Result<String, String> {
    let (entry, request) = messages
        .iter()
        .find_map(|m| match m {
            Message::WorkSaved { entry, request, .. } => Some((entry, request.as_deref())),
            _ => None,
        })
        .ok_or("the bridge saved no Work item")?;
    let request = request.ok_or("the bridge saved no assistant request")?;
    if as_json {
        println!("{}", json!({"work": entry, "request": request}));
    } else {
        println!("{}", request_line(request));
    }
    Ok(String::new())
}

/// `assist show WORK|REQUEST`: the newest Result, of the request or of any
/// of the item's requests.
pub async fn show(conn: &mut Conn, target: &str, as_json: bool) -> Result<String, String> {
    let listing = work::fetch(conn).await?;
    let request = pick(&listing.work, &listing.requests, target)?;
    if as_json {
        println!("{}", json!(request));
        return Ok(String::new());
    }
    println!("{}", request_line(request));
    match request.results.first() {
        Some(r) => print!("{}", result_text(r, request.version, crate::when::now())),
        None => println!("no Result yet"),
    }
    Ok(String::new())
}

fn pick<'a>(
    items: &[WorkItem],
    requests: &'a [AssistantRequest],
    target: &str,
) -> Result<&'a AssistantRequest, String> {
    if let Some(q) = requests.iter().find(|q| q.id == target) {
        return Ok(q);
    }
    let item = items
        .iter()
        .find(|w| w.id == target)
        .ok_or_else(|| format!("no Work item or assistant request {target}"))?;
    let mine: Vec<&AssistantRequest> = requests
        .iter()
        .filter(|q| item.requests.contains(&q.id))
        .collect();
    // The newest Result wins; without any, the newest request.
    mine.iter()
        .filter_map(|q| q.results.first().map(|r| (r.at, *q)))
        .max_by_key(|(at, _)| *at)
        .map(|(_, q)| q)
        .or_else(|| work::latest_request(item, requests))
        .ok_or_else(|| format!("{target} has no assistant request"))
}

/// `q-… v2 research  processing  [work]`.
pub fn request_line(q: &AssistantRequest) -> String {
    format!(
        "{} v{} {}  {}  [{}]",
        q.id,
        q.version,
        crate::items::wire(json!(q.scope)),
        crate::items::wire(json!(q.delivery)),
        plain(&q.workspace)
    )
}

/// One Result as plain text, every section only when it has something.
fn result_text(r: &AssistResult, current: u64, now: u64) -> String {
    let stale = if r.stale || r.version < current {
        " (stale: an older version)"
    } else {
        ""
    };
    let mut out = format!(
        "Result v{}, {} ago{stale}\n{}\n",
        r.version,
        crate::items::age(now.saturating_sub(r.at)),
        plain(&r.summary)
    );
    if !r.findings.is_empty() {
        out.push_str("\nFindings\n");
        for (n, f) in r.findings.iter().enumerate() {
            out.push_str(&format!("  {}. {}\n", n + 1, plain(&f.text)));
            for s in &f.sources {
                let place = [s.url.as_deref(), s.path.as_deref()]
                    .into_iter()
                    .flatten()
                    .map(plain)
                    .collect::<Vec<_>>()
                    .join("  ");
                let title = s
                    .title
                    .as_deref()
                    .map(|t| format!("{}  ", plain(t)))
                    .unwrap_or_default();
                out.push_str(&format!("     - {title}{place}\n"));
            }
        }
    }
    section(&mut out, "Open questions", &r.open_questions);
    section(&mut out, "Next", &r.next);
    section(&mut out, "Files", &r.files);
    let duplicates: Vec<String> = r
        .possible_duplicates
        .iter()
        .map(|d| format!("{}  {}", d.work, d.reason))
        .collect();
    section(&mut out, "Possible duplicates", &duplicates);
    out
}

fn section(out: &mut String, title: &str, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    out.push_str(&format!("\n{title}\n"));
    for l in lines {
        out.push_str(&format!("  - {}\n", plain(l)));
    }
}

/// An agent's or the user's text without terminal control characters; line
/// breaks and tabs stay.
pub fn plain(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use muxa_protocol::{Delivery, Finding, PossibleDuplicate, ResultSource};

    fn request(results: Vec<AssistResult>) -> AssistantRequest {
        AssistantRequest {
            id: "q-0000abcd".into(),
            work: "w-0000abcd".into(),
            version: 2,
            workspace: "work".into(),
            scope: AssistScope::Compare,
            delivery: Delivery::TimedOut,
            delivery_at: 0,
            created_at: 0,
            updated_at: 0,
            purpose: None,
            run: None,
            results,
        }
    }

    fn result(version: u64, summary: &str) -> AssistResult {
        AssistResult {
            version,
            at: 100,
            summary: summary.into(),
            stale: false,
            findings: Vec::new(),
            open_questions: Vec::new(),
            next: Vec::new(),
            files: Vec::new(),
            possible_duplicates: Vec::new(),
            notes: None,
        }
    }

    #[test]
    fn a_request_line_shows_version_scope_delivery_and_workspace() {
        assert_eq!(
            request_line(&request(Vec::new())),
            "q-0000abcd v2 compare  timed-out  [work]"
        );
    }

    #[test]
    fn a_result_shows_every_section_without_control_characters() {
        let mut r = result(2, "Plan A \u{1b}[31mis\u{1b}[0m cheaper.\nSee below.");
        r.findings = vec![Finding {
            text: "Costs less.".into(),
            sources: vec![ResultSource {
                url: Some("https://example.com/p".into()),
                path: None,
                title: Some("Pricing".into()),
                read_at: None,
            }],
        }];
        r.open_questions = vec!["Which region?".into()];
        r.possible_duplicates = vec![PossibleDuplicate {
            work: "w-4e5f6a7b".into(),
            reason: "Same vendor.".into(),
        }];
        assert_eq!(
            result_text(&r, 2, 220),
            "Result v2, 2m ago\nPlan A [31mis[0m cheaper.\nSee below.\n\
             \nFindings\n  1. Costs less.\n     - Pricing  https://example.com/p\n\
             \nOpen questions\n  - Which region?\n\
             \nPossible duplicates\n  - w-4e5f6a7b  Same vendor.\n"
        );
    }

    #[test]
    fn an_older_versions_result_is_marked_stale() {
        let text = result_text(&result(1, "Nine steps."), 2, 100);
        assert!(text.starts_with("Result v1, 0s ago (stale: an older version)\n"));
    }

    #[test]
    fn show_picks_the_request_or_the_items_newest_result() {
        let item = WorkItem {
            id: "w-0000abcd".into(),
            rev: 1,
            title: "t".into(),
            state: muxa_protocol::WorkState::Open,
            created_at: 0,
            updated_at: 0,
            workspace: None,
            later_at: None,
            merged_into: None,
            pinned_at: None,
            due: None,
            captures: Vec::new(),
            requests: vec!["q-0000abcd".into(), "q-0000beef".into()],
            tasks: Vec::new(),
        };
        let with_result = request(vec![result(2, "x")]);
        let newer = AssistantRequest {
            id: "q-0000beef".into(),
            ..request(Vec::new())
        };
        let requests = vec![with_result, newer];
        let items = std::slice::from_ref(&item);
        assert_eq!(
            pick(items, &requests, "w-0000abcd").unwrap().id,
            "q-0000abcd"
        );
        assert_eq!(
            pick(items, &requests, "q-0000beef").unwrap().id,
            "q-0000beef"
        );
        assert_eq!(
            pick(items, &requests[1..], "w-0000abcd").unwrap().id,
            "q-0000beef"
        );
        assert!(pick(items, &requests, "w-0000ffff").is_err());
    }
}
