# 0088: A Bot's own shell commands, granted one by one

## Status

Proposed (2026-11-23). Builds on ADR 0087. Wire: `protocol/PROTOCOL-v2.md`, `BotSpec.commands`. Evidence: `docs/findings/claude-code-permissions.md`.

## Context

ADR 0087 gave a Workspace's Bots one set of rules. A Bot that needs a script (a collector, a reader) then forced a choice: allow the script for every Bot of the Workspace, or deny the shell to all of them. A run has nobody to answer a prompt, so leaving a command neither allowed nor denied made the run wait until its limit. The developer wants a script granted to one Bot, explicitly, and kept with that Bot.

## Decision

1. **`commands` on a `[[bot]]`** lists shell command patterns only that Bot may run, each the inside of a Claude Code `Bash(...)` rule, such as `/path/collect.sh delivery:*`. They join the Workspace's Bot rules as allow rules for that Bot alone. The Workspace's deny rules still win.
2. **Granted only explicitly.** The user adds a command in the Bot editor, or applies a talk's `bot-update` proposal, whose note names every command. A run cannot change it. A talk's `bot-update` that leaves `commands` out keeps the granted ones, so a talk only adds commands by naming them, and only the user removes one.
3. **Runs refuse instead of asking.** A Bot's Routine runs and team members start with `--permission-mode dontAsk`: an allowed command runs, any other is refused at once and the run goes on. The Bot is told to say in a briefing which command it needed and why. Talks keep prompts, since the user is there to answer.
4. **Each Bot is told its commands** and how to ask for more (ADR 0086's muxa Bot section).

## Consequences

- A Workspace no longer needs a blanket `Bash` deny to keep runs from waiting; its Bot rules can stay about tools, and scripts go to the Bots that use them.
- Claude Code still runs read-only commands it approves by itself (`ls`, `date`) in `dontAsk`.
- Alternatives not taken:
  - Scripts in the Workspace's Bot rules: every Bot of the Workspace gets them.
  - Per-Bot tool rules beyond the shell: more to configure; the Workspace is the boundary for tools (ADR 0087).
  - Letting a run ask and the user answer from Needs you: a Bot's run would then wait on the user, often for hours.
