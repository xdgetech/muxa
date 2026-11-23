# Claude Code permission modes for unattended runs

Checked with Claude Code 2.1.330, `claude -p --model haiku --strict-mcp-config --settings <file>`, in a throwaway folder whose settings allow one script (`Bash(<folder>/ok.sh)`).

| Behavior | How checked |
|---|---|
| `--permission-mode dontAsk` lists among the modes: `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`. | `claude --help` |
| In `dontAsk`, an allowed command runs: `ok.sh` printed its output. | *probe* |
| In `dontAsk`, a command no rule allows is refused at once, not asked: `touch <folder>/made.txt` and `curl -sI https://example.com` were refused, the file was not made, and the agent said the call was refused. | *probe* |
| In `dontAsk`, read-only commands Claude Code approves by itself still run without a rule: `date` ran. | *probe* |

muxa relies on this for a Bot's Routine runs and team members (ADR 0088): nobody answers a prompt there, so an unlisted command must fail the call instead of waiting until the run's limit.
