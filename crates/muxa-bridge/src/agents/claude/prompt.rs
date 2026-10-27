//! The menus Claude Code shows while it waits on a tool call, as seen in 2.1.310
//! (`tests/data/claude-code/waiting-*.jsonl`). Only keys that picked the same
//! choice in every menu seen are offered. The menus' other options vary with
//! settings, so they stay in the terminal.

use muxa_protocol::{ChatPrompt, PromptKind, PromptOption};
use serde_json::Value;

use crate::agents::log::{MAX_SUMMARY, MAX_TEXT, cap};

/// "1. Yes" is first in every permission menu.
const ALLOW: &str = "1";
/// Denies a permission (and interrupts the turn), or keeps planning.
const ESCAPE: &str = "Escape";
/// A question's options are numbered from 1, and a digit picks one at once.
const MAX_OPTIONS: usize = 9;
/// A subagent asks for its own calls, which are sidechain lines left out of the
/// chat. A card would name the subagent instead of the call it asks about.
const SUBAGENTS: [&str; 2] = ["Task", "Agent"];

/// The prompt for a pending call to `name`; None for a menu not known.
pub fn prompt(entry: &str, name: &str, input: &Value) -> Option<ChatPrompt> {
    if SUBAGENTS.contains(&name) {
        return None;
    }
    let (kind, question, options) = match name {
        "AskUserQuestion" => (
            PromptKind::Question,
            Some(question(input)?),
            answers(input)?,
        ),
        "ExitPlanMode" => (
            PromptKind::Plan,
            None,
            vec![option("Keep planning", ESCAPE, true)],
        ),
        _ => (
            PromptKind::Permission,
            None,
            vec![option("Allow", ALLOW, false), option("Deny", ESCAPE, true)],
        ),
    };
    Some(ChatPrompt {
        entry: entry.to_owned(),
        kind,
        question,
        options,
    })
}

fn option(label: &str, key: &str, cancel: bool) -> PromptOption {
    PromptOption {
        label: label.to_owned(),
        keys: vec![key.to_owned()],
        cancel,
    }
}

/// The only question, when there is one and it takes one answer. Several
/// questions or several answers use other keys, not checked yet.
fn only_question(input: &Value) -> Option<&Value> {
    let questions = input.get("questions")?.as_array()?;
    let [q] = questions.as_slice() else {
        return None;
    };
    let multi = q
        .get("multiSelect")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (!multi).then_some(q)
}

fn question(input: &Value) -> Option<String> {
    let text = only_question(input)?.get("question")?.as_str()?;
    Some(cap(text, MAX_TEXT))
}

fn answers(input: &Value) -> Option<Vec<PromptOption>> {
    let options = only_question(input)?.get("options")?.as_array()?;
    if options.is_empty() || options.len() > MAX_OPTIONS {
        return None;
    }
    options
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let label = o.get("label")?.as_str()?;
            Some(PromptOption {
                label: cap(label, MAX_SUMMARY),
                keys: vec![(i + 1).to_string()],
                cancel: false,
            })
        })
        .collect()
}
