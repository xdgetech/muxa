# Shared iOS simulators

UI tests and screenshots reuse existing devices. They never create, clone,
erase or delete a simulator. The roles are `iphone` (iPhone 18 Pro Max),
`ipad-mini` (iPad mini A17 Pro), and `ipad-pro` (13-inch iPad Pro M5).
The iPhone used for screenshots is the same one used for normal UI tests.
The 13-inch iPad retains the App Store screenshot dimensions.

## Host setup

Copy `scripts/ios-qa.example.json` to `.ios-qa.local.json` in the main checkout,
then fill in the mounted QA volume, storage directory, existing device UDIDs
and runtime identifiers from `xcrun simctl list devices --json`.
The example contains placeholders, not usable host identities.

The local JSON and `mise.local.toml` are gitignored. Never commit host UDIDs or
personal storage paths. Worktrees share the main checkout's local JSON.
For a configuration shared by different projects, use
`$XDG_CONFIG_HOME/ios-qa/config.json` (default `~/.config/ios-qa/config.json`),
or set `IOS_QA_CONFIG` to a host-owned JSON file. The explicit environment
override takes precedence over the host default and checkout-local file.
An ignored `mise.local.toml` can set `IOS_QA_CONFIG` for another project.

```sh
mise run qa:ios:check
mise run qa:ios:ipad-mini
mise run qa:ios:iphone
mise run screenshots:ios
```

`make test-app`, `make test-app-iphone` and `make screenshots` use the same
wrapper, without requiring mise. `MUXA_SIM` and `MUXA_SIM_IPHONE` still accept
an existing device's name or UDID; its configured runtime and role's device
type must match, and name resolution must be unambiguous.

## Serialization and output

`scripts/ios-simulator.py run <role> [<role> ...] -- <command> ...` is the shared
entry point. Other projects can invoke it with the same host JSON. It holds
one user-wide advisory file lock in `/tmp` for the entire command, including
project generation, builds, tests and screenshot cleanup. All participating
projects must use this wrapper; direct Xcode or simctl operations do not acquire
the lock. The lock file is never removed, so stale-file races cannot create
two lock holders. Signals are forwarded to the command's process group and the
wrapper waits for it to exit before releasing the lock.

The wrapper exports `IOS_QA_UDID` for a single role, role-specific
`IOS_QA_IPHONE_UDID`, `IOS_QA_IPAD_MINI_UDID` and `IOS_QA_IPAD_PRO_UDID`,
plus `IOS_QA_DERIVED_DATA` and `IOS_QA_OUTPUT_ROOT`.
Other test runners must also disable parallel testing to prevent Xcode-managed
clones. Muxa passes `-parallel-testing-enabled NO`.

The volume must be mounted before any output directory is created. Missing
devices, wrong types/runtimes and unavailable volumes fail without creating a
replacement or falling back to internal storage. This guard applies to these
QA entry points; the generic CI `make build-app` does not require host setup.
Simulator device data itself stays in CoreSimulator's existing device set.

DerivedData is reused per checkout under the configured storage root. Each run
gets a fresh output directory for results, logs and images. Screenshots retain
2752×2064 iPad and 1320×2868 iPhone output checks. README JPEGs are produced
under the run's `screenshots/readme/`; copy selected images into `docs/images/`
when ready. Existing runs and repository images are not removed or overwritten.
Screenshot status-bar overrides are cleared on exit, and devices that the
script booted are shut down. Already-booted devices are left running.

Simulator deletion and data erasure are separate, explicitly reviewed host
operations. No task here performs either operation or removes previous outputs.
