# Workers and workspace memory: design review

GPT-6 Astra reviewed the proposed ADR 0022 draft before implementation. The original draft was in its authoring worktree; the revised proposal is now `docs/adr/0022-workers-and-workspace-memory.md`. This review does not accept it or authorize a memory-writing implementation.

## Coverage

The draft structurally covers all three requested features: named workers referring to native Claude Code subagents or Codex profiles, a selected worker on each Task with restart handoff, and workspace memory with a proposed close-out diff for human review.

## Original review findings

1. **Reset the old worker environment (deferred with worker.env).** Worker A's `new-session -e` values become session environment and survive `respawn-pane`; passing only worker B's overrides does not remove A's configuration or secrets. Define a complete resolved environment without mutating user-session options. Verify A → B → A with disjoint environment keys and config homes. Evidence: `scripts/repro/respawn-pane.sh`, `scripts/repro/session-env.sh`.
2. **Do not replace full memory from a truncated base.** The draft snapshots only 64 KiB but allows a 256 KiB replacement. Preserve a full editable base or reject close-out above the supported size. Verify memory larger than the cap and UTF-8 boundaries.
3. **Serialize memory applies per workspace path.** Hash-check then rename is not a compare-and-swap across processes. A Task lock cannot serialize two Tasks updating one WORKSPACE.md. Lock the canonical memory path around hash comparison, backup and replacement, including fallback bridges. Verify simultaneous independent writers. Evidence: `crates/muxa-bridge/src/records/store.rs`, ADR 0019's fallback-writer contract.
4. **Give close-out proposals a generation.** A declined proposal remains on disk and a later request immediately returns to `asked`. Bind output to close-out generation, agent run and full base snapshot; invalidate old output and require completion before enabling apply. Verify decline → ask again, repeated requests, restart and reads before completion.
5. **Define close-out readiness separately.** The existing start/restart prompt waiter requires a fresh `done` and expires after 30 seconds. An already-finished Task may have an older `done`, so reusing that waiter can prevent the prompt that would start the next turn. Define already-idle readiness, running-turn waits, timeout and retry. Evidence: `crates/muxa-bridge/src/task_records/handoff.rs`.
6. **Specify keep/set/clear worker semantics.** Omitted worker means keep, but the proposed picker offers Workspace default. Add an explicit clear operation and CLI spelling, with compatibility tests.
7. **Resolve configuration consistently on restart.** The draft reloads instructions but can reuse the previous command for the same worker name. Either resolve the command/profile on every restart or pin the complete worker configuration to a run and define refresh. Verify editing native profile or agent choice without renaming the worker.

## Trust and native configuration

- A human-approved diff is a muxa workflow convention unless enforced by permissions outside the agent. An agent running as the same OS user may write reachable memory or invoke an apply CLI. Do not claim an enforced human-only boundary without designing one.
- Worker workspace allowlists control assignment by muxa; they do not sandbox filesystem or tool access.
- File validation must cover symlinked parent directories and races, not only the final memory file. Include proposal and backup paths.
- Local Codex CLI help confirms profile-file layering in the installed version. Historical version claims, accepted profile keys, Claude ancestor/config-directory discovery and exact agent prompt behavior still need the draft's native-configuration probes or official documentation verification.

Follow-up: the revised ADR 0022 in the current checkout addresses all seven findings. Astra's second pass requested exclusive prompt delivery claims, Task/workspace lock coordination and a durable apply operation journal; those contracts are now included. Native role probes remain Workers gates; exact environment cleanup gates only a future per-worker environment extension. Memory has its own completion and transaction gates. ADR 0022 remains Proposed.


## Delivery-plan refinement

A further Astra review recommended independent Workers/handoff (W) and workspace-memory (M) releases. Per-worker environment overrides were optional scope, so the revised proposal rejects `worker.env` in W and preserves the existing workspace-env behavior and its inherited-key limitation. The exact-reset research no longer blocks named workers. Native configuration can still affect an agent's environment; no account-isolation guarantee is added.

`docs/workers.md` separates each release's protocol, host/CLI, app and verification phases. It also specifies explicit run assignment (known default vs legacy unknown), displayed-revision conflicts, restart lifecycle claims, stale adapter rematching and coherent snapshots captured under the workspace lock. Documentation and installed help establish native launch interfaces; controlled runtime observations remain unverified gates, not completed work.

Astra's consistency pass found one further memory edge case: snapshot and revision-bearing proposal reads must recover/refuse a prepared journal operation before exposing a revision, not only before apply. The ADR and M acceptance cases now cover crash-after-rename followed by create, restart or close-out, preventing new bytes from being paired with the old revision.
