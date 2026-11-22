# Known gaps

What is built but not verified, and what is left after the F phases and their follow-ups (2026-11-22). `docs/plan.md` stays the roadmap; this list is the open edges. Remove a line when it is done.

## Needs a human

- **Mac UI tests** (`make test-mac-app`): macOS times out with "Timed out while enabling automation mode". Allow UI automation once (`automationmodetool`), then run them.
- **`make test-herdr`**: the live herdr-core suite, humans only.
- **App name**: `store/*/name.txt` and `title.txt` still say "AI agents in tmux". The rest of the store text is current.
- **Store screenshots** of Home and Focus: none exist; take them by hand (`store/RELEASE.md`, Screenshots). The captions in `apple/Muxa/Art/frame-screenshots.swift` still describe the v1 screens.
- **Install**: `make install`, then `muxa-bridge daemon install` to rewrite the service file without the old tmux flags.

## Built on an assumption (check on a real host)

- **Codex Chat** assumes the Codex process keeps the Task folder as its working directory (`src/agents/codex.rs`). No row in `docs/findings/` yet. The Linux `/proc` cwd path has not run.
- **MCP config**: Claude Code expands `${VAR}` in `.mcp.json`; that it does the same in a `--mcp-config` file is assumed (`docs/findings/claude-code-mcp.md`).
- **claude.ai connectors** under `--strict-mcp-config`: unknown whether they still load. Only workspaces that list `[[workspace.mcp]]` get the flag.
- **Bracketed paste** for multi-line Request changes: unverified for Claude Code and Codex, so the apps send one line only (ADR 0072).
- **Japanese input in the terminal**: claimed in the store text, not re-checked on a device after D4.

## Not built

- **Settings** that show who uses which connection and MCP server, and when it was last used (ADR 0069 §7).
- **Codex**: no MCP servers, no Routines (whether Codex sends OSC 7501 by itself is unchecked), no approval prompt in Chat.
- **Edit Routine** on a Failed item and **Make it a Task** on a Read item (`docs/next.md`).
- **Adding a place** to an open Task.
- **Review from a draft PR**: only a passed check or the user makes a Review item. Approve's "merged" is by ancestry, so a squash-merged branch needs Remove anyway.
- **A restarted agent** (Retry) is not in plan mode and does not continue the old conversation.
- **Push** for Needs you while the app is closed (ADR 0066, a relay).
- **A Routine run in a folder Claude Code does not trust** waits on the trust dialog until the 2 h limit.
- **Agent state age** (invariant 6) is not reported.

## Tests missing

- No UI test reaches "Remove anyway": the demo servers never answer `would-lose-work`.
- Server-level Retry of a live agent's loop is untested (fake host ids differ).
- No end-to-end test of `muxa-cli v2 workspaces|approve|request-changes|retry`.
- `BridgeHost::prepare` is long; the adapters' and runner's MCP paths are tested, the shape is not refactored.
