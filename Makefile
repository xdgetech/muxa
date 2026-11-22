# One entry point for humans, agents and CI. `make check` is the bar before a commit.
.PHONY: install check fmt fmt-check clippy test test-herdr test-harness test-apple test-apple-v2 app-project build-app test-app test-app-iphone build-mac-app test-mac-app test-android test-android-v2 test-android-release build-android lint-android test-android-app bundle-android app-icon install-ipad install-iphone install-mac ios-device-app archive-ios lint-sh lint-store

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

# Humans only: the herdr-core client against the running herdr-core (ADR 0067).
# It adds one labelled terminal per test and removes it.
test-herdr:
	MUXA_TEST_HERDR=1 cargo test -p muxa-herdr --features herdr-integration
	MUXA_TEST_HERDR=1 cargo test -p muxa-server --features herdr-integration --test herdr_live
	MUXA_TEST_HERDR=1 cargo test -p muxa-bridge --features herdr-integration v2::

test-harness:
	bats tests/harness
	python3 -m unittest discover -s tests/harness -p 'test_ios_simulator.py'

# MuxaKit on macOS. Needs Xcode's toolchain (swift-testing); see DEVELOPER_DIR above.
test-apple:
	swift test --package-path apple/MuxaKit

# Also runs the protocol v2 client against the real server on the fake backend.
test-apple-v2:
	cargo build -p muxa-server --example fake_server
	MUXA_TEST_V2_SERVER=$(CURDIR)/target/debug/examples/fake_server swift test --package-path apple/MuxaKit --filter MuxaV2Tests

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

# The terminal, transport, session and v2 modules: plain JVM tests, no emulator.
test-android:
	cd android && $(GRADLEW) test

# The Android v2 client, also against the real server on the fake backend.
test-android-v2:
	cargo build -p muxa-server --example fake_server
	cd android && MUXA_TEST_V2_SERVER=$(CURDIR)/target/debug/examples/fake_server $(GRADLEW) :v2:test

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

# The app's UI tests against the in-app v2 demo server, on the running emulator or device
# (start one first, e.g. `emulator -avd Pixel_Tablet`).
test-android-app:
	cd android && $(GRADLEW) :app:connectedDebugAndroidTest

# The minified release build connects over SSH to a throwaway host, which runs the v2
# server on the fake backend, and shows a terminal (ADR 0029). Boots a read-only emulator unless ANDROID_SERIAL names a device.
test-android-release:
	cargo build -p muxa-server --example fake_server
	scripts/test-android-release.sh

# The iPad and iPhone app. The Xcode project is generated from apple/Muxa/project.yml (XcodeGen).
# SwiftTerm has a build plugin; command-line builds skip Xcode's trust prompt for it.
export MUXA_SIM MUXA_SIM_IPHONE
XCODEBUILD = xcodebuild -project apple/Muxa/Muxa.xcodeproj -scheme Muxa -skipPackagePluginValidation

app-project:
	cd apple/Muxa && xcodegen generate

build-app: app-project
	$(XCODEBUILD) -destination 'generic/platform=iOS Simulator' build

# The Mac app (ADR 0070), signed to run on this Mac only: the device key's keychain
# entitlement needs a provisioning profile, which a human creates in Xcode.
# MAC_SIGNED=1 (humans only) signs with DEVELOPMENT_TEAM from Local.xcconfig and
# lets Xcode create that profile for the Mac it builds on, so SSH hosts work.
ifeq ($(MAC_SIGNED),1)
MAC_SIGNING = -allowProvisioningUpdates
else
MAC_SIGNING = CODE_SIGN_IDENTITY=- CODE_SIGN_ENTITLEMENTS= DEVELOPMENT_TEAM=
endif
MAC_XCODEBUILD = xcodebuild -project apple/Muxa/Muxa.xcodeproj -scheme MuxaMac -skipPackagePluginValidation \
	-destination 'platform=macOS' $(MAC_SIGNING)
build-mac-app: app-project
ifeq ($(MAC_SIGNED),1)
	@team="$$($(MAC_XCODEBUILD) -showBuildSettings 2>/dev/null | awk '$$1 == "DEVELOPMENT_TEAM" { print $$3 }')"; \
	test -n "$$team" || { echo "No team: put DEVELOPMENT_TEAM = <team id> in apple/Muxa/Config/Local.xcconfig."; exit 1; }; \
	security find-identity -v -p codesigning | grep -q "Apple Development" || { \
	  echo "No Apple Development certificate on this Mac. In Xcode: Settings > Accounts, sign in,"; \
	  echo "pick team $$team, Manage Certificates > + > Apple Development. Then run this again."; exit 1; }
endif
	$(MAC_XCODEBUILD) build

# Humans only: builds the Mac app (signed to run on this Mac), replaces
# /Applications/Muxa.app (MAC_APPS picks another folder) and opens it. The app
# keeps running with its window closed, so a running one is quit first.
MAC_APPS ?= /Applications
install-mac: build-mac-app
	@app="$$($(MAC_XCODEBUILD) -showBuildSettings 2>/dev/null | awk '$$1 == "BUILT_PRODUCTS_DIR" { print $$3 }')/Muxa.app"; \
	test -d "$$app" || { echo "no built app at $$app"; exit 1; }; \
	id="$$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$$app/Contents/Info.plist")"; \
	osascript -e "tell application id \"$$id\" to quit" >/dev/null 2>&1 || true; \
	rm -rf "$(MAC_APPS)/Muxa.app" && cp -R "$$app" "$(MAC_APPS)/" && open "$(MAC_APPS)/Muxa.app" && \
	echo "Muxa is installed in $(MAC_APPS) and open."

# Its UI tests on the in-app v2 demo server. macOS asks once to allow UI automation.
test-mac-app: app-project
	$(MAC_XCODEBUILD) test

# UI test against the in-app demo bridge; no host needed.
test-app:
	python3 scripts/ios-simulator.py run ipad-mini -- bash scripts/test-app.sh

# The same UI test on an iPhone.
test-app-iphone:
	python3 scripts/ios-simulator.py run iphone -- bash scripts/test-app.sh

# Renders the icon from apple/Muxa/Art/app-icon.swift: iOS's light, dark and tinted images,
# the Mac's sizes, Android's adaptive icon layers, and Play's store icon and feature graphic.
app-icon:
	swift apple/Muxa/Art/app-icon.swift apple/Muxa/Muxa/Assets.xcassets/AppIcon.appiconset \
		android/app/src/main/res store/play

# Installs the bridge and muxa-cli on this host, then restarts a running daemon
# so it runs the new version. PREFIX picks another root.
PREFIX ?= $(HOME)/.local
install:
	cargo install --locked --path crates/muxa-bridge --root $(PREFIX)
	cargo install --locked --path crates/muxa-cli --root $(PREFIX)
	@if $(PREFIX)/bin/muxa-bridge daemon status 2>/dev/null | grep -q '^running'; then \
		$(PREFIX)/bin/muxa-bridge daemon restart; \
	fi

# Humans only: build, sign, install and launch the app on a paired device (USB or
# "Connect via network"). Each target reads its device's name or identifier from
# the environment; list them with `xcrun devicectl list devices`.
#   MUXA_IPHONE  the iPhone, for make install-iphone
#   MUXA_IPAD    the iPad, for make install-ipad (MUXA_DEVICE still works)
IOS_BUILD = build/ios
IOS_APP = $(IOS_BUILD)/Build/Products/Debug-iphoneos/Muxa.app
MUXA_IPAD ?= $(MUXA_DEVICE)

install-iphone:
	@$(call need_device,$(MUXA_IPHONE),MUXA_IPHONE)
	@$(MAKE) --no-print-directory ios-device-app
	@$(call install_on_device,$(MUXA_IPHONE),the iPhone)

install-ipad:
	@$(call need_device,$(MUXA_IPAD),MUXA_IPAD)
	@$(MAKE) --no-print-directory ios-device-app
	@$(call install_on_device,$(MUXA_IPAD),the iPad)

# Checked before the build, so a missing variable fails at once.
define need_device
test -n "$(1)" || { echo "set $(2) to the device's name or identifier (see: xcrun devicectl list devices)"; exit 2; }
endef

# $(1) the device, $(2) its name in messages. A locked device still gets the
# app; only the launch waits for the user to unlock it.
define install_on_device
xcrun devicectl device install app --device '$(1)' $(IOS_APP) && \
{ xcrun devicectl device process launch --terminate-existing --device '$(1)' \
	"$$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' $(IOS_APP)/Info.plist)" >/dev/null 2>&1 \
	&& echo "Muxa is installed and running on $(2)." \
	|| echo "Muxa is installed on $(2). Unlock the device and open Muxa (the launch needs it unlocked)."; }
endef

# The signed device build both install targets share.
ios-device-app: app-project
	@mkdir -p $(IOS_BUILD)
	@# Over SSH the login keychain is locked, and codesign fails with only errSecInternalComponent.
	@# SYMROOT and OBJROOT: Xcode's own build location setting otherwise moves the
	@# products out of $(IOS_BUILD), and the app installed below would be an old one.
	@set -o pipefail; $(XCODEBUILD) -destination 'generic/platform=iOS' -derivedDataPath $(IOS_BUILD) \
		SYMROOT=$(CURDIR)/$(IOS_BUILD)/Build/Products OBJROOT=$(CURDIR)/$(IOS_BUILD)/Build/Intermediates.noindex \
		-allowProvisioningUpdates build 2>&1 | tee $(IOS_BUILD)/xcodebuild.log || { \
		if grep -q errSecInternalComponent $(IOS_BUILD)/xcodebuild.log; then \
			echo; echo "codesign could not use the signing key: the login keychain is locked."; \
			echo "Unlock it, then run the install again:"; \
			echo "  security unlock-keychain ~/Library/Keychains/login.keychain-db"; \
		fi; exit 1; }

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
