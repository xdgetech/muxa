# ADR 0003: SSH exec channel as the transport

Status: accepted (2026-10-25)

## Context

The client reaches the host over a private network such as Tailscale. The bridge must not become a new unauthenticated network service. Options: a listening daemon (WebSocket or TCP) with its own auth, or SSH.

## Decision

The client opens an SSH connection and runs the bridge as the exec command (`~/.local/bin/muxa-bridge`). The protocol is JSON lines on the channel's stdin and stdout. There is no daemon and no listening port. One bridge process serves one connection and exits when the connection ends, on a `bye` request, or after an idle timeout.

On iOS the transport is Citadel (MIT), which pins a fork of swift-nio-ssh. Its exec stdin cannot signal EOF, so the protocol has an explicit `bye`.

## Consequences

- Authentication, encryption and host identity come from SSH. The client pins the host key on first use and keeps an ed25519 key in the Keychain.
- An SSH key is shell-equivalent. `SECURITY.md` documents how to restrict the key to the bridge with `command=` in `authorized_keys`.
- Non-interactive SSH has a minimal `PATH`, so the bridge locates tmux itself.
- iOS drops connections in the background. Reconnecting means a new bridge process and a fresh snapshot. There is no server-side session to resume.
