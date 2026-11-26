# 0094: A Bot has no daily item limit

## Status

Proposed (2026-11-24). Amends ADR 0082 decision 3 and ADR 0089 decision 4. Wire: `protocol/PROTOCOL-v2.md`, `BotSpec` and `Inbox`.

## Context

ADR 0082 gave each Bot `daily_items` (1 to 50, default 3). Past it, a Bot's items went into one Read item, "N more from <bot>". The developer: "daily_items is very limiting, i think we should remove as we cannot predict what is the good number". A watch Bot has quiet days and busy days. A low number hides the items that matter on a busy day. A high number limits nothing.

## Decision

1. **No daily limit.** Every item a Bot's run publishes becomes its own Inbox item. Nothing new replaces the limit. These still keep a Bot from flooding the Inbox:
   - a Routine's `output.items`, 1 to 50 per run, default 10;
   - runs at least 15 minutes apart;
   - one item per key: a key published before is skipped;
   - a talk's `items`: at most 20 per output, and the user asked for them;
   - the Soul and the Routine instructions, which the user writes, and a brief that says to publish only what the user needs;
   - the Done and Deleted counts beside the Bot, and turning the Bot off;
   - Delete Older Than in the Inbox (ADR 0095).
2. **The "N more from <bot>" item goes.** It existed only for the limit. An open one in an Inbox stays as a plain Read item. Nothing migrates.
3. **Old settings and older apps: accept and ignore.**
   - `daily_items` in a `[[bot]]` of `routines.toml` still loads, with any value, and does nothing.
   - `daily_items` leaves `BotSpec` on the wire. The host ignores it when an older app sends it. An older app that reads a spec without it shows its own default.
   - `bot-update` removes the key from the `[[bot]]` table, so files clean up as they are edited. `bot-add` never writes it.
4. **`Inbox.bots[].added` stays.** It now counts the items the Bot's runs added on `day`, with no limit. The Mac's Bot home shows it as "Items today".

## Consequences

- A busy day shows every item. The user reads them, or clears old ones in one step.
- A Bot with bad instructions can publish up to 50 items a run. Its Done and Deleted counts show it, and the user can turn it off.
- Alternatives not taken:
  - A higher default: still a number nobody can pick.
  - A limit per Routine per day: the same guess, in more places.
  - Keeping the field as an optional cap: one more setting that is rarely right.
