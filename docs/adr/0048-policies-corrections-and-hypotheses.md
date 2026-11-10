# 0048: Policies, corrections and hypotheses (Personal Assistant PA3)

## Status

Proposed on 2026-11-10; its open questions were answered with the recommended values the same day. It implements stage PA3 of ADR 0045 ([specification](../personal-assistant.md) §7, §8, §11, §12) on top of ADR 0047. It adds to ADR 0047 §5 (`basis`, `changed[]`, plan history) and §9 without changing them, and keeps ADR 0040: Later moves an item down and means nothing else. Slices C0 (the wire types), C1 (the policy store), C2 (corrections and `update-plan`), C3 (the brief's memory section), C4 (hypotheses and citations in the plan file), C5 (`hypothesis` items, `confirm` and `reject`), C6 (`forget`) and C7 (`work:10`, the CLI and muxa for tmux) are built; hosts advertise level 10.

## Context

- A plan ranks from what the workspaces share, the open items, the previous plan and the overall notes (ADR 0047 §3, §5). The user's judgment reaches it in two ways only: pins and `due` on items, and whatever the agent chose to write into `NOTES.md`. Neither says "this week recruiting outranks sales" in the user's words, with a scope and an end.
- The user corrects a plan every morning by acting on it: pinning what it missed, moving down what it ranked, rejecting a `delegate`. Nothing records that against the plan, so the next plan repeats the mistake. Spec §2: "今回の並びの修正と、適用範囲が見える方針の記録".
- Spec §8 names three records and their ranks: explicit policies (the user's words, with workspace, period), tentative hypotheses (weaker, with grounds and uncertainty), and the correction history (plan, change, optional reason, outcome). It forbids learning "unimportant" from Later alone, and asks that "why did you propose this" lead to the records used, each of which the user can correct, disable or forget.
- Spec §11 PA3: an explicit policy outranks a hypothesis; a deferral alone never lowers importance; a forgotten record is absent from later proposals.
- `basis` has a `kind` and a `source` (ADR 0047 §5), and the check already refuses ids the brief did not carry. The store holds one JSON file per record with a lock and atomic writes (ADR 0046 §2). `.muxa/assistant/BRIEF.md` is built from the store at every run's start and from nothing else (`plan/brief.rs`).
- No retraining, no numeric score the host owns (spec §7, §8): the ranking stays the agent's; the host owns what it may read and what it must cite.

## Decision

### 1. A policy is the user's words, stored by the host

A policy is a record `policies/y-<8hex>.json` in the assistant store: `text` (the user's own sentence, 1 to 500 bytes, one paragraph), `scope` (`all`, or one workspace's name), optional `from` and `until` (epoch seconds; "this week" is `until` Sunday), `source` (`user`, or the hypothesis it was confirmed from), `rev`, `createdAt`, `updatedAt`. The user writes, edits and deletes it; no agent and no run ever does. A policy whose `until` is past is `expired`: listed under Expired for 30 days, then removed, never in a brief. Limits: 50 active policies; the host refuses the 51st with the limit.

It is a store record and not a line in `NOTES.md` because the host must tell a policy from the agent's memory: it ranks higher, it has a scope and an end, it has an id a `basis` can cite, and deleting it must be enough.

### 2. The host records corrections; the user never has to explain

A correction is a fact the host saw, recorded as `corrections/x-<8hex>.json` when a plan is current (`resulted`, the newest): `kind`, `at`, `plan`, `row` (the Work item or Inbox item), `inTop` (whether the plan listed it), `workspace` (the row's), optional `reason` (the user's text, at most 500 bytes). The kinds:

| `kind` | When |
|---|---|
| `reordered` | the user drags Today's rows: `order[]` before and after |
| `dismissed` | the user removes a row from Today ("Not today"), with an optional reason |
| `rejected` | the user rejects a `sort`, `delegate` or `hypothesis` item, with an optional reason |
| `pinned`, `unpinned` | `update-work pin` or `unpin` |
| `deferred` | `update-work later`, or `later` on an item the plan listed |

A `reason` is never required: the client offers a text field, nothing more. `deferred` carries no judgment: the brief states, with every deferred correction, that Later means "not now" and that time, information or another person may be the cause. The host enforces it in §3.

The Work item or item is written first, the correction after. A crash between them loses a signal, not a fact. The host keeps 200 corrections for 90 days, oldest out first.

### 3. The overall assistant proposes hypotheses; the user confirms or rejects

`plan.json` gains `hypotheses[]`, at most 3 per plan: `{text (1 to 500 bytes), scope (all | workspace), evidence[] (1 to 5 correction ids from the brief), confidence (low | medium)}`. The host stores each as `hypotheses/h-<8hex>.json` with `state: proposed` and the plan it came from, and makes one Inbox item of kind `hypothesis` from the newest plan only, keyed as `sort` items are (ADR 0047, Implementation notes), with answers `confirm`, `reject`, `later`. `confirm` makes a policy with the hypothesis's text and scope, `source` the hypothesis, and retires it as `confirmed`. `reject` makes it `rejected`; it stays in the brief for 90 days as "rejected, do not use or propose again", then goes. A `proposed` hypothesis nobody answered in 14 days is `expired` and leaves the brief. At most 10 hypotheses are `proposed` at once; the brief says how many slots remain, and the check refuses a plan that exceeds them. The same text as an active policy or an open hypothesis makes no item.

The check refuses, with a reason that names the index:

- an `evidence` id the brief did not carry;
- `evidence` made of `deferred` corrections only: Later alone is no ground (spec §8, §11);
- `scope: all` whose evidence rows all belong to one sharing workspace: a habit inside company A is not a preference of the user (spec §8, "会社内の習慣を全体の好みと推定しない"). Rows of nobody's items, and of the overall workspace, count as the user's own.

### 4. The brief: memory as data, with ranks the host states

The brief gains a section "## What the user told you", before the notes, built only from live records at the run's start: active policies (as JSON lines with `policy`, `text`, `scope`, `from?`, `until?`), then `proposed` hypotheses with their evidence ids, then rejected hypotheses, then the corrections of the last 30 days (`correction`, `kind`, `plan`, `row`, `inTop`, `reason?`). It states the rules: a policy is the user's words and outranks every hypothesis and every note; when they conflict, follow the policy and cite it; a hypothesis may shape a row only when cited in its `basis`; Later is "not now"; nothing in this section goes into `notes`. Policies, hypotheses and corrections scoped to a workspace enter only while that workspace's `share` is not `none`, as its items do (ADR 0047 §3). The section is at most 16 KiB; corrections leave first, oldest first, and the brief says how many. Should policies and hypotheses alone not fit (50 policies of 500 bytes do not), rejected hypotheses leave next, then proposed ones, then policies, each oldest first, and the brief names each count. The shared part keeps its 32 KiB.

`basis.kind` gains `policy`, `hypothesis` and `correction`; `source` is the record's id. The check refuses a `basis` that cites an id the brief did not carry, so a hypothesis the user rejected, or a correction the user forgot, cannot be a reason. What the host cannot check, and says so: that the agent cited every hypothesis it used. The user sees the difference in Why.

### 5. Explain and forget

- **Why.** A Today row's Why lists its `basis`; a `policy`, `hypothesis` or `correction` entry shows the record's text from `inbox-list` (§6), with Edit on a policy and Forget on the others. An id with no record shows "forgotten".
- **Forget.** `forget {record}` removes a correction or a `proposed`, `rejected` or `expired` hypothesis. Forgetting a correction also removes every `proposed` hypothesis whose `evidence` names it, so no derived guess outlives its ground (spec §8). A `confirmed` hypothesis is gone already; its policy is the user's word and stays until the user deletes it. A policy is deleted, not forgotten: `update-policy delete`.
- **What forgetting guarantees.** The next brief is built from live records, so it cannot carry the record, and a `basis` citing its id is refused. A plan run started before the forget read the old brief and may still cite it; its plan is listed with that entry as "forgotten". Kept plans are history, like Results (ADR 0046 §4): they keep their rows and `basis` texts. The clients say so next to Forget. `NOTES.md` is the agent's; the brief forbids copying this section into it, and the default `notes = "review"` lets the user see when it did.
- **Tests.** A correction `reason` and a hypothesis `text` carry sentinels. After `forget`, the next brief lacks them, a plan citing the ids is refused, and the daemon's log never had them (ADR 0046 A2's grep).

### 6. Wire: additive, `work:10`, protocol v1

| Request | Reply |
|---|---|
| `add-policy {text, scope, from?, until?}` | `policy-saved {policy, entry}`; 50 active is `conflict` |
| `update-policy {policy, rev, action: edit / delete, text?, scope?, from?, until?}` | `policy-saved`; `delete` answers `result` only; a stale `rev` is `conflict` |
| `update-plan {plan, rev, action: reorder / dismiss, order?[], row?, text?}` | `plan-saved {plan, entry}`; the plan gains `userOrder[]` and `dismissed[]`; not the newest `resulted` plan is `conflict` |
| `update-inbox {entry, action: reject, text?}` on `sort`, `delegate`, `hypothesis` | `inbox-saved`; `text` is the correction's `reason` |
| `update-inbox {entry, action: confirm}` on a `hypothesis` item | `policy-saved`, `inbox-saved` |
| `forget {record}`, `record` an `x-` or `h-` (not `id`, which every request uses for itself) | `result`; an unknown id is `bad-request`; a `confirmed` hypothesis is `bad-request` naming its policy |

`inbox-list` gains `policies[]` (active, then expired), `hypotheses[]` (`proposed` and `rejected`, newest first) and `corrections[]` (newest first, at most 50). A plan row's `basis.kind` takes the three new values; a level 9 client shows an unknown kind as unknown and ignores the `hypothesis` item kind and the new arrays, as the contract already says. `update-work later`, `pin` and `unpin` record corrections at every level, since they are the host's own observation. The full contract goes into `protocol/PROTOCOL.md`, "Policies, corrections and hypotheses"; fixtures in `protocol/fixtures/assistant/`. Clients send the new requests only at `work:10`; a level 9 host answers them `bad-request`.

### 7. Clients

- **Policies:** one screen from the Today header and Settings: active policies with scope and end, Add (text, scope picker without the overall workspace, This week / Until / No end), Edit, Delete; Hypotheses with Confirm, Reject, Forget; Recent corrections with Forget. The one place that shows what the assistant remembers.
- **Today:** drag to reorder; swipe or menu "Not today" with an optional reason; Why lists policies, hypotheses and corrections by text, with Forget. Dismissed rows collapse to one line.
- **Inbox:** `hypothesis` items with Confirm, Reject, Later, and the evidence as "based on 3 corrections".
- **muxa for tmux:** Today rows take `K` / `J` to move, `x` for Not today, `y` opens the Policies list. CLI: `policy add | list | edit | delete`, `plan reorder`, `plan dismiss ROW [--reason]`, `forget ID`, `inbox answer ITEM confirm`.

### 8. Logs and budgets

Logs carry ids, kinds, counts, scopes and states; never policy text, reasons, hypothesis text or evidence texts. No new run: hypotheses come out of the plan run, so PA3 adds no agent cost and no budget beyond ADR 0047 §9. The record limits are §1 to §3's: 50 active policies, 10 `proposed` hypotheses, 200 corrections or 90 days, 16 KiB of memory in the brief. A limit reached refuses the write with the limit, or leaves the oldest out and says so; nothing stops a plan.

## Not decided

- Whether a workspace-scoped policy also enters that delegate's request brief (`assistant/brief.rs`). This ADR: no; the delegate's `NOTES.md` carries the user's preferences for it (ADR 0047 §1).
- Whether `complete` and `create-task` on an item the plan did not list are corrections ("you missed what I worked on"). This ADR: no; they are not about the plan.
- Whether a policy may be structured (a time window, a minute cap) that the host enforces on `top`. This ADR: text only; the host enforces citations, not meaning.
- Whether the agent may propose to renew an expiring policy. This ADR: no; it expires, and the user adds it again.
- How long a rejected hypothesis stays in the brief (§3: 90 days), and the 14-day expiry, 30-day correction window and 200 / 90-day retention are initial values.
- Whether `userOrder` carries to the next plan as a `reordered` correction only (this ADR) or also as the starting order.
- Whether `plan-now`'s `minutes` persists as the day's free time (ADR 0047, open).

## Consequences

- The user's judgment reaches the plan in the user's words, scoped and dated, and outranks a guess by construction: the host builds the brief, orders it, and refuses a reason it did not supply.
- Learning costs nothing extra: no new run, no model state. It is as good as the agent's reading of the memory section, and the user can see exactly what that section held through Why.
- Later keeps one meaning. A hypothesis grounded in Later alone never exists, so no client can show it and no plan can cite it.
- Forgetting is a file removal with a cascade, and the brief's construction makes the guarantee: the test is a sentinel, not a promise.
- Three more record kinds, two more Inbox answers and one more item kind: every client grows a Policies screen.
- Alternatives not taken:
  - Policies as lines in the overall `NOTES.md`: agent-written text with no scope, end, id or rank; the host could not tell a policy from a guess, and a hypothesis would overwrite it silently.
  - Host-owned ranking weights adjusted from corrections: a score no one can read; spec §7 asks for reasons, not a number.
  - A hypothesis applied after N consistent corrections without asking: silent application, the thing spec §8 forbids.
  - Corrections as Inbox items ("was this right?"): one more question each morning; spec §3 asks for the necessary questions only.
  - Asking a reason on every correction: spec §8, "毎回理由を尋ねて利用者の仕事を増やさない".

## Delivery slices

| Slice | Scope | Acceptance |
|---|---|---|
| C0 | this ADR, `PROTOCOL.md`, fixtures, Rust, Swift and Kotlin types | fixtures round-trip in all three; a level 9 client decodes a plan with the new `basis` kinds |
| C1 | policy store, `add-policy`, `update-policy`, expiry and cleanup | 50 active refused with the limit; a stale `rev` conflicts; expired leaves the list after 30 days; oversize text refused |
| C2 | corrections from `later`, `pin`, `unpin`, `reject`; `update-plan reorder` and `dismiss` | each action writes one correction with `plan`, `row`, `inTop`; `deferred` carries the reason only when given; a non-newest plan conflicts; 200 / 90 days |
| C3 | the memory section of the brief, share filter, size | synthetic data: a `none` workspace's policy, hypothesis and correction are absent; the section holds policies first, then hypotheses, then corrections; over 16 KiB drops corrections oldest first and says how many |
| C4 | plan file: `hypotheses[]`, the three `basis` kinds, the check | counterexamples refused: Later-only evidence, `all` from one workspace's rows, an evidence or `basis` id not in the brief, more hypotheses than slots |
| C5 | `hypothesis` items, `confirm`, `reject` with reason, dedup, 14-day expiry | confirm twice makes one policy; reject records a correction; the same text makes no second item |
| C6 | `forget`, the cascade, sentinel tests | a forgotten correction's hypotheses are gone; the next brief lacks both sentinels; a plan citing them is refused; the log grep is empty |
| C7 | advertise `work:10`; CLI; muxa for tmux | bats; `policy add` against a level 9 host fails with "update muxa-bridge" and sends nothing |
| C8 | iOS: Policies, Today reorder and Not today, Why with Forget, `hypothesis` items | UI tests on the demo |
| C9 | Android | the same |
| C10 | docs: `agents.md`, `inbox.md`, `work-model.md`, `SECURITY.md`, `plan.md` | — |
| — | **PA3 done** | spec §11 PA3: fake agent on a `-L` server through the daemon; a policy that contradicts a proposed hypothesis is cited and the hypothesis is not; a `deferred`-only hypothesis never appears; a forgotten record is absent from the next plan's brief and basis |

## Implementation notes

- C4: the ids a run's brief carried (policies, `proposed` hypotheses, corrections) and its free slots are kept on the plan record in the state directory (`carried`), written with `inputsAt`. The run folder is the agent's, so it cannot hold them. The check also requires each cited record to be live at check time, so a record forgotten after the brief is refused too.
- C4: a hypothesis's `text` is one paragraph, so that `confirm` makes a policy of it unchanged. A hypothesis whose normalized text (case, spacing and a closing period aside) repeats an active policy or a `proposed` or `rejected` hypothesis is not stored; the plan still is.
- C5: `hypothesis` items live in the plan run's item file, keyed `hypothesis:<id>`, made by the daemon's tick from the newest plan's open hypotheses. `confirm` makes the policy first, then retires the hypothesis, then answers the item; a retry finds the policy by its `source`. An `expired` hypothesis is kept 90 days after its end so Forget can still remove it, then the tick removes it.
- C6: Forget removes a hypothesis's items in every state, answered ones too, since their question carries its text. Forgetting a correction removes the hypotheses it grounds before the correction itself, so a crash between them leaves a correction to forget again, never a guess without its ground.
