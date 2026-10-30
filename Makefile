# One entry point for humans, agents and CI. `make check` is the bar before a commit.
.PHONY: check fmt fmt-check clippy test test-tmux test-harness test-apple test-apple-tmux app-project build-app test-app test-app-iphone test-android test-android-tmux build-android lint-android test-android-app bundle-android app-icon screenshots screenshots-android install-ipad archive-ios lint-sh lint-store repro sandbox sandbox-down

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

check: fmt-check clippy test lint-sh lint-store test-harness

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

# The store listings in store/ fit each store's limits (store/RELEASE.md).
lint-store:
	scripts/check-store-text.sh

# The Android client (android/, Gradle). The JDK defaults to Android Studio's bundled one.
ANDROID_STUDIO_JDK := /Applications/Android Studio.app/Contents/jbr/Contents/Home
ifeq ($(origin JAVA_HOME),undefined)
ifeq ($(shell test -x "$(ANDROID_STUDIO_JDK)/bin/java" && echo yes),yes)
export JAVA_HOME := $(ANDROID_STUDIO_JDK)
endif
endif
GRADLEW = ./gradlew --console=plain

# The protocol, terminal, transport and session modules: plain JVM tests, no emulator.
test-android:
	cd android && $(GRADLEW) test

# The Android app: build it, and run Android lint on it.
build-android:
	cd android && $(GRADLEW) :app:assembleDebug

lint-android:
	cd android && $(GRADLEW) :app:lintDebug

# A signed bundle for Google Play, with the upload key from ~/.gradle/gradle.properties
# (store/RELEASE.md). Play re-signs it with the app key.
bundle-android:
	cd android && $(GRADLEW) :app:bundleRelease
	@mkdir -p build/release
	cp android/app/build/outputs/bundle/release/app-release.aab build/release/muxa.aab
	@echo "Upload: build/release/muxa.aab"

# The app's UI tests against the in-app demo bridge, on the running emulator or device
# (start one first, e.g. `emulator -avd Pixel_Tablet`). Includes the fold and resize tests.
test-android-app:
	cd android && $(GRADLEW) :app:connectedDebugAndroidTest

# Also runs PaneSession against the real bridge on a throwaway tmux server.
test-android-tmux:
	cargo build -p muxa-bridge
	cd android && MUXA_BRIDGE=$(CURDIR)/target/debug/muxa-bridge $(GRADLEW) :session:test --rerun-tasks

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

# Renders the icon from apple/Muxa/Art/app-icon.swift: iOS's light, dark and tinted images,
# Android's adaptive icon layers, and Play's store icon and feature graphic.
app-icon:
	swift apple/Muxa/Art/app-icon.swift apple/Muxa/Muxa/Assets.xcassets/AppIcon.appiconset \
		android/app/src/main/res store/play

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

# Google Play screenshots into build/screenshots, from the showcase on a phone and a tablet emulator.
screenshots-android:
	scripts/screenshots-android.sh

# Humans only: archives a signed release build and exports the .ipa for App Store Connect.
# Upload it with Xcode's Organizer or Transporter (store/RELEASE.md).
RELEASE_DIR = build/release
archive-ios: app-project
	@mkdir -p $(RELEASE_DIR)
	$(XCODEBUILD) -configuration Release -destination 'generic/platform=iOS' \
		-archivePath $(RELEASE_DIR)/Muxa.xcarchive -allowProvisioningUpdates archive
	xcodebuild -exportArchive -archivePath $(RELEASE_DIR)/Muxa.xcarchive \
		-exportOptionsPlist apple/Muxa/Config/ExportOptions.plist -exportPath $(RELEASE_DIR)/ios -allowProvisioningUpdates
	@echo "Upload: $(RELEASE_DIR)/ios/Muxa.ipa"

# Re-checks every tmux fact the design relies on (docs/findings/tmux-control-mode.md).
repro:
	@set -e; for f in scripts/repro/*.sh; do echo "== $$f"; bash "$$f"; done

sandbox:
	scripts/tmux-sandbox.sh up

sandbox-down:
	scripts/tmux-sandbox.sh down
