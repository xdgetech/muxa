# 0045: A Personal Assistant coordinates work across Workspaces

## Status

Proposed. Product specification: [Personal Assistant](../personal-assistant.md).
This is a design proposal, not an implemented feature or a change to the order in `docs/plan.md` Next.
It proposes extending ADR 0033's Inbox presentation and changing ADR 0035's default `+` action, while preserving ADR 0040's Later semantics.

## Context

The user wants to capture thoughts without choosing a Project or deadline, have an assistant investigate and move them forward, and receive useful next actions across their companies and personal work. Current Tasks start execution; Inbox items represent decisions. Neither is a suitable sole record for an unstarted idea. Workspace-specific credentials already exist, but there is no durable inter-workspace request protocol.

## Proposed decision

1. Make Personal Assistant a coordinating role across explicitly participating Workspaces on one host, distinct from Personal Workspace. Each Workspace's delegate uses its own agent environment and tools; the coordinator receives permitted results, not the combined credentials. Workspace configuration is not an OS sandbox.
2. Persist raw captures, unstarted Work items, delegated requests and results separately from execution Tasks and decision Inbox items. A shared Inbox surface presents their relationships without duplicating the underlying work. Saving or deferring never launches an agent; starting execution remains explicit or covered by a bounded, previously granted processing policy.
3. Make common capture the default `+` action, retaining an explicit route to New Task. Return concrete research, comparisons or private drafts with sources and remaining decisions. External sends, publication, spending and large changes require explicit authorization.
4. Begin prioritization with a morning top three and on-demand proposals. Explain deadlines, dependencies, impact, time estimates and freshness; preserve manual choices, current selection and Later's move-to-bottom behavior.
5. Improve proposals through explicit policies and separately marked hypotheses, corrections and outcomes. Support explanation, correction and forgetting, with bounded cost and frequency. Deferral alone is not evidence of low importance.
6. Reuse daemon, Routine, Task, Worker and delivery-recovery foundations. Add durable request identity, destination, scope, result provenance, deduplication and partial/stale-result handling. Do not blindly resend uncertain delivery or treat existing Ask sessions or MCP access as an implemented RPC protocol. Initial delegation is one level deep.

## Consequences

The product gains an intake and coordination layer without turning every note into execution. It also needs new records, host operations, sharing policy, client presentation and migration; these are not supplied by today's Routine scheduler or Workspace memory design. The specification's stages and acceptance conditions precede implementation. Existing decision semantics and direct Terminal access remain available.

Alternatives not selected: putting all accounts in a privileged Personal Workspace; using Tasks as an unstarted backlog; treating all work as unanswered questions; keeping progress only in agent conversations; automatically reordering the active Inbox on every update. Each conflates a distinct responsibility or loses the user's control over execution and attention.
