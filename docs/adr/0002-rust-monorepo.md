# ADR 0002: Rust bridge in one open-source monorepo

Status: accepted (2026-10-25)

## Context

muxa has a host-side bridge and native clients: iPad first, then iPhone and Android. It is open source under MIT OR Apache-2.0. The clients share protocol logic, such as message types, snapshot/stream handling and resync. Later they may share more, such as SSH transport and terminal state.

## Decision

- The bridge is written in Rust: `crates/muxa-tmux`, `crates/muxa-protocol`, `crates/muxa-bridge`, `crates/muxa-cli`.
- All parts live in one repository: Rust crates, `apple/` for Swift, later `android/`, `protocol/` for the contract, and `docs/`.
- The client contract is `protocol/PROTOCOL.md` plus shared fixtures, which every implementation tests against.
- A shared Rust client core through UniFFI is deferred until the Android client starts. The fixtures keep that switch safe.

## Consequences

- One CI covers the bridge on macOS and Linux and the Swift package on macOS.
- Rust can later back the Swift and Kotlin clients through UniFFI. A Go bridge could not do that well.
- Contributors need Rust and, for the app, Xcode. The bridge and the protocol can be developed without Xcode.
