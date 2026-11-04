//! What an item shows besides its question: the agent's last reply and menu
//! from its adapter, the pane's changes, and the files and links the reply
//! names. Blocking (files, git). The reply is stored in the item and never
//! logged (invariant 5).
use std::path::{Component, Path};

use muxa_protocol::{
    Attachments, ChangeCount, ChangeInfo, ChangeStatus, ChatEntry, ChatKind, ChatPrompt,
};
use muxa_tmux::Pane;

use crate::agents::{ChatEvent, Registry};
use crate::changes::git;

/// Bytes of a summary.
pub(super) const MAX_SUMMARY: usize = 4096;
/// Paths per attachment list.
pub(super) const MAX_PATHS: usize = 20;

/// What the adapter's transcript says about the pane's agent now.
#[derive(Debug, Default)]
pub(super) struct Chat {
    pub summary: Option<String>,
    pub prompt: Option<ChatPrompt>,
    /// The name and summary of the tool call the prompt is for.
    pub tool: Option<(String, String)>,
    /// The last message is the agent's reply. None without a transcript.
    pub replied: Option<bool>,
}

pub(super) fn chat(agents: &Registry, session: &str, pane: &Pane) -> Chat {
    let info = crate::sessions::info(session, pane);
    let mut source = match agents.open(&info) {
        Ok(Some((_, source))) => source,
        Ok(None) => return Chat::default(),
        Err(e) => {
            tracing::debug!(pane = %info.pane, error = %e, "no transcript for an inbox item");
            return Chat::default();
        }
    };
    let entries = match source.poll(&info) {
        Ok(events) => entries_of(events).unwrap_or_else(|| source.entries()),
        Err(e) => {
            tracing::debug!(pane = %info.pane, error = %e, "cannot read a transcript");
            return Chat::default();
        }
    };
    let prompt = source.prompt();
    let tool = prompt.as_ref().and_then(|p| {
        let t = entries
            .iter()
            .rev()
            .find(|e| e.id == p.entry)?
            .tool
            .as_ref()?;
        Some((t.name.clone(), t.summary.clone()))
    });
    let (summary, replied) = reply(&entries);
    Chat {
        summary,
        prompt,
        tool,
        replied: Some(replied),
    }
}

fn entries_of(events: Vec<ChatEvent>) -> Option<Vec<ChatEntry>> {
    events.into_iter().find_map(|e| match e {
        ChatEvent::Reset { entries, .. } => Some(entries),
        ChatEvent::Upsert(_) => None,
    })
}

/// The last reply, and whether no prompt of the user came after it.
pub(super) fn reply(entries: &[ChatEntry]) -> (Option<String>, bool) {
    let last = |kind: ChatKind| {
        entries
            .iter()
            .rposition(|e| e.kind == kind && !e.text.trim().is_empty())
    };
    let Some(at) = last(ChatKind::Assistant) else {
        return (None, false);
    };
    let replied = last(ChatKind::User).is_none_or(|u| u < at);
    (Some(cut(entries[at].text.trim(), MAX_SUMMARY)), replied)
}

/// At most `max` bytes, cut at a char boundary.
pub(super) fn cut(text: &str, max: usize) -> String {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_owned()
}

/// Folders whose files are muxa's or the agent's own, never evidence.
const PRIVATE: [&str; 2] = [".muxa", ".claude"];

/// What a turn that started at `since` (epoch seconds) leaves in the pane's
/// folder `dir`: in a repo, its changes; outside one, the documents and
/// images changed since then. Then the files and links `summary` names.
/// Paths are relative to the repo root, or to `dir` outside a repo, as the
/// pane's `docs` and `images` list them.
pub(super) fn attachments(dir: &Path, summary: Option<&str>, since: Option<u64>) -> Attachments {
    let root = crate::docs::repo_root(dir);
    let mut out = match &root {
        Some(_) => changes(dir),
        None => changed_since(dir, since),
    };
    let Some(summary) = summary else {
        return out;
    };
    let base = root.unwrap_or_else(|| dir.to_path_buf());
    for raw in super::summary::paths(summary) {
        let Some(path) = named(dir, &base, &raw) else {
            continue;
        };
        let list = if crate::docs::is_image(&path) {
            &mut out.images
        } else {
            &mut out.docs
        };
        if list.len() < MAX_PATHS && !list.contains(&path) {
            list.push(path);
        }
    }
    out.links = super::summary::links(summary);
    out
}

/// The pane's changes: uncommitted, else the branch's own.
fn changes(dir: &Path) -> Attachments {
    let changes = git::list(dir);
    let (entries, scope) = match &changes.branch {
        _ if !changes.entries.is_empty() => (&changes.entries, None),
        Some(b) if !b.entries.is_empty() => (&b.entries, Some("branch".to_owned())),
        _ => return Attachments::default(),
    };
    of_changes(entries, scope)
}

/// The documents and images under `dir` changed since `since`, newest
/// first, at most `MAX_PATHS` each. Nothing without a start.
fn changed_since(dir: &Path, since: Option<u64>) -> Attachments {
    let Some(since) = since else {
        return Attachments::default();
    };
    let newest = |keep: fn(&str) -> bool| {
        let mut found = crate::docs::scan(dir, keep, Some(since));
        found.sort_by(|a, b| b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path)));
        found.into_iter().take(MAX_PATHS).map(|f| f.path).collect()
    };
    Attachments {
        docs: newest(crate::docs::is_doc),
        images: newest(crate::docs::is_image),
        ..Attachments::default()
    }
}

/// `raw`, a path from the summary, relative to `base` when it names a file
/// inside the pane's folder `dir`, also after symlinks, and outside `PRIVATE`.
fn named(dir: &Path, base: &Path, raw: &str) -> Option<String> {
    let given = Path::new(raw);
    let rel = if given.is_absolute() {
        given
            .strip_prefix(dir.canonicalize().ok()?)
            .ok()?
            .to_path_buf()
    } else {
        given.to_path_buf()
    };
    let real = crate::docs::inside(dir, &rel)?;
    let path = real.strip_prefix(base.canonicalize().ok()?).ok()?;
    let private = path.components().any(|c| match c {
        Component::Normal(n) => PRIVATE.iter().any(|p| n == *p),
        _ => true,
    });
    if private {
        return None;
    }
    path.to_str().map(str::to_owned)
}

pub(super) fn of_changes(entries: &[ChangeInfo], scope: Option<String>) -> Attachments {
    let sum = |f: fn(&ChangeInfo) -> Option<u32>| {
        entries
            .iter()
            .fold(0u32, |n, c| n.saturating_add(f(c).unwrap_or(0)))
    };
    let present = || entries.iter().filter(|c| c.status != ChangeStatus::Deleted);
    let paths = |keep: fn(&str) -> bool| {
        present()
            .map(|c| c.path.clone())
            .filter(|p| keep(p))
            .take(MAX_PATHS)
            .collect()
    };
    Attachments {
        changes: Some(ChangeCount {
            files: u32::try_from(entries.len()).unwrap_or(u32::MAX),
            added: sum(|c| c.added),
            removed: sum(|c| c.removed),
            scope,
        }),
        docs: paths(crate::docs::is_doc),
        images: paths(crate::docs::is_image),
        links: Vec::new(),
    }
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
