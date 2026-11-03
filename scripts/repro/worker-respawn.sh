#!/usr/bin/env bash
# W0: exercise explicit fake-worker commands through muxa-tmux's quoting path.
# No native agent, account or default tmux server is used.
set -euo pipefail
. "$(dirname "$0")/../lib/tmux.bash"
cargo test --manifest-path "$MUXA_LIB/../../Cargo.toml" -p muxa-tmux \
  --features tmux-integration --test tmux_smoke worker_ -- --nocapture
