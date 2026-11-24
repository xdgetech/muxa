# Claude Code permission modes for unattended runs

Checked with Claude Code 2.1.330, `claude -p --model haiku --strict-mcp-config --settings <file>`, in a throwaway folder whose settings allow one script (`Bash(<folder>/ok.sh)`).

| Behavior | How checked |
|---|---|
| `--permission-mode dontAsk` lists among the modes: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`. | `claude --help` |
| In `dontAsk`, an allowed command runs: `ok.sh` printed its output. | *probe* |
| In `dontAsk`, a command no rule allows is refused at once, not asked: `touch <folder>/made.txt` and `curl -sI https://example.com` were refused, the file was not made, and the agent said the call was refused. | *probe* |
| In `dontAsk`, read-only commands Claude Code approves by itself still run without a rule: `date` ran. | *probe* |
| In `dontAsk`, built-in tools need a rule too (2.1.331): with no allow rules, `Write` in the working folder, `Read` outside it, `WebFetch` and a claude.ai connector's tool were all refused. | *probe*, and a Bot's runs that published nothing |
| `Edit(//<dir>/**)` and `Edit(./**)` let `Write` make a file there; `Read(//<dir>/**)` lets `Read` outside the working folder. | *probe* |
| An MCP tool runs with `mcp__<server>`, `mcp__<server>__*` or `mcp__<server>__<tool>`. A wildcard over servers, `mcp__claude_ai_*`, is an invalid rule: Claude Code warns about the settings and refuses the tool. | *probe* with `mcp__claude_ai_Google_Calendar` and `list_calendars` |
| A prefix wildcard in the tool part works: `mcp__claude_ai_Google_Calendar__list_*` let `list_calendars` run and refused `get_event`. | *probe* |

muxa relies on this for a Bot's Routine runs and team members (ADR 0088): nobody answers a prompt there, so an unlisted command must fail the call instead of waiting until the run's limit.
