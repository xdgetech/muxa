# muxa protocol v2

Status: draft (2026-11-21), built in slices B1–B3 of `docs/plan.md`. ADR 0067. Protocol v1 (`PROTOCOL.md`) stays until the hard switch.

## Transport

- A client on another device runs `muxa-bridge connect` over SSH exec. The bridge relays stdin and stdout to the server's socket and adds nothing.
- A client on the host connects to the server's Unix socket directly: `server.sock` in the daemon's directory (`$XDG_STATE_HOME/muxa`, else `~/.local/state/muxa`), beside v1's `daemon.sock`.
- The socket is mode 0600 in a 0700 directory; the server checks the peer's uid.

## Frames

Every byte in both directions is a frame:

```
[channel u32 LE][kind u8][len u32 LE][payload: len bytes]
```

A payload is at most 16 MiB. An unknown kind or an oversized frame ends the connection.

| Kind | Name | Payload |
|---|---|---|
| 0 | DATA | channel 0: one JSON control message. Terminal channel: see below |
| 1 | CREDIT | u32 LE: output bytes the client consumed |
| 2 | CLOSE | the channel is over. From the server: a JSON exit `{"reason","exit_code","signal"}` |
| 3 | RESIZE | client → server: u16 LE cols, u16 LE rows |
| 4 | ACK | server → client: u64 LE, the input sequence number just applied |

## Terminal channels

The server opens one channel per attach. Channel numbers are the server's.

- **Output** (server → client DATA): `[offset u64 LE][bytes]`. `offset` is the position of the first byte in the stream since its last paint. A paint starts at offset 0.
- **Paint:** before a paint, the server sends the control message `paint` with the channel. The client then resets its terminal emulator. A paint follows an attach, and a resume the server's buffer cannot cover (B3).
- **Input** (client → server DATA): `[seq u64 LE][bytes]`. `seq` starts at 1 per channel and grows by 1. The server applies each `seq` once and answers ACK. A repeated `seq` (after a resume) is acknowledged again, not applied again.
- **Credit:** each channel starts with 256 KiB of output credit. The client sends CREDIT for the output bytes (not headers) it has consumed. With no credit left, the server stops reading the terminal for this client, and the backend's own flow control slows the program.
- **Resize** (client → server RESIZE): the terminal takes this size while this client is its last active viewer. The program redraws itself; no paint is sent.
- **Close:** the client sends CLOSE to detach. The server sends CLOSE with the exit when the program ends or the terminal goes away.

## Control messages (channel 0)

JSON objects with `type`. A request has an `id` (u64, chosen by the client). The server answers `{"type":"reply","id",...}` or `{"type":"error","id","code","message"}`. Messages without `id` are notifications.

### Handshake

The client's first frame is `hello`. The server answers `welcome`, or `error` and closes.

```json
{"type":"hello","version":2,"client":"muxa-ios 1.3","resume":null}
{"type":"welcome","version":2,"server":"muxa 0.1.0","backend":"herdr-core 0.1.21","token":"…","resumed":[]}
```

`token` identifies the client's session for a resume (slice B3).

### Requests

| Request | Reply |
|---|---|
| `{"type":"terminals"}` | `{"terminals":[Terminal...]}`: the live terminals |
| `{"type":"attach","terminal","cols","rows","history"}` | `{"channel"}`. A `paint` notification and the first output follow |
| `{"type":"detach","channel"}` | `{}`. Same as CLOSE on the channel |

`Terminal`: `{"id","name","live","status":{"state","app"}|null,"labels":{},"cols","rows"}`. It never carries screen text.

### Notifications

| Notification | Meaning |
|---|---|
| `{"type":"paint","channel"}` | The next output on the channel starts a fresh screen at offset 0 |

### Error codes

`bad-request`, `version`, `not-found`, `backend`, `closed`.

## Not yet specified

Resume (B3), Task start and agent state (B4), and everything from protocol v1 beyond terminals: Tasks, Inbox, Reader, Chat, Routines. Each is added here when it is built.
