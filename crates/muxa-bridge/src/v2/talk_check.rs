//! Pure parts of a talk with a Bot (ADR 0083): its brief, the check of the
//! settings it proposes, and how each op reads in the Inbox.

use muxa_protocol::settings::{SettingsOp, SettingsWorkspace};
use serde::Deserialize;

use crate::records::bots::MAX_MEMORY;

/// `.muxa/bot.json` in the talk's folder.
pub const OUTPUT: &str = "bot.json";
/// Ops in one proposal.
const MAX_OPS: usize = 20;
const MAX_SUMMARY: usize = 200;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TalkOutput {
    pub memory: Option<String>,
    pub proposal: Option<RawProposal>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawProposal {
    pub summary: String,
    pub ops: Vec<SettingsOp>,
}

/// Parses the output; the reasons go to the agent.
pub fn parse(bytes: &[u8]) -> Result<TalkOutput, String> {
    let out: TalkOutput =
        serde_json::from_slice(bytes).map_err(|e| format!(".muxa/{OUTPUT} is not valid: {e}"))?;
    if out.memory.as_ref().is_some_and(|m| m.len() > MAX_MEMORY) {
        return Err(format!("memory is larger than {} KiB", MAX_MEMORY / 1024));
    }
    if let Some(p) = &out.proposal {
        let summary = p.summary.trim();
        if summary.is_empty() || summary.chars().count() > MAX_SUMMARY {
            return Err(format!(
                "proposal summary is one line of 1 to {MAX_SUMMARY} characters"
            ));
        }
    }
    Ok(out)
}

/// The ops a talk with `bot` in `ws` may propose: its own Bot and its own
/// Routines, in its own Workspace. A Routine it adds is turned off.
pub fn check_ops(
    ws: &SettingsWorkspace,
    bot: &str,
    ops: Vec<SettingsOp>,
) -> Result<Vec<SettingsOp>, String> {
    if ops.is_empty() || ops.len() > MAX_OPS {
        return Err(format!("a proposal has 1 to {MAX_OPS} ops"));
    }
    let owned = |name: &str| {
        ws.routines
            .iter()
            .any(|r| r.spec.name == name && r.spec.bot.as_deref() == Some(bot))
    };
    let bot_exists = ws.bots.iter().any(|b| b.spec.name == bot);
    ops.into_iter()
        .enumerate()
        .map(|(i, op)| {
            let refuse = |why: &str| format!("ops[{i}] {}: {why}", op.name());
            if op.workspace() != Some(ws.name.as_str()) {
                return Err(refuse(&format!("workspace must be {:?}", ws.name)));
            }
            match op {
                SettingsOp::BotAdd { bot: ref b, .. } if b.name != bot => {
                    Err(refuse(&format!("only Bot {bot:?} can be added")))
                }
                SettingsOp::BotAdd { .. } if bot_exists => {
                    Err(refuse("the Bot exists already; use bot-update"))
                }
                SettingsOp::BotUpdate { bot: ref b, .. } if b.name != bot => {
                    Err(refuse(&format!("only Bot {bot:?} can be changed")))
                }
                SettingsOp::BotAdd { .. } | SettingsOp::BotUpdate { .. } => Ok(op),
                SettingsOp::RoutineAdd {
                    workspace,
                    mut routine,
                    instructions_text,
                } => {
                    if routine.bot.as_deref() != Some(bot) {
                        return Err(format!(
                            "ops[{i}] routine-add: the Routine's bot must be {bot:?}"
                        ));
                    }
                    routine.enabled = false;
                    Ok(SettingsOp::RoutineAdd {
                        workspace,
                        routine,
                        instructions_text,
                    })
                }
                SettingsOp::RoutineUpdate { ref routine, .. }
                    if routine.bot.as_deref() != Some(bot) || !owned(&routine.name) =>
                {
                    Err(refuse(&format!(
                        "only Routines of Bot {bot:?} can be changed"
                    )))
                }
                SettingsOp::RoutineRemove { ref name, .. } if !owned(name) => Err(refuse(
                    &format!("only Routines of Bot {bot:?} can be removed"),
                )),
                SettingsOp::RoutineUpdate { .. } | SettingsOp::RoutineRemove { .. } => Ok(op),
                _ => Err(refuse("is not allowed in a talk")),
            }
        })
        .collect()
}

/// One line per op for the item's note.
pub fn describe(op: &SettingsOp) -> String {
    match op {
        SettingsOp::BotAdd { bot, soul_text, .. } => {
            let soul = if soul_text.is_some() {
                ", with a Soul"
            } else {
                ""
            };
            format!("Add Bot {}{soul}", bot.name)
        }
        SettingsOp::BotUpdate { bot, soul_text, .. } => {
            let mut parts = vec![format!("Change Bot {}", bot.name)];
            if soul_text.is_some() {
                parts.push("its Soul".into());
            }
            if !bot.members.is_empty() {
                parts.push(format!("team: {}", bot.members.join(", ")));
            }
            parts.join(", ")
        }
        SettingsOp::RoutineAdd { routine, .. } => {
            format!("Add Routine {} ({}, off)", routine.name, when(routine))
        }
        SettingsOp::RoutineUpdate {
            routine,
            instructions_text,
            ..
        } => {
            let text = if instructions_text.is_some() {
                ", new instructions"
            } else {
                ""
            };
            let state = if routine.enabled { "on" } else { "off" };
            format!(
                "Change Routine {} ({}, {state}{text})",
                routine.name,
                when(routine)
            )
        }
        SettingsOp::RoutineRemove { name, .. } => format!("Remove Routine {name}"),
        other => other.name().to_owned(),
    }
}

fn when(r: &muxa_protocol::settings::RoutineSpec) -> String {
    match (&r.on, &r.at, &r.schedule) {
        (Some(on), _, _) => format!("on {on}"),
        (None, Some(at), _) => format!("daily at {at}"),
        (None, None, Some(cron)) => cron.clone(),
        (None, None, None) => "Run now only".into(),
    }
}

/// The talk part of `.muxa/TASK.md`: who the Bot is, its settings now, and
/// how to propose changes.
pub fn brief(ws: &SettingsWorkspace, bot: &str) -> String {
    let found = ws.bots.iter().find(|b| b.spec.name == bot);
    let mut out = format!("\n## You are the Bot {bot}\n\n");
    out.push_str(&format!(
        "You are talking with the user in the muxa Workspace {:?}. Help them shape you: who you are, what you watch, and your Routines.\n",
        ws.name
    ));
    match found {
        Some(b) => {
            out.push_str("\n### Your Soul (the user's words; follow them)\n\n");
            out.push_str(text_or(b.soul_text.as_deref(), "(none yet)"));
            out.push_str("\n\n### Your memory (your own notes from earlier runs and talks)\n\n");
            out.push_str(text_or(b.memory_text.as_deref(), "(nothing yet)"));
            let spec = serde_json::to_string_pretty(&b.spec).unwrap_or_default();
            out.push_str(&format!("\n\n### Your settings now\n\n```json\n{spec}\n```\n"));
        }
        None => out.push_str(
            "\nYou do not exist yet. Learn what the user wants, then propose bot-add for yourself with a Soul, and your first Routines.\n",
        ),
    }
    let routines: Vec<_> = ws
        .routines
        .iter()
        .filter(|r| r.spec.bot.as_deref() == Some(bot))
        .collect();
    if !routines.is_empty() {
        out.push_str("\n### Your Routines now\n");
        for r in routines {
            let spec = serde_json::to_string_pretty(&r.spec).unwrap_or_default();
            out.push_str(&format!(
                "\n```json\n{spec}\n```\n\nInstructions ({}):\n\n{}\n",
                r.spec.instructions,
                text_or(r.instructions_text.as_deref(), "(cannot be read)")
            ));
        }
    }
    let others: Vec<String> = ws
        .bots
        .iter()
        .filter(|b| b.spec.name != bot)
        .map(|b| match &b.spec.description {
            Some(d) => format!("- {}: {d}", b.spec.name),
            None => format!("- {}", b.spec.name),
        })
        .collect();
    if !others.is_empty() {
        out.push_str(&format!(
            "\n### Other Bots of this Workspace (you may lead them as members)\n\n{}\n",
            others.join("\n")
        ));
    }
    out.push_str(&reach(ws));
    out.push_str(&format!(
        r#"
### How to change your settings

You never edit files in the Workspace's `.muxa/` folder: they are denied to you. Propose instead. Write `.muxa/{OUTPUT}` in this folder, then end your turn:

```json
{{
  "memory": "your whole memory, replaced (at most 16 KiB)",
  "proposal": {{
    "summary": "one line the user reads in their Inbox",
    "ops": [ /* settings ops, see below */ ]
  }}
}}
```

Both keys are optional. The user applies a proposal from their Inbox; nothing changes before. Allowed ops, all with "workspace": {ws:?}:
- {{"op":"bot-add"|"bot-update","workspace":…,"bot":{{"name":{bot:?},"description":…,"worker":…,"daily_items":3,"enabled":true,"members":[],"handoffs_per_day":10}},"soul_text":"…"}}
- {{"op":"routine-add"|"routine-update","workspace":…,"routine":{{"name":…,"bot":{bot:?},"instructions":"routines/<name>.md","enabled":false,"schedule":"*/30 9-18 * * 1-5","timezone":"Europe/Berlin","read_only":true}},"instructions_text":"…"}}
  (a schedule is five cron fields, runs at least 15 minutes apart; or "at":"09:00" daily; or "on":"review" to run when a Task asks for review)
- {{"op":"routine-remove","workspace":…,"name":…}}

Routines you add are added turned off; the user turns them on. After you write the file and end your turn, muxa checks it and writes the result to `.muxa/CHECK.md`; read it on your next turn. Update your memory when you learn something about the user worth keeping.
"#,
        ws = ws.name
    ));
    out
}

/// Pure. What a Bot's Routine runs can use, so a talk does not have to guess.
fn reach(ws: &SettingsWorkspace) -> String {
    let names = |v: Vec<String>| {
        if v.is_empty() {
            "none".to_owned()
        } else {
            v.join(", ")
        }
    };
    let connections = names(
        ws.connections
            .iter()
            .map(|c| format!("{} (${})", c.spec.name, c.spec.env))
            .collect(),
    );
    let mcp = names(ws.mcp.iter().map(|m| m.name.clone()).collect());
    format!(
        r#"
### What your Routine runs can use

Your Routines are muxa Routines: muxa runs Claude Code on this host on their schedule. They are not Claude cloud routines.
- Connections of this Workspace, as environment variables: {connections}.
- MCP servers of this Workspace: {mcp}. Only these load; the user's claude.ai connectors do not. A service with no connection or server here cannot be read; tell the user to add one in Settings › Connections or MCP servers.
- Each run puts its items in the user's Inbox, one item per `key`; a key already published is skipped, so an item is never rewritten.
- `daily_items` (1 to 50, now in your settings) is how many of your items reach the Inbox each day; the rest go into one "N more" item. A Bot that reports every to-do needs a higher number: propose it with bot-update.
"#
    )
}

fn text_or<'a>(text: Option<&'a str>, empty: &'a str) -> &'a str {
    match text.map(str::trim) {
        Some(t) if !t.is_empty() => t,
        _ => empty,
    }
}

#[cfg(test)]
#[path = "talk_tests.rs"]
mod tests;
