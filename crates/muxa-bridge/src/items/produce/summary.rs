//! What the agent's summary names: file paths and web links (ADR 0033 §4a).
//! Both stay data: a path is only looked up, never run (invariant 4), and
//! the summary is never logged (invariant 5).
use muxa_protocol::SourceLink;

use crate::routines::output::valid_url;

/// Links per item.
pub(super) const MAX_LINKS: usize = 10;
/// Candidate paths looked up per summary.
const MAX_CANDIDATES: usize = 40;
/// A longer word is not a path worth looking up.
const MAX_WORD: usize = 1024;
/// Characters that end a path or a link in prose and Markdown.
const DELIMITERS: &[char] = &[
    '`', '"', '\'', '(', ')', '[', ']', '<', '>', '{', '}', '|', ',', ';', '*',
];
/// Punctuation that ends a sentence after a path or a link.
const TRAILING: &[char] = &['.', ':', '!', '?'];

/// Words that name a document or an image, in order, each once. A
/// `:line` or `:line:col` suffix is removed.
pub(super) fn paths(summary: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in words(summary) {
        if word.contains("://") || word.len() > MAX_WORD {
            continue;
        }
        let word = without_line(word);
        if (crate::docs::is_doc(word) || crate::docs::is_image(word))
            && !out.iter().any(|p| p == word)
        {
            out.push(word.to_owned());
            if out.len() == MAX_CANDIDATES {
                break;
            }
        }
    }
    out
}

/// The `http` and `https` links, in order, each once. A GitHub pull
/// request's link is titled `PR #<n>`.
pub(super) fn links(summary: &str) -> Vec<SourceLink> {
    let mut out: Vec<SourceLink> = Vec::new();
    for word in summary.split(|c: char| c.is_whitespace() || DELIMITERS.contains(&c)) {
        let url = word.trim_end_matches(TRAILING);
        if !(url.starts_with("https://") || url.starts_with("http://")) || !valid_url(url) {
            continue;
        }
        if out.iter().any(|l| l.url == url) {
            continue;
        }
        out.push(SourceLink {
            url: url.to_owned(),
            title: pull_request(url).map(|n| format!("PR #{n}")),
        });
        if out.len() == MAX_LINKS {
            break;
        }
    }
    out
}

fn words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| c.is_whitespace() || DELIMITERS.contains(&c))
        .map(|w| w.trim_end_matches(TRAILING))
        .filter(|w| !w.is_empty())
}

/// `src/a.md:12:3` is `src/a.md`.
fn without_line(word: &str) -> &str {
    let mut w = word;
    for _ in 0..2 {
        match w.rsplit_once(':') {
            Some((head, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => w = head,
            _ => break,
        }
    }
    w
}

/// The number of `https://github.com/<owner>/<repo>/pull/<n>`.
fn pull_request(url: &str) -> Option<u64> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut parts = rest.split('/');
    let (_, _, pull, n) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if pull != "pull" {
        return None;
    }
    n.split(['#', '?']).next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_the_documents_and_images_the_summary_names() {
        let summary = "Wrote `docs/plan.md` and [the chart](out/chart.png). See /tmp/x/Notes.md:12, \
                       docs/plan.md again, src/lib.rs and https://example.com/a.md.";
        assert_eq!(
            paths(summary),
            ["docs/plan.md", "out/chart.png", "/tmp/x/Notes.md"]
        );
        assert!(paths("No files.").is_empty());
    }

    #[test]
    fn links_are_the_web_links_and_a_pull_request_gets_a_title() {
        let summary = "Opened https://github.com/example/app/pull/817. Docs: \
                       (http://example.com/a?b=1), <https://example.com/a?b=1>, \
                       ftp://example.com and https://github.com/example/app/issues/3.";
        let found = links(summary);
        let got: Vec<(&str, Option<&str>)> = found
            .iter()
            .map(|l| (l.url.as_str(), l.title.as_deref()))
            .collect();
        assert_eq!(
            got,
            [
                ("https://github.com/example/app/pull/817", Some("PR #817")),
                ("http://example.com/a?b=1", None),
                ("https://example.com/a?b=1", None),
                ("https://github.com/example/app/issues/3", None),
            ]
        );
        let many: String = (0..20).map(|i| format!("https://e.com/{i} ")).collect();
        assert_eq!(links(&many).len(), MAX_LINKS);
    }
}
