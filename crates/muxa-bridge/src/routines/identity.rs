//! What every Bot is told about itself, in a run, a talk or as a team
//! member (ADR 0086): that it is a muxa Bot, what it can use and set, and
//! what it may not do. One text, so the three briefs cannot drift.

/// What the Bot can reach and how much it may publish.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reach {
    pub workspace: String,
    pub daily_items: u32,
    /// Connection names with their environment variables, as `name ($ENV)`.
    pub connections: Vec<String>,
    pub mcp: Vec<String>,
    /// The worker it runs as, with its description, when it names one.
    pub worker: Option<String>,
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
    // muxa passes --strict-mcp-config only when the Workspace lists servers (ADR 0075).
    if reach.mcp.is_empty() {
        out.push_str(&format!(
            "- {}. Its own settings decide which other MCP servers and connectors load, \
             the user's claude.ai connectors among them.\n",
            match &reach.worker {
                Some(w) => format!("You run as the worker {w}"),
                None => "You run as the Workspace's agent".to_owned(),
            }
        ));
    } else {
        out.push_str("- Only those MCP servers load; the user's claude.ai connectors do not.\n");
    }
    out.push_str(
        "- Try a source before you say you cannot reach it. For one you cannot reach, ask the user to add \
         a connection or an MCP server in Settings.\n",
    );
    match place {
        Place::Member { lead } => out.push_str(&format!(
            "- You run as a member of the team \"{lead}\" leads. You report to your lead; you never write to the Inbox.\n"
        )),
        Place::Run | Place::Talk => out.push_str(&format!(
            "- Your items reach the user's Inbox, at most {} a day; the rest go into one \"N more\" item. \
             One item per key: a key published before is skipped, so an item is never rewritten.\n",
            reach.daily_items
        )),
    }
    out.push_str(
        "- You never edit the Workspace's `.muxa/` folder, and you never change connections, MCP servers, \
         workers, environment variables or other Bots.\n",
    );
    match place {
        Place::Talk => out.push_str(
            "- In this talk you may propose changes to yourself and your Routines, as shown below: your \
             description, worker, daily items (1 to 50), on or off, team members and handoffs a day, your \
             Soul, and each Routine's schedule or time or review trigger, timezone, read only, items per run \
             and instructions. The user applies them.\n",
        ),
        Place::Run | Place::Member { .. } => out.push_str(
            "- You cannot change your settings in a run. When one holds you back (too few daily items, a \
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
            daily_items: 20,
            connections: vec!["linear ($LINEAR_API_KEY)".into()],
            mcp: vec![],
            worker: Some("lisk-cos: Reads Calendar and Slack".into()),
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
                "You run as the worker lisk-cos: Reads Calendar and Slack",
                "Try a source before you say you cannot reach it",
                "never edit the Workspace's `.muxa/`",
            ] {
                assert!(text.contains(part), "{place:?}: {part}");
            }
        }
    }

    #[test]
    fn claude_ai_connectors_are_off_only_when_the_workspace_lists_servers() {
        let open = section("scout", &reach(), Place::Run);
        assert!(open.contains("the user's claude.ai connectors among them"));
        let strict = section(
            "scout",
            &Reach {
                mcp: vec!["linear".into()],
                ..reach()
            },
            Place::Run,
        );
        assert!(
            strict.contains("claude.ai connectors do not")
                && !strict.contains("You run as the worker")
        );
    }

    #[test]
    fn only_a_talk_may_propose_and_only_a_member_skips_the_inbox() {
        let talk = section("scout", &reach(), Place::Talk);
        assert!(talk.contains("you may propose") && talk.contains("at most 20 a day"));
        let run = section("scout", &reach(), Place::Run);
        assert!(run.contains("cannot change your settings in a run"));
        let member = section("scout", &reach(), Place::Member { lead: "lead" });
        assert!(member.contains("never write to the Inbox") && !member.contains("a day;"));
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
