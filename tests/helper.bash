# shellcheck shell=bash
# Each test gets its own tmux server (-L) and a short TMPDIR for fzf sockets.

ROOT="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"

setup_server() {
  unset TMUX TMUX_PANE
  TMPDIR="$(mktemp -d /tmp/ta.XXXXXX)"
  export TMPDIR
  export TMUX_AGENTS_SOCKET="ta-$$-${BATS_TEST_NUMBER}"
  # Outside any git repo, so the tree shows no branch for it.
  tx -f /dev/null new-session -d -s base -x 200 -y 50 -c "$TMPDIR"
  tx set-option -g @agents_check_gh off
  tx set-option -g @agents_notify off
  # A muxa-cli on the developer's PATH must not change what the keys do.
  tx set-option -g @agents_muxa off
}

teardown_server() {
  tx kill-server 2>/dev/null || true
  rm -rf "$TMPDIR"
}

tx() { tmux -L "$TMUX_AGENTS_SOCKET" "$@"; }

load_plugin() { "$ROOT/agents.tmux"; }

# Retry a condition for up to ~5 s.
wait_for() {
  local i
  for i in $(seq 50); do
    eval "$1" && return 0
    sleep 0.1
  done
  echo "timed out waiting for: $1" >&2
  return 1
}

strip_ansi() { sed $'s/\x1b\\[[0-9;]*m//g'; }

pane_opt() { tx display-message -p -t "$1" "#{$2}"; }

# A repo with one commit on main, at $REPO.
make_repo() {
  REPO="$TMPDIR/repos/app"
  mkdir -p "$REPO"
  git -C "$REPO" init -q -b main
  git -C "$REPO" -c user.email=t@t -c user.name=t commit -q --allow-empty -m init
}
