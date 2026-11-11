# 0058: A request's result is reviewed like a Task

## Status

Accepted on 2026-11-11. It amends ADR 0051 §3 (Approval: the `handoff` item) and §7 (Inbox), ADR 0052 §2 (a `write` item's answers) and the "Result ready" marker of ADR 0048 §10 and commit 96a94cd. It adds to ADR 0048 §4 (a request version) without changing it. Hosts advertise `work:16`.

## Context

- A step's Result (ADR 0048 §4, ADR 0051 §3) reaches the Inbox only when it asks for a hand-off: one `handoff` item with Approve, Reject and Later. A Result without a hand-off makes no item. The user finds it as "Result ready" on the Work row, or as "· new result" in muxa for tmux, both kept on the device.
- The user: "In most cases it is like a Task: I want to say what to do next, or I must review what it wrote." A Task's finished turn makes a `review` item with the agent's last message and a reply box (ADR 0033). A Result has no such item.
- Asking the same member again exists only as Ask Again in Work detail: a new request version, with the lead unless the user picks a member, and a purpose typed from scratch.
- A `write` item (ADR 0052) is approved or rejected. A draft that is almost right can only be rejected, then asked for again from Work detail.

## Decision

### 1. Every Result makes one `result` item

When a request version's newest step has its Result stored, the daemon's tick makes one Inbox item of kind `result` in the request's workspace, in the step's run item file, keyed `result:<request>:<version>:<step>`. It replaces the `handoff` item: a Result with a hand-off makes a `result` item that also offers the hand-off.

- `title` is the Work item's title. `question` is "`<worker>` finished: `<summary's first line>`", or "`<worker>` asks `<next>` to: `<purpose>`" with a hand-off. Without a staff, `<worker>` is "The assistant".
- `summary` is the Result's summary, as Markdown, as a Task's `review` has the agent's last message.
- `attachments.links` are the findings' source URLs, each once, at most 10, titled by the source's title.
- `result {request, version, step, work, worker?, findings[], openQuestions[], next[], files[], handoff?}`: the findings' first lines (at most 200 characters each), the open questions, the next steps, the files the run wrote, and the hand-off `{worker, purpose}` the step proposes. The user reviews the Result without opening the Work item.
- Only Results stored in the last 24 hours make an item, as failure items do (ADR 0048, addendum 2026-11-11). An upgraded host does not fill the Inbox with old Results.
- Under `handoffs = "auto"`, a step whose hand-off the host started makes no item: the chain goes on, and the last step's Result makes the item. Past a limit, the item is made with the hand-off and the limit before the summary, as the `handoff` item had it (ADR 0051 §3).
- A newer request version resolves an open `result` item of an older version: "A newer version of this request replaced this result." A cancel leaves it open: the Result is still there to review.

An open `handoff` item made before this ADR keeps its answers until it is answered.

### 2. Answers: Hand off, Send, Done, Later

| Answer | Wire | Effect |
|---|---|---|
| **Hand off to `<worker>`** | `approve` | Only when the step proposes a hand-off that can still start. Starts the next step, once, as the `handoff` item's approve did (ADR 0051 §3). |
| **Send** | `reply {text}` | Asks the member who wrote the Result to go on, with the text (§3). |
| **Done** (✓) | `dismiss` | Closes the item and ends the chain: its hand-off never starts. Replaces Reject. |
| **Later** | `later` | Moves the item down (ADR 0040). |

`answers` is `approve` (with a hand-off), `reply`, `dismiss`, `later`. The wire reuses existing actions, so a client before level 16 shows a `result` item as an unknown kind with working buttons: Approve, the reply box, ✓ and Later. New actions (`send`, `handoff`, `done`) would show as nothing there (`protocol/PROTOCOL.md`, "Inbox items" 2). The labels are the client's: on a `result`, `approve` is "Hand off to `<worker>`" and `dismiss` is "Done".

### 3. Send asks the same member again, as a new version

Send makes a new version of the request, as `request-assistant` does at the Work item's current `rev`:

- its step 1 is the member who wrote the Result, `by: user`; without a staff, the workspace's delegate;
- its `purpose` is the user's text, and its scope is the request's;
- the request file keeps `followUp {version, step}`: the Result it answers. That step's run gets the Result whole in `.muxa/assistant/followup.json`, and its goal says "The user read the result of version `<v>`, step `<s>`, and wrote what is under 'What the user asked'. Continue from it."

So a newer version still makes every older Result `stale` (ADR 0048 §4): the follow-up's answer is the current one, and the Result it continues stays as history, in full in the run folder and cut in the goal's "Earlier work". The item is answered `reply`. One Send per item: it is keyed by the item and its `result` hash through ADR 0049's `Approval`, so a second Send returns the version the first made. A budget reached is `conflict`, and the item stays open. Without the daemon, Send is `unsupported`.

Not taken: a new step of the same member in the same version. ADR 0051 §3 lets a worker appear once per chain and counts steps against `max_steps`; a follow-up is the user asking again, which is what a version is.

### 4. `write` items: Approve, Reject, and Revise

A `write` item's answers become `approve`, `reject`, `reply`, `later`. **Revise** is `reply {text}`: a new version as in §3, step 1 the member that proposed the write (`write.worker`, else the delegate), purpose the user's text, and `followUp {version, step, write}` with the write's index. The goal also says which write to draft again. The revised item is answered `reply`; the new Result proposes the new text as a new `write` item. Approve still needs the shown `hash` (ADR 0052 §2).

ADR 0052 §2 otherwise stays: another new version leaves an open `write` item open. Only the item the user revised closes.

### 5. The new-result marker goes

A `result` item is unread until the user opens it, on every client (ADR 0033). That is the marker.

- muxa for tmux drops "· new result" on Work rows and their sort. Its poll still notifies once per Result (`results.sh track`), because the Inbox poll notifies nothing else.
- The apps show "Result ready" on a Work row only for a host below level 16, which makes no `result` items.

### 6. Wire: additive, `work:16`, protocol v1

`ItemKind` gains `result`; an item gains `result {…}`. `update-inbox reply` is accepted on `result` and `write` items, `approve` and `dismiss` on `result` items; their replies are `work-saved {work, entry, request}`, `inbox-saved`, `result` for `approve` and `reply`. Clients send `reply` on a `write` item only at level 16; a level 15 host answers `bad-request`. Fixtures: `protocol/fixtures/assistant/`.

### 7. Clients

- **Inbox**: a `result` item shows the summary, then Findings, Open Questions, Next Steps and Files, then the links. The bottom has **Hand off to `<worker>`** when offered and a reply box **Send** ("Tell `<worker>` what to do next…"). ✓ is Done.
- A `write` item gets a reply box **Revise** ("Ask `<worker>` for a new draft…") under Approve and Reject.
- muxa for tmux (`prefix + T`): `a` Hand off, `s` Send (asks for the text), `d` Done, `l` Later. On a `write` item, `s` is Revise. CLI: `inbox answer ITEM handoff | send --text T | done | revise --text T`.

### 8. Logs

Logs carry ids, versions, steps, worker names and the answer. Never the text, the summary or the Result.

## Consequences

- Every Result is one decision in one place, with the same shape as a Task's review. The Work row no longer carries a second, device-local "new" state.
- A conversation with a member is a line of versions; each Send costs one run and counts against the budgets.
- A Result the user does not want to act on still makes an item. Done or Delete Old clears it.
- Alternatives not taken:
  - Keeping `handoff` items and adding `result` items beside them: two items for one Result.
  - New wire actions `send`, `handoff`, `done`: clients before level 16 would show none of them.
  - Done completing the Work item: the Work item is the user's (ADR 0048 §4); a Result never changes it.
