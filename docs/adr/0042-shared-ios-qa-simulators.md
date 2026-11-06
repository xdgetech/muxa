# 0042: Reuse host-managed simulators for iOS QA and screenshots

## Status

Accepted.

## Context

Per-project and screenshot-only devices duplicate simulator data and allow
simultaneous test runs to interfere. Public source code must not contain
the developer's simulator identities or storage paths.

## Decision

Resolve three hardware roles from a host-only JSON file. Tests and screenshots
use explicit destinations under one user-wide lock. Screenshot capture shares
the normal QA iPhone; the existing 13-inch iPad is another shared hardware role.
Normal runs never provision or erase devices. Disable Xcode test parallelism
to avoid implicit test-runner clones. Keep every run's outputs in a fresh
directory on the configured mounted volume, and preserve repository images.

Keep mise as a thin entry point to the same commands as Make. Another project
uses the same host configuration and wrapper. Publish only the role names,
validation, locking and a configuration example containing placeholders.

## Consequences

Simulator QA is serialized across projects and worktrees. A developer must
explicitly provision the hardware roles before running UI tests. Generic CI
builds remain independent of host storage and simulator configuration. Direct
Xcode or simctl operations must be coordinated separately. See
[`docs/ios-qa.md`](../ios-qa.md) for setup and output locations.
