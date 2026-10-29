# One entry point for humans, agents and CI. `make check` is the bar before a commit.
.PHONY: check fmt fmt-check clippy test test-tmux test-harness test-apple test-apple-tmux app-project build-app test-app test-app-iphone app-icon screenshots install-ipad lint-sh repro sandbox sandbox-down

SHELL_SCRIPTS := $(shell find scripts -name '*.sh' -o -name '*.bash')
SHELLCHECK ?= shellcheck

# xcodebuild and the swift-testing macros need Xcode, not the Command Line Tools.
# Use Xcode when xcode-select points to the tools and Xcode is installed.
ifeq ($(origin DEVELOPER_DIR),undefined)
ifneq ($(findstring CommandLineTools,$(shell xcode-select -p 2>/dev/null)),)
ifneq ($(wildcard /Applications/Xcode.app/Contents/Developer),)
export DEVELOPER_DIR := /Applications/Xcode.app/Contents/Developer
endif
endif
endif

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

# MuxaKit on macOS. Needs Xcode's toolchain (swift-testing); see DEVELOPER_DIR above.
test-apple:
	swift test --package-path apple/MuxaKit

# Also runs PaneSession against the real bridge on a throwaway tmux server.
test-apple-tmux:
	cargo build -p muxa-bridge
	MUXA_TEST_BRIDGE=$(CURDIR)/target/debug/muxa-bridge swift test --package-path apple/MuxaKit

lint-sh:
	$(SHELLCHECK) -x $(SHELL_SCRIPTS)

# The iPad and iPhone app. The Xcode project is generated from apple/Muxa/project.yml (XcodeGen).
# SwiftTerm has a build plugin; command-line builds skip Xcode's trust prompt for it.
MUXA_SIM ?= iPad mini (A17 Pro)
MUXA_SIM_IPHONE ?= iPhone 18 Pro Max
XCODEBUILD = xcodebuild -project apple/Muxa/Muxa.xcodeproj -scheme Muxa -skipPackagePluginValidation

app-project:
	cd apple/Muxa && xcodegen generate

build-app: app-project
	$(XCODEBUILD) -destination 'generic/platform=iOS Simulator' build

# UI test against the in-app demo bridge; no host needed.
test-app: app-project
	$(XCODEBUILD) -destination 'platform=iOS Simulator,name=$(MUXA_SIM)' test

# The same UI test on an iPhone.
test-app-iphone: app-project
	$(XCODEBUILD) -destination 'platform=iOS Simulator,name=$(MUXA_SIM_IPHONE)' test

# Renders the icon's light, dark and tinted images from apple/Muxa/Art/app-icon.swift.
app-icon:
	swift apple/Muxa/Art/app-icon.swift apple/Muxa/Muxa/Assets.xcassets/AppIcon.appiconset

# App Store screenshots into build/screenshots, README images into docs/images.
screenshots:
	scripts/screenshots.sh

# Builds, signs, installs and launches the app on a paired iPad (USB or "Connect via network").
# MUXA_DEVICE is the device name or identifier from `xcrun devicectl list devices`.
IOS_BUILD = build/ios
install-ipad: app-project
	@test -n "$(MUXA_DEVICE)" || { echo "set MUXA_DEVICE (see: xcrun devicectl list devices)"; exit 2; }
	@mkdir -p $(IOS_BUILD)
	@# Over SSH the login keychain is locked, and codesign fails with only errSecInternalComponent.
	@set -o pipefail; $(XCODEBUILD) -destination 'generic/platform=iOS' -derivedDataPath $(IOS_BUILD) \
		-allowProvisioningUpdates build 2>&1 | tee $(IOS_BUILD)/xcodebuild.log || { \
		if grep -q errSecInternalComponent $(IOS_BUILD)/xcodebuild.log; then \
			echo; echo "codesign could not use the signing key: the login keychain is locked."; \
			echo "Unlock it, then run make install-ipad again:"; \
			echo "  security unlock-keychain ~/Library/Keychains/login.keychain-db"; \
		fi; exit 1; }
	xcrun devicectl device install app --device '$(MUXA_DEVICE)' $(IOS_BUILD)/Build/Products/Debug-iphoneos/Muxa.app
	xcrun devicectl device process launch --terminate-existing --device '$(MUXA_DEVICE)' \
		"$$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' $(IOS_BUILD)/Build/Products/Debug-iphoneos/Muxa.app/Info.plist)"

# Re-checks every tmux fact the design relies on (docs/findings/tmux-control-mode.md).
repro:
	@set -e; for f in scripts/repro/*.sh; do echo "== $$f"; bash "$$f"; done

sandbox:
	scripts/tmux-sandbox.sh up

sandbox-down:
	scripts/tmux-sandbox.sh down
