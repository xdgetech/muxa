# One entry point for humans, agents and CI. `make check` is the bar before a commit.
.PHONY: check fmt fmt-check clippy test test-tmux test-harness test-apple test-apple-tmux lint-sh repro sandbox sandbox-down

SHELL_SCRIPTS := $(shell find scripts -name '*.sh' -o -name '*.bash')
SHELLCHECK ?= shellcheck

check: fmt-check clippy test lint-sh test-harness

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets -- -D warnings

test:
	cargo test --workspace

# Integration tests on throwaway tmux servers (tmux >= 3.2).
test-tmux:
	cargo test --workspace --features muxa-tmux/tmux-integration,muxa-bridge/tmux-integration

test-harness:
	bats tests/harness

# MuxaKit on macOS. Needs Xcode's toolchain (swift-testing). When xcode-select points to the
# Command Line Tools: DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer make test-apple
test-apple:
	swift test --package-path apple/MuxaKit

# Also runs PaneSession against the real bridge on a throwaway tmux server.
test-apple-tmux:
	cargo build -p muxa-bridge
	MUXA_TEST_BRIDGE=$(CURDIR)/target/debug/muxa-bridge swift test --package-path apple/MuxaKit

lint-sh:
	$(SHELLCHECK) -x $(SHELL_SCRIPTS)

# Re-checks every tmux fact the design relies on (docs/findings/tmux-control-mode.md).
repro:
	@set -e; for f in scripts/repro/*.sh; do echo "== $$f"; bash "$$f"; done

sandbox:
	scripts/tmux-sandbox.sh up

sandbox-down:
	scripts/tmux-sandbox.sh down
