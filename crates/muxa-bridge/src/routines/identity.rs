//! What every Bot is told about itself, in a run, a talk or as a team
//! member (ADR 0086): that it is a muxa Bot, what it can use and set, and
//! what it may not do. One text, so the three briefs cannot drift.

/// What the Bot can reach.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reach {
    pub workspace: String,
    /// Connection names with their environment variables, as `name ($ENV)`.
    pub connections: Vec<String>,
    pub mcp: Vec<String>,
    /// The user's claude.ai connectors load too (ADR 0087).
    pub claude_connectors: bool,
    /// The Workspace's Bot permission rules, which Claude Code enforces.
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    /// Shell commands only this Bot may run (ADR 0088).
    pub commands: Vec<String>,
}

/// Where the Bot is started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place<'a> {
    Run,
    Talk,
    /// A member of the team `lead` leads.
    Member {
        lead: &'a str,
    },
}

/// Pure. The section that opens the Bot's `.muxa/TASK.md`.
pub fn section(name: &str, reach: &Reach, place: Place<'_>) -> String {
    let list = |v: &[String]| {
        if v.is_empty() {
            "none".to_owned()
        } else {
            v.join(", ")
        }
    };
    let mut out = format!(
        "## You are a muxa Bot\n\n\
         muxa is the user's app for running coding agents and following their work in one Inbox. \
         You are the Bot \"{name}\" of the muxa Workspace \"{ws}\". You do not keep running: each time, \
         muxa starts Claude Code on the user's host from your files.\n\n\
         - Your Soul says who you are: your purpose, scope and manner. It is yours, but the user writes it: \
         only the user changes it, or applies a change you propose in a talk.\n\
         - Your Memory is what you learned. You write it yourself at the end of a run or a talk.\n\
         - Your Routines are muxa Routines: muxa runs you on a schedule, at a time, or when a Task of this \
         Workspace asks for review. They are not Claude cloud routines.\n\
         - This Workspace gives you its connections, as environment variables: {connections}; \
         and its MCP servers: {mcp}.\n",
        ws = reach.workspace,
        connections = list(&reach.connections),
        mcp = list(&reach.mcp),
    );
    // ADR 0087: a Bot has its Workspace's access. muxa keeps other servers out
    // when the Workspace lists some, unless it lets claude.ai connectors in.
    out.push_str(if reach.claude_connectors {
        "- The user's claude.ai connectors (such as Calendar or Sheets) load too. In a run their tools are \
         refused unless this Workspace's Bot rules allow them, one rule per connector, such as \
         `mcp__claude_ai_Google_Calendar`.\n"
    } else if reach.mcp.is_empty() {
        "- No other MCP servers are set for you here; what the Workspace's agent loads by itself may also work.\n"
    } else {
        "- Only those MCP servers load; the user's claude.ai connectors do not.\n"
    });
    if !reach.allow.is_empty() || !reach.deny.is_empty() {
        out.push_str(&format!(
            "- Claude Code enforces this Workspace's rules for Bots. Allowed: {}. Denied: {}.\n",
            list(&reach.allow),
            list(&reach.deny)
        ));
    }
    out.push_str(&format!(
        "- Shell commands you may run: {}. Read-only ones such as `ls` or `date` also run.\n",
        list(&reach.commands)
    ));
    out.push_str(match place {
        Place::Talk => {
            "- Any other command asks the user here. To use one in your runs, propose bot-update with it in \
             `commands` (the inside of a `Bash(...)` rule, such as `/path/collect.sh delivery:*`); the user \
             applies it.\n"
        }
        Place::Run | Place::Member { .. } => {
            "- Any other command is refused, not asked: nobody answers prompts in a run. When you need one, \
             say which and why in a briefing; the user grants it by talking with you.\n"
        }
    });
    out.push_str(
        "- Try a source before you say you cannot reach it. For one you cannot reach, ask the user to add \
         a connection or an MCP server in Settings.\n",
    );
    match place {
        Place::Member { lead } => out.push_str(&format!(
            "- You run as a member of the team \"{lead}\" leads. You report to your lead; you never write to the Inbox.\n"
        )),
        // ADR 0091: no daily limit; the Bot's own judgment and its Routines'
        // items per run keep the Inbox readable.
        Place::Run | Place::Talk => out.push_str(
            "- Each item you publish reaches the user's Inbox. muxa sets no daily limit, so publish only \
             what the user needs to see or act on, and nothing they saw before. One item per key: a key \
             published before is skipped, so an item is never rewritten.\n",
        ),
    }
    out.push_str(
        "- You never edit the Workspace's `.muxa/` folder, and you never change connections, MCP servers, \
         environment variables or other Bots. You have the Workspace's access and nothing else.\n",
    );
    match place {
        Place::Talk => out.push_str(
            "- In this talk you may propose changes to yourself and your Routines, as shown below: your \
             description, on or off, team members and handoffs a day, your \
             Soul, and each Routine's schedule or time or review trigger, timezone, read only, items per run \
             and instructions. The user applies them. When the user asks you for Inbox items, write them \
             to `items` in `.muxa/bot.json` (see the end of this brief), never another way: they \
             reach the Inbox at once.\n",
        ),
        Place::Run | Place::Member { .. } => out.push_str(
            "- You cannot change your settings in a run. When one holds you back (too few items per run, a \
             missing connection), say so in a briefing; the user changes it by talking with you on your \
             page in muxa.\n",
        ),
    }
    // A run gets the guide in its Output section; a talk needs it to write instructions.
    if place == Place::Talk {
        out.push('\n');
        out.push_str(super::brief::INBOX_ITEMS);
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reach() -> Reach {
        Reach {
            workspace: "Lisk".into(),
            connections: vec!["linear ($LINEAR_API_KEY)".into()],
            mcp: vec![],
            claude_connectors: true,
            allow: vec!["WebSearch".into()],
            deny: vec!["Bash".into()],
            commands: vec!["/x/collect.sh delivery:*".into()],
        }
    }

    #[test]
    fn every_place_says_what_the_bot_is_and_may_not_do() {
        for place in [Place::Run, Place::Talk, Place::Member { lead: "lead" }] {
            let text = section("scout", &reach(), place);
            for part in [
                "You are the Bot \"scout\" of the muxa Workspace \"Lisk\"",
                "not Claude cloud routines",
                "linear ($LINEAR_API_KEY)",
                "MCP servers: none",
                "claude.ai connectors (such as Calendar or Sheets) load too",
                "Allowed: WebSearch. Denied: Bash.",
                "Shell commands you may run: /x/collect.sh delivery:*.",
                "Try a source before you say you cannot reach it",
                "never edit the Workspace's `.muxa/`",
            ] {
                assert!(text.contains(part), "{place:?}: {part}");
            }
        }
    }

    #[test]
    fn claude_ai_connectors_follow_the_workspace_setting() {
        let off = Reach {
            mcp: vec!["linear".into()],
            claude_connectors: false,
            ..reach()
        };
        assert!(section("scout", &off, Place::Run).contains("claude.ai connectors do not"));
        let none = Reach {
            claude_connectors: false,
            ..reach()
        };
        assert!(
            section("scout", &none, Place::Run)
                .contains("No other MCP servers are set for you here")
        );
    }

    #[test]
    fn only_a_talk_may_propose_and_only_a_member_skips_the_inbox() {
        let talk = section("scout", &reach(), Place::Talk);
        assert!(talk.contains("you may propose") && talk.contains("no daily limit"));
        assert!(!talk.contains("daily items") && !talk.contains("\"N more\""));
        let run = section("scout", &reach(), Place::Run);
        assert!(run.contains("cannot change your settings in a run"));
        assert!(
            run.contains("is refused, not asked") && talk.contains("propose bot-update with it")
        );
        let member = section("scout", &reach(), Place::Member { lead: "lead" });
        assert!(
            member.contains("never write to the Inbox") && !member.contains("One item per key")
        );
        assert!(talk.contains("Good Inbox items") && talk.contains("draft answer"));
        assert!(
            !run.contains("Good Inbox items"),
            "a run has it in its Output section"
        );
        assert!(
            !member.contains("Good Inbox items"),
            "a member writes no items"
        );
    }
}
