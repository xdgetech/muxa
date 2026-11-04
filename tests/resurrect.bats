#!/usr/bin/env bats
load helper
setup() { setup_server; }
teardown() { teardown_server; }
metadata() { python3 "$ROOT/scripts/resurrect.py" "$@"; }

@test "metadata follows session names across new ids and mixed pane directories" {
  make_repo
  tx new-session -d -s task -c "$REPO"
  sid=$(tx display-message -p -t =task: '#{session_id}')
  tx split-window -d -t "$sid" -c "$TMPDIR"
  tx set-option -t "$sid" @agent_repo 'app "quoted"'
  tx set-option -t "$sid" @agent_worktree "$REPO"
  tx set-option -t "$sid" @muxa_task t-example
  tx set-option -p -t "$sid" @agent_state running
  printf 'pane\tplaceholder\n' > "$TMPDIR/snapshot"
  metadata save "$TMPDIR/snapshot"
  tx kill-session -t "$sid"
  tx new-session -d -s task -c "$TMPDIR"
  restored=$(tx display-message -p -t =task: '#{session_id}')
  [ "$sid" != "$restored" ]
  metadata restore "$TMPDIR/snapshot"
  [ "$(pane_opt "$restored" @agent_repo)" = 'app "quoted"' ]
  [ "$(pane_opt "$restored" @agent_worktree)" = "$REPO" ]
  [ "$(pane_opt "$restored" @muxa_task)" = t-example ]
  [ -z "$(pane_opt "$restored" @agent_state)" ]
}

@test "device sessions are excluded and existing live tags are retained" {
  tx set-option -t '$0' @agent_repo app
  tx new-session -d -s device
  tx set-option -t '$1' @muxa_device 1
  tx set-option -t '$1' @agent_repo hidden
  metadata save "$TMPDIR/snapshot"
  ! grep -q hidden "$TMPDIR/snapshot"
  tx set-option -t '$0' @agent_repo other
  metadata restore "$TMPDIR/snapshot"
  [ "$(pane_opt '$0' @agent_repo)" = other ]
}

@test "old snapshots are harmless and unsupported tags cannot execute commands" {
  printf 'pane\told\n' > "$TMPDIR/snapshot"
  metadata restore "$TMPDIR/snapshot"
  printf 'agents-metadata\t{"version":1,"sessions":[{"name":"base","tags":{"@bad":"payload"}}]}\n' > "$TMPDIR/snapshot"
  run metadata restore "$TMPDIR/snapshot"
  [ "$status" -ne 0 ]
  [ -z "$(pane_opt '$0' @bad)" ]
}

@test "hook registration is idempotent and preserves original filename arguments" {
  tx set-option -g @resurrect-hook-post-save-layout 'printf "%s\n" > "$TMPDIR/previous"'
  metadata install
  metadata install
  tx set-option -t '$0' @agent_repo app
  snapshot="$TMPDIR/snapshot with spaces"
  # Match resurrect's quoted argument invocation, without evaluating test data.
  python3 - "$snapshot" <<'PY'
import os, shlex, subprocess, sys
command = subprocess.check_output(['tmux', '-L', os.environ['TMUX_AGENTS_SOCKET'],
    'show-options', '-gqv', '@resurrect-hook-post-save-layout'], text=True).strip()
subprocess.run(['sh', '-c', command + ' ' + shlex.quote(sys.argv[1])], check=True)
PY
  [ "$(cat "$TMPDIR/previous")" = "$snapshot" ]
  grep -q '^agents-metadata' "$snapshot"
  tx set-option -u -t '$0' @agent_repo
  mkdir "$TMPDIR/saved"
  cp "$snapshot" "$TMPDIR/saved/last"
  tx set-option -g @resurrect-dir "$TMPDIR/saved"
  metadata hook restore
  [ "$(pane_opt '$0' @agent_repo)" = app ]
}

@test "restoring an older selected snapshot uses its own metadata" {
  tx set-option -t '$0' @agent_repo first
  metadata save "$TMPDIR/first"
  tx set-option -t '$0' @agent_repo second
  metadata save "$TMPDIR/second"
  tx set-option -u -t '$0' @agent_repo
  metadata restore "$TMPDIR/first"
  [ "$(pane_opt '$0' @agent_repo)" = first ]
}
