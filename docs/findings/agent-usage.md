# Agent usage and plan limits: what muxa can read

ADR 0103 relies on these facts. Sources were read on 2026-11-24. Rows marked *local* were seen in files on a Mac with Claude Code 2.1.331 and codex-cli 0.171.0. Rows marked *docs* were not probed. `scripts/repro/agent-usage.sh` checks the status line and Codex rows on a real login (humans only, `MUXA_REPRO_LOGIN=1`). Re-check after an agent upgrade.

## Claude Code (Pro, Max, Ultra login)

| Fact | Source |
|---|---|
| The status line command gets `rate_limits` on stdin, since 2.1.80: `five_hour` and `seven_day`, each `{used_percentage, resets_at}`. `used_percentage` is 0–100. `resets_at` is Unix epoch seconds. | *docs*: code.claude.com/docs/en/statusline; CHANGELOG 2.1.80 |
| `--settings <file>` with a `statusLine` replaces the user's own status line: muxa's command ran, the one in `~/.claude/settings.json` did not run by itself. A chained command that reads the login's `settings.json` and runs its `statusLine.command` with the same stdin prints the user's line. | *local*, `agent-usage.sh claude` on the default login, 2026-11-24 |
| On the developer's default login, the second status line run (after the first reply) had `five_hour` and `seven_day`, each with an integer `used_percentage` and `resets_at` in epoch seconds. The weekly window can reset before the 5-hour one. | *local*, `agent-usage.sh claude`, 2026-11-24 |
| `rate_limits` is sent only to Pro, Max and Ultra logins, and only after the session's first API response. A window can be missing, and is dropped once its `resets_at` passes. The script runs again when a `resets_at` time is reached. 2.1.243 fixed stale percentages after a reset. | *docs*: statusline; CHANGELOG 2.1.243 |
| `spend_limit` (`used_percentage`, `resets_at`, `used_usd`, `limit_usd`, `period`) appears only behind a Claude apps gateway. | *docs*: CHANGELOG 2.1.251, 2.1.284 |
| `/usage` shows plan limits on screen only. It also holds the old `/cost` and `/stats` since 2.1.118. There is no `claude usage` subcommand. | *docs*: CHANGELOG 2.1.118, 2.1.149 |
| OTEL (`CLAUDE_CODE_ENABLE_TELEMETRY=1`) exports `claude_code.cost.usage` and `claude_code.token.usage`. No metric gives the plan's remaining allowance. | *docs*: code.claude.com/docs/en/monitoring-usage |
| Each assistant line of a session `.jsonl` has `message.usage`: `input_tokens`, `output_tokens`, `cache_creation_input_tokens`, `cache_read_input_tokens`. It has no limit fields. | *local* |
| One message is written as several assistant lines, one per content block, with the same `message.id` and the same `usage`: in three recent transcripts, 480 of 756 message ids repeated, all with identical usage. Each line also has `cwd` and an ISO `timestamp`. Subagents write their own transcripts in `projects/<project>/<session>/subagents/*.jsonl`. | *local*, 2026-11-25 |
| `GET https://api.anthropic.com/api/oauth/usage` returns the plan windows. It is undocumented and needs the user's OAuth token. muxa does not use it. | third-party crates only (`cship`, `claude-usage` on docs.rs) |
| The terms say third parties may not "collect, store, or intermediate Claude.ai credentials or session tokens". | *docs*: code.claude.com/docs/en/legal-and-compliance |
| The default login (no `CLAUDE_CONFIG_DIR`) keeps `.claude.json` in `$HOME`, not in `~/.claude`. Setting `CLAUDE_CONFIG_DIR=~/.claude` is a different config: Claude Code 2.1.331 said "Claude configuration file not found at: ~/.claude/.claude.json" and made a new one there. | *local*, `agent-usage.sh claude`, 2026-11-24 |
| Without a terminal on stdin, `claude` runs as `--print` and exits with "Input must be provided either through stdin or as a prompt argument". | *local*, same run |
| `CLAUDE_CONFIG_DIR` separates logins. The macOS Keychain entry is keyed to that directory. | *docs*: code.claude.com/docs/en/authentication; ADR 0016 |

## Codex (ChatGPT login)

| Fact | Source |
|---|---|
| app-server `account/rateLimits/read` returns `rateLimits` and `rateLimitsByLimitId`. Each has `limitId`, `limitName`, `primary` and `secondary` (`{usedPercent, windowDurationMins, resetsAt}`; `resetsAt` in Unix seconds; `secondary` may be null) and `planType`. `account/rateLimits/updated` sends the same shape. | *docs*: learn.chatgpt.com/docs/app-server |
| `account/read` returns `{type: "chatgpt", email, planType}` or `{type: "apiKey"}`. | *docs*: same |
| Rollout `token_count` events carry `info.total_token_usage` and `rate_limits` in snake_case: `primary`/`secondary` `{used_percent, window_minutes, resets_at}`, `credits`, `plan_type`, `rate_limit_reached_type`. Values seen: 300 and 10080 minutes. A window past its `resets_at` stays in the last event until the next reply. | *local*; `agent-usage.sh codex` on a Plus login, 2026-11-24 |
| `info.total_token_usage` is cumulative over the session: `input_tokens`, `cached_input_tokens`, `output_tokens`, `reasoning_output_tokens`, `cache_write_input_tokens`, `total_tokens`. `total_tokens` equals `input_tokens + output_tokens`: input includes the cached part, and output includes reasoning. `info` also has `last_token_usage` and `last_usage_includes_reasoning`, and is `null` on some events. Rollouts are filed under `sessions/YYYY/MM/DD` by the day the session started. | *local*, codex-cli 0.171.0, 2026-11-25 |
| Credentials are in `auth.json` under `CODEX_HOME` or in the keyring. An API key bills at API rates, not plan credits. | *docs*: learn.chatgpt.com/docs/auth |

## Gemini CLI

| Fact | Source |
|---|---|
| `/stats model` shows tokens and quota for the session, on screen only. | *docs*: gemini-cli `docs/reference/commands.md` |
| The quota call (`retrieveUserQuota`) is a private endpoint in the CLI's source. | gemini-cli `packages/core/src/code_assist/server.ts` |
| The quota page says Antigravity CLI replaced Gemini CLI for unpaid and Google One users on 2026-06-18. | geminicli.com/docs/resources/quota-and-pricing; not confirmed elsewhere |

## Not confirmed

- Chaining from the project's and the local settings (`.claude/settings.json`, `settings.local.json` in the working folder) was not tried; the probe chains only the login's own.
- That two `CLAUDE_CONFIG_DIR` logins, and two `CODEX_HOME` logins, stay apart.
- `rate_limits` in Codex rollouts on another plan than Plus.
- That a one-shot `codex app-server` starts no daemon in the home.
- OpenAI's and Google's terms on reading a plan login from another app.
