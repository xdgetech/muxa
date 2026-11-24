#!/usr/bin/env bash
# Facts (docs/findings/agent-usage.md, ADR 0091 S0): what a login reports
# about its plan limits. Needs a real login, so humans only, with
# MUXA_REPRO_LOGIN=1; CI skips. It keeps only rate limit fields, never a
# transcript, a prompt or a token.
#
#   agent-usage.sh claude <CLAUDE_CONFIG_DIR>  status line via --settings
#   agent-usage.sh codex <CODEX_HOME>          rate_limits in the newest rollout
set -euo pipefail

if [ "${MUXA_REPRO_LOGIN:-}" != 1 ]; then
  printf 'SKIP  set MUXA_REPRO_LOGIN=1 and name a login folder\n'
  exit 0
fi
if [ $# -ne 2 ] || [ ! -d "$2" ]; then
  printf 'usage: %s claude|codex <login folder>\n' "$0" >&2
  exit 2
fi
here="$(cd "$(dirname "$0")" && pwd)"
home="$(cd "$2" && pwd)"

case "$1" in
claude)
  command -v claude >/dev/null || { printf 'FAIL  claude is not on PATH\n'; exit 1; }
  # Without a terminal, claude falls back to --print and asks for a prompt.
  [ -t 0 ] && [ -t 1 ] || { printf 'FAIL  run this in a terminal, not through a pipe\n'; exit 1; }
  dir="$(mktemp -d "${TMPDIR:-/tmp}/muxa-usage.XXXXXX")"
  trap 'rm -rf "$dir"' EXIT
  cat >"$dir/statusline.sh" <<EOF
#!/usr/bin/env bash
python3 -I "$here/agent_usage.py" statusline "$home" "$dir"
EOF
  chmod 700 "$dir/statusline.sh"
  printf '{"statusLine":{"type":"command","command":"%s"}}\n' "$dir/statusline.sh" >"$dir/settings.json"
  mkdir "$dir/work"
  printf 'Claude Code starts in a throwaway folder with this login.\n'
  printf 'Send one short message and wait for the full answer: rate_limits come only after the first reply. Then quit with /exit.\n'
  # The default login keeps .claude.json in $HOME, not in ~/.claude: setting
  # CLAUDE_CONFIG_DIR=~/.claude makes a new, empty config there.
  if [ "$home" = "$HOME/.claude" ]; then
    (cd "$dir/work" && env -u CLAUDE_CONFIG_DIR claude --settings "$dir/settings.json")
  else
    (cd "$dir/work" && CLAUDE_CONFIG_DIR="$home" claude --settings "$dir/settings.json")
  fi
  python3 -I "$here/agent_usage.py" report "$dir"
  ;;
codex)
  python3 -I "$here/agent_usage.py" codex "$home"
  ;;
*)
  printf 'usage: %s claude|codex <login folder>\n' "$0" >&2
  exit 2
  ;;
esac
