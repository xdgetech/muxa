# Inbox UI specification

ADR 0023 defines the shared rules. Slice 1 provides navigation; direct actions remain in the existing Chat and Task detail screens.

## Session rows

Task sessions show the Task title, Inbox group and reported age on the first line; session name, branch and loop badge below. Closed records show Closed. Missing ages are omitted. Manual sessions keep their existing presentation. Search matches Task title as well as existing session fields.

## Inbox

A Triage/List control retains the existing grouped List and detail. Open Triage when its visible queue is nonempty, otherwise List. Triage shows position, group, Task title, workspace/project, session/branch when present, reported agent state and loop facts. Use “Ready for review” for both idle and done; do not infer that idle means a completed turn.

| Need | Text | Navigation |
|---|---|---|
| answer | Waits for your answer | Open Chat when supported, Open Terminal |
| agent-error | The agent stopped with an error | Open Terminal, Task detail |
| loop | Loop paused / Loop stopped: reason | Check log when available, Task detail, Open Session |
| review | Ready for review | Changes when supported, TASK.md, Check log when available, Task detail, Open Session |
| no-agent | No agent state is available | Open Terminal when a pane exists, Task detail |
| no-session | No session runs this Task | Task detail |

Only enable links with a supported capability and a known target. With no chat support, an answer card offers Open Terminal. Show secondary loop facts even when the primary need is answer or agent-error. The detail retains existing lifecycle actions, their confirmations and errors.

Phone: bottom bar Open, Snooze, Next. Tablet: card with the same controls, Previous and next two items. Swiping left moves to Next. At the end show “Nothing else needs you”, skipped/snoozed counts, Start over and Show List. Do not wrap automatically. Show snoozed restores local hidden items; List always includes them.

Reader opens over Inbox and returns to the same Task when it still exists. Opening Chat or Terminal uses the existing session navigation; Inbox never starts a chat behind the pane screen. Disconnect clears snoozes and disables stale-target links.

## Sidebar and initial navigation

Show up to three queue items with need labels, Triage all (count), and New Task. If empty, show Nothing needs you and All Tasks (List). Follow ADR 0023's once-per-connection phone routing after both lists arrive; tablet routing remains unchanged.

## Slice 1 keyboard controls

| Action | iPad | Android |
|---|---|---|
| Inbox | Command+T | Ctrl+Shift+T |
| Triage/List | Command+L | Ctrl+Shift+L |
| Next/Previous | Command+Down/Up | Ctrl+Shift+Down/Up |
| Open session | Command+O | Ctrl+Shift+O |
| Snooze | Command+S | Ctrl+Shift+S |

Scope shortcuts to the visible screen and suspend pane navigation chords while Inbox owns focus. Approval, denial and follow-up shortcuts remain in Chat. Slice 2's inline actions and desktop summary are proposed in ADR 0023, with no UI implementation yet.
