# Agent Workspace — Project Handoff & Implementation Plan

## 1. Project Overview

### Goal

Build a native, keyboard-first mobile workspace for managing and interacting with multiple AI coding agents running persistently on a Mac mini.

The primary device is an **iPad mini with an external keyboard**. The application must eventually support:

- iPad mini (primary)
- iPhone
- Android foldable devices (e.g. Galaxy Fold, Surface Duo-style devices)
- Potentially other tablets

The application should make it practical to perform real software development work remotely without needing a laptop.

**Core principle: The same running agent sessions must be seamlessly accessible from desktop and mobile devices.**

This is not intended to be another general-purpose SSH terminal application.

It is a native Agent Workspace that uses existing tmux sessions as its initial backend.

### Key Product Principles

1. Preserve the existing desktop workflow.
2. Never restart or duplicate running agents when switching devices.
3. Avoid displaying desktop-style tmux splits on small screens.
4. Optimize for external keyboard navigation.
5. Provide native navigation between sessions and panes.
6. Keep raw terminal access available at all times.
7. Progressively introduce agent-aware native interfaces.
8. Design the backend protocol for multiple client platforms.
9. Minimize changes to the existing Mac mini agent infrastructure.
10. Prefer reliability and seamless handoff over unnecessary abstraction.

---

## 2. Existing Environment

### Host

A Mac mini acts as the always-on development machine.

Existing components:

- macOS
- tmux
- SSH
- Tailscale for remote connectivity
- Git worktrees
- Multiple concurrent AI coding agents
- Claude Code and/or Codex
- Custom agent orchestration/runner scripts
- Existing terminal-based development workflow

The agent infrastructure already exists and should be reused.

Do not introduce OpenClaw or replace the existing agent runner.

### tmux Structure

**Each agent has its own tmux session.**

Each session contains two primary panes:

- Left pane: AI coding agent (Claude Code or Codex)
- Right pane: Regular terminal/shell

Example:

```text
Mac mini
│
├── tmux session: feature-auth
│   ├── pane 0: Claude Code
│   └── pane 1: Shell
│
├── tmux session: feature-payment
│   ├── pane 0: Codex
│   └── pane 1: Shell
│
└── tmux session: feature-ui
    ├── pane 0: Claude Code
    └── pane 1: Shell
```

The names are illustrative.

Do not assume pane IDs, indices, session names, or process types are fixed.

Discover them dynamically.

### Existing Desktop Workflow

When working on the Mac:

- Attach to the relevant tmux session.
- Interact with the agent in the left pane.
- Use the right pane for commands, tests, Git operations, etc.
- Switch between agent sessions.
- Agents continue running independently.

**This workflow must remain unchanged.**

### Desired Mobile Workflow

When leaving the desk:

1. Open the iPad application.
2. Connect securely to the Mac mini.
3. See all existing tmux sessions.
4. Select an agent.
5. View its existing agent pane independently.
6. Interact using the external keyboard.
7. Switch to the corresponding terminal pane.
8. Switch between agents instantly.
9. Close or background the application.
10. Return to the Mac and continue using the exact same running session.

No agent restarts, duplicated sessions, or manual synchronization.

---

## 3. Current Problems

Traditional SSH applications are not optimized for this workflow.

### Problem A: Small tmux Panes

When attaching to a tmux session from an iPad mini, the existing two-pane layout makes both panes too narrow.

Agent TUI interfaces become difficult to read.

### Problem B: Navigation Friction

Switching tmux sessions and panes requires terminal-specific keyboard sequences.

These interactions are awkward on mobile devices.

### Problem C: Poor Mobile UX

Standard terminals expose desktop-oriented text interfaces.

They do not provide:

- Native session navigation
- Agent-oriented status displays
- Attention queues
- Native Git diff viewers
- Convenient agent/terminal switching

### Problem D: Device Handoff

The user wants a single persistent workspace, not separate mobile and desktop agent instances.

The same agent processes, working directories, and terminal state must remain available across devices.

---

## 4. Agreed Product Roadmap

The implementation direction is explicitly:

**A → C**

### Phase A: Native tmux Pane Workspace

Build a native interface around existing tmux sessions.

Display each pane independently rather than reproducing the tmux split layout.

Prioritize:

- Session discovery
- Pane discovery
- Full-screen pane display
- Terminal input
- External keyboard navigation
- Fast switching
- Reconnection
- Seamless desktop/mobile handoff

This is the first deliverable.

### Phase C: Agent-Aware Native Workspace

Gradually introduce native interfaces for:

- Agent conversation
- Agent execution status
- Input requests
- Approval requests
- Git changes
- Code review
- Agent activity
- Session orchestration

Use structured agent events, hooks, and supported integration interfaces where possible.

Avoid relying on fragile terminal screen scraping for semantic information.

**Phase C must not replace Phase A.**

Raw terminal access must always remain available as a fallback.

---

## 5. UX Design

### 5.1 Session Navigator

The main screen lists running agent sessions.

Example:

```text
Agents

● feature-auth
  Claude Code

● feature-payment
  Codex

● feature-ui
  Claude Code
```

In Phase C, enrich this with status:

```text
Agents

⚠ feature-auth
  Waiting for approval

● feature-payment
  Working · Running tests

✓ feature-ui
  Completed
```

Status labels must come from reliable signals.

Do not invent agent states based on weak heuristics.

### 5.2 Session Detail

Each session has two primary views:

```text
feature-auth

[ Agent ] [ Terminal ]

┌───────────────────────────┐
│                           │
│    Selected tmux pane     │
│                           │
│                           │
└───────────────────────────┘
```

Only one pane needs to be visible on the iPad mini at a time.

The native UI handles switching.

The original Mac tmux layout must remain unchanged.

### 5.3 Keyboard-First Navigation

Proposed shortcuts:

| Shortcut | Action |
|---|---|
| Cmd+K | Session switcher |
| Cmd+1…9 | Select session |
| Cmd+Shift+[ | Previous session |
| Cmd+Shift+] | Next session |
| Cmd+J | Toggle Agent / Terminal |
| Cmd+Shift+D | Open Git diff |
| Esc | Dismiss overlays |
| Cmd+Enter | Send native agent input, where applicable |

These shortcuts are proposals, not immutable requirements.

Important:

- Preserve normal terminal key behavior.
- Support Ctrl+C, Ctrl+D, Esc, arrows, Option/Alt, etc.
- Avoid intercepting shortcuts required by Claude Code or Codex.
- Allow shortcut customization later.
- Prioritize external hardware keyboard support.

### 5.4 Session Switcher

Cmd+K opens a searchable native overlay.

Example:

```text
Switch Agent

> payment

feature-payment
feature-auth
feature-ui
```

Search by:

- Session name
- Project/repository
- Worktree
- Agent type

Searchable metadata can be added progressively.

### 5.5 Device-Adaptive Layout

The application must support different screen sizes without changing the backend session model.

| Device | Preferred UI |
|---|---|
| iPad mini landscape | Single large pane, native navigation |
| iPad mini portrait | Full-screen selected pane |
| iPhone | Compact agent-first navigation |
| Foldable closed | Single-column view |
| Foldable unfolded | Optional dual-pane layout |
| Desktop Mac | Existing tmux workflow |

Do not assume that all foldables have the same hinge, aspect ratio, or window configuration.

Design layouts based on available window size rather than hard-coded device models.

---

## 6. Proposed Architecture

```text
                    Mac mini
                       │
            ┌──────────▼──────────┐
            │ Existing tmux       │
            │                     │
            │ Session A           │
            │  ├ Agent pane       │
            │  └ Terminal pane    │
            │                     │
            │ Session B           │
            │  ├ Agent pane       │
            │  └ Terminal pane    │
            └──────────┬──────────┘
                       │
            ┌──────────▼──────────┐
            │ Workspace Bridge    │
            │                     │
            │ tmux adapter        │
            │ pane I/O            │
            │ session discovery   │
            │ git adapter         │
            │ agent adapters      │
            └──────────┬──────────┘
                       │
                Secure transport
                SSH / Tailscale
                       │
             ┌─────────┼──────────┐
             │         │          │
           iPad      iPhone     Android
           SwiftUI   SwiftUI    Compose
```

### 6.1 Mac mini Workspace Bridge

A lightweight service responsible for interacting with the existing local environment.

Potential responsibilities:

- Discover tmux sessions
- Discover windows and panes
- Identify agent and shell panes
- Read pane output
- Route input to the correct pane
- Observe session lifecycle
- Provide session metadata
- Handle client reconnections
- Eventually expose agent-specific events
- Eventually expose Git information

The bridge should be designed with clear interfaces, but Phase A should remain minimal.

### 6.2 Client Architecture

Suggested initial technology stack:

- Swift
- SwiftUI
- UIKit where necessary
- SwiftTerm or another suitable terminal emulator
- SSH transport or secure WebSocket through an SSH tunnel
- iOS Keychain for credentials

Android implementation later:

- Kotlin
- Jetpack Compose
- Adaptive layouts / WindowManager
- A compatible terminal rendering solution

The client/backend protocol should be platform-independent.

### 6.3 Transport and Security

Preferred initial network model:

```text
iPad
  │
  │ Tailscale
  │
  ▼
Mac mini
  │
  │ SSH / local authenticated bridge
  │
  ▼
tmux
```

Security requirements:

- Do not expose an unauthenticated bridge to the public internet.
- Prefer existing SSH authentication.
- Store secrets securely.
- Validate host identity.
- Avoid logging sensitive terminal contents by default.
- Avoid shell-command injection when handling session identifiers.
- Treat remote session metadata and terminal output as untrusted.
- Do not allow arbitrary remote commands without an explicit user action or permission model.

---

## 7. Critical Technical Challenge: Independent tmux Pane Rendering

**This is the highest-risk part of Phase A.**

The objective is to display and interact with a single existing tmux pane without changing the desktop layout or restarting the process.

### Important tmux Constraints

tmux fundamentally manages terminal panes with particular dimensions.

A pane has an underlying PTY and a current terminal size.

Multiple attached clients do not automatically receive independently reflowed versions of the same pane.

Therefore:

**Displaying the same running pane at different dimensions is not equivalent to creating two independent terminal views.**

The application must not assume that a tmux pane can be resized independently per client.

### Potential Approaches

#### Approach 1: Capture Pane + Input Injection

Use tmux commands such as:

```bash
tmux list-sessions
tmux list-windows -a
tmux list-panes -a
tmux capture-pane -p -t <pane-id>
tmux send-keys -t <pane-id> ...
```

Advantages:

- Simple
- Works with existing sessions
- No agent changes
- Suitable for early proof-of-concept

Limitations:

- Snapshots are not equivalent to a full terminal protocol.
- ANSI styles and alternate-screen behavior need careful handling.
- Polling can be inefficient.
- Input injection may not preserve every terminal interaction correctly.
- `send-keys` is not a substitute for a general binary-safe PTY transport.

This approach may be suitable for initial discovery and testing, but should not automatically be considered production-ready.

#### Approach 2: tmux Control Mode

Investigate tmux control mode (`tmux -CC`) for:

- Session/window/pane events
- Output notifications
- Layout changes
- Input routing
- Client synchronization

Important:

Control mode is not automatically a standalone independent-pane PTY protocol.

Determine what information is available and how to reconstruct terminal state correctly.

#### Approach 3: tmux Pipe Pane

Investigate `pipe-pane` for streaming pane output.

Potential advantages:

- Live output streaming
- Lower polling overhead

Limitations:

- May not provide a complete initial screen.
- Does not by itself provide independent resize semantics.
- May require terminal emulation and snapshot synchronization.
- Must handle output encoding and lifecycle carefully.

### Required Investigation

Before committing to a rendering architecture, establish:

1. Can a selected existing pane be displayed independently?
2. Can full-screen Claude Code/Codex TUIs be rendered correctly?
3. Can keyboard input be delivered reliably?
4. Can the iPad view avoid modifying the Mac layout?
5. What happens when both devices are connected?
6. What happens when the Mac tmux window is resized?
7. How are output and snapshot streams synchronized?
8. Can the client reconnect without losing usable terminal state?
9. What are the performance implications?
10. What behavior is possible without changing the existing agent runner?

If full independent rendering is infeasible, document the constraints and propose the least disruptive alternative.

Do not silently replace the existing agent process with a new one.

---

## 8. Phase C: Agent-Native Architecture

After Phase A works reliably, add structured agent integration.

### Agent Adapter Interface

Conceptual architecture:

```text
                  Workspace Bridge
                         │
            ┌────────────┼────────────┐
            │            │            │
       Claude Adapter  Codex Adapter  Git Adapter
            │            │            │
            └────────────┼────────────┘
                         │
                  Unified Events
                         │
                Native Client UI
```

### Proposed Agent States

```text
WORKING
WAITING_FOR_INPUT
WAITING_FOR_APPROVAL
IDLE
DONE
ERROR
UNKNOWN
```

`UNKNOWN` is important.

Do not falsely classify agent states when reliable information is unavailable.

### Proposed Event Model

Illustrative only:

```json
{
  "type": "agent.status_changed",
  "sessionId": "feature-auth",
  "status": "WAITING_FOR_APPROVAL",
  "timestamp": "2026-10-25T00:00:00Z"
}
```

Approval event:

```json
{
  "type": "agent.approval_requested",
  "sessionId": "feature-auth",
  "requestId": "req-123",
  "action": "edit_file",
  "summary": "Modify authentication schema"
}
```

The actual protocol should be versioned and designed during implementation.

### Native Agent UI

Long-term desired features:

- Structured conversation history
- Tool execution summaries
- Agent status
- Approval requests
- Native input composer
- Agent activity history
- Session-specific Git changes

Prefer official or supported structured interfaces, event streams, and hooks when available.

Do not assume that Claude Code and Codex expose identical capabilities.

Approval buttons must only be shown when the backend can safely identify and execute the exact pending approval action.

Do not implement approvals by blindly sending keystrokes based on screen-text guesses.

---

## 9. Native Git Integration

The native Git viewer is an important part of Phase C.

Desired features:

- Current worktree identification
- Changed files
- Added/deleted line counts
- Native diff viewer
- Commit history
- Selected-code actions
- Ask Agent about selected changes

Example:

```text
feature-auth

Changes (4)

M src/auth.ts           +43 -12
M src/session.ts        +18  -4
A src/token.ts          +92
M tests/auth.test.ts    +37  -2

[ View Diff ]
```

Later, allow selecting a diff and sending contextual instructions to the existing agent.

All actions must target the correct session/worktree.

---

## 10. Reliability and Session Handoff

The Mac mini is the source of truth.

Clients are views/controllers of the same persistent sessions.

### Requirements

- Opening the iPad application must not create duplicate agents.
- Closing the application must not terminate agents.
- Network interruption must not terminate agents.
- Reconnection must restore session discovery and pane views.
- Switching between devices must preserve the underlying process state.
- The existing desktop tmux layout must remain usable.
- The application should detect terminated or renamed sessions.
- The UI should handle a missing or restarted pane gracefully.

### Concurrent Clients

Both desktop and mobile may be connected simultaneously.

This introduces:

- Competing keyboard input
- Pane resize interactions
- Focus confusion
- Output synchronization concerns

Initially, allow explicit user-directed input to the selected pane.

Later, consider:

- Active controller indicators
- Optional input leases
- Read-only observation mode
- Conflict warnings

Do not introduce complicated distributed locking into the first MVP unless necessary.

---

## 11. Implementation Roadmap

### Milestone 0: Repository and Environment Investigation

Before implementation:

1. Inspect the existing Mac mini setup.
2. Identify the current tmux version.
3. Inspect actual session/window/pane structures.
4. Identify the agent runner and session naming conventions.
5. Identify how agent panes are distinguished from shell panes.
6. Determine whether the current project already contains reusable SSH or tmux code.
7. Inspect available Swift terminal libraries and licenses.
8. Choose the minimum viable transport architecture.

Do not change existing running sessions during investigation.

**Deliverable:** Architecture decision record and implementation plan.

### Milestone 1: tmux Pane Proof of Concept

Build a minimal proof of concept that:

1. Connects to the Mac mini.
2. Discovers existing tmux sessions.
3. Lists panes.
4. Selects a single pane.
5. Displays its content.
6. Sends basic keyboard input.
7. Preserves the existing Mac tmux layout.

Test with:

- Ordinary shell
- Claude Code
- Codex, if available
- Full-screen TUI
- Long output
- Alternate-screen applications
- Unicode text
- Rapid updates

**Deliverable:** Working pane rendering prototype and documented limitations.

### Milestone 2: iPad Native MVP

Implement:

- Connection screen
- Session list
- Session switcher
- Agent/Terminal toggle
- Terminal renderer
- Keyboard shortcuts
- Reconnection
- Basic error handling
- Connection persistence

**Deliverable:** iPad application suitable for real remote agent work.

### Milestone 3: Reliability and Polish

Implement:

- Better reconnection handling
- Session lifecycle updates
- Stable pane targeting
- Keyboard focus management
- Rendering performance improvements
- Background/foreground transitions
- Session search
- Connection diagnostics

**Deliverable:** Daily-use quality Phase A.

### Milestone 4: Agent-Aware Features

Implement:

- Agent adapters
- Structured status events
- Attention queue
- Worktree metadata
- Native Git diff viewer

**Deliverable:** First usable Phase C features.

### Milestone 5: Native Agent Interaction

Implement:

- Structured conversation UI
- Native instruction composer
- Supported approval flows
- Agent activity history
- Rich Git/agent integration

**Deliverable:** Hybrid native agent workspace.

### Milestone 6: Additional Devices

Expand to:

- iPhone
- Android foldables
- Adaptive dual-pane layouts
- Touch-first interactions
- Optional notification integration

Do not let Android implementation delay the initial iPad MVP.

---

## 12. Phase A Acceptance Criteria

The MVP is successful when the following scenario works reliably:

1. Start Claude Code in an existing tmux session on the Mac.
2. Keep the usual Agent + Terminal two-pane layout.
3. Open the application on an iPad mini.
4. Connect to the Mac mini over Tailscale/SSH.
5. See the running sessions.
6. Select the existing Claude Code session.
7. Display the Agent pane independently.
8. Interact using the external keyboard.
9. Switch to the session's Terminal pane.
10. Run a command in the existing shell.
11. Switch to another agent using the native session switcher.
12. Disconnect or background the iPad application.
13. Return to the Mac.
14. Continue interacting with the same original processes.
15. Verify the original tmux session and layout were preserved.

Additional checks:

- No duplicated agent processes.
- No unintended tmux pane resizing.
- No lost or misrouted input.
- Correct terminal rendering.
- Graceful reconnection after network loss.
- Responsive navigation on an iPad mini.

---

## 13. Non-Goals for the Initial MVP

Do not initially build:

- A new AI coding agent framework
- A replacement for tmux
- A replacement for the existing agent runner
- A cloud-hosted agent platform
- A full IDE
- A complete Claude Code UI clone
- A complete Codex UI clone
- Cross-device file synchronization
- Collaboration/multi-user management
- Complex notifications
- Agent orchestration from scratch
- Android and iOS simultaneously

Focus on making the existing workflow usable remotely.

---

## 14. Open Technical Decisions

The implementation team should investigate and recommend solutions for:

1. SSH-only versus lightweight Mac-side bridge.
2. tmux control mode versus capture/pipe approaches.
3. Terminal rendering library.
4. Accurate terminal input handling.
5. Existing-pane display and resize semantics.
6. Session/pane identity and metadata discovery.
7. Reconnection protocol.
8. Backend language and deployment method.
9. Protocol design for eventual Android support.
10. Existing OSS reuse versus new implementation.

Relevant projects to evaluate:

- SwiftTerm — https://github.com/migueldeicaza/SwiftTerm
- tmux — https://github.com/tmux/tmux
- Multiplex — https://github.com/multiplex-term/Multiplex
- Remux — https://github.com/h3nock/remux

Verify repository status, implementation details, compatibility, and licenses before relying on them.

---

## 15. Instructions to the Implementing Agent

You are acting as the lead engineer for this project.

Start by investigating the existing environment and validating the core technical assumptions.

**Do not immediately implement the full application.**

Your first task is to determine the most reliable way to independently display and control an existing tmux pane from an iPad without disrupting the desktop tmux layout.

Follow this sequence:

1. Inspect the existing tmux/agent setup.
2. Review relevant OSS implementations.
3. Evaluate the available pane I/O mechanisms.
4. Identify technical constraints and failure modes.
5. Recommend the smallest viable architecture.
6. Implement a focused technical proof of concept.
7. Test the proof of concept against an actual existing tmux session.
8. Document results and remaining risks.
9. Proceed to the native iPad MVP once feasibility is established.

Prefer incremental, testable implementation.

Preserve existing workflows and running processes.

Make reasonable technical decisions independently, but document architectural tradeoffs.

Prioritize real-world usability over theoretical elegance.

### Definition of Success

**I can leave my Mac mini running several AI agents, take only an iPad mini and external keyboard, connect to those exact same sessions, work comfortably, and return to the Mac without any interruption or change to my original workflow.**

The product should eventually evolve into a native, cross-device Agent Workspace, but the first priority is making this handoff genuinely seamless.
