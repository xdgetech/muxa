# 0014: The store release: a built-in demo, one paid price, R8 that only removes code

## Status

Accepted.

## Context

- The iOS and Android apps are ready for the App Store and Google Play. Both stores review an app by using it.
- muxa does nothing without a Mac or Linux host running tmux and muxa-bridge. App Review asks for full access to the app's features (guideline 2.1). A reviewer has no host.
- The in-app demo bridge existed only in debug builds, for the UI tests (`-demo`, `--ez demo true`).
- muxa has no server, so it has no running costs. Comparable apps: Blink (about $20 a year, after users disliked its move to a subscription), Prompt 3 ($20 a year or $100 once), Termius (about $10 a month), and free tools such as Happy and ServerCC.
- The apps are GPL-3.0-only in the repo. The author sells the store builds under a separate commercial license (README).

## Decision

- **The demo ships in every build, as Try the Demo.**
  - It is offered on the empty host list (the sidebar, which a phone starts on) and in the empty detail. Leave Demo goes back to the saved hosts.
  - It runs the demo bridge on its showcase data, the same data as the store screenshots.
  - It saves nothing and never opens SSH or touches the key: its host store is in memory, and the bridge is chosen before any key is loaded.
  - The launch arguments for the UI tests stay DEBUG-only. The tests keep their fixtures.
  - No server is hosted for review. The review notes (`store/apple/review_information/notes.txt`, `store/play/app_access.txt`) say where to tap.
- **The store text never calls it a demo or a trial.** Guideline 2.2 rejects trial versions; the listing calls it a "sample workspace". `make lint-store` checks this.
- **One price, paid once: $9.99, ¥1,500 in Japan, on both stores.** No subscription and no in-app purchase in 1.0. There is no cost for a subscription to pay for, and a one-time price needs no billing code. Try the Demo lets people see the app before buying.
- **France is left out of both stores for 1.0.** The SSH library ships its own standard encryption, and the App Store in France needs a French encryption declaration for that. The declaration can be filed later.
- **R8 in Android release builds only removes code** (`android/app/proguard-rules.pro`). It renames and rewrites nothing, and it keeps all of muxa, sshj, JediTerm, slf4j and BouncyCastle, which load classes by name.
  - It is on for one reason. BouncyCastle's EST package (certificate enrollment over TLS) has an `X509TrustManager` that trusts every certificate. Play's security scan rejects apps that contain one, even unused. R8 drops that package; muxa never uses TLS.
  - Checked on the release APK: no class implements a TrustManager, the demo runs, and an SSH connection reaches the host-key prompt with the host's real fingerprint.
- **The contributor agreement covers `android/` too**, so the Play build can be sold under the commercial license (`CONTRIBUTING.md`).

## Consequences

- Anyone can try the app. A buyer finds out before paying that it needs a host.
- A paid Play app can later become free, but a free one cannot become paid. A free app with a one-time unlock remains possible later, with StoreKit and Play Billing.
- The release APK is about 16 MB. Full R8, with renaming and optimization, needs a release-build test that connects over SSH (`docs/plan.md`, Next).
- `store/RELEASE.md` lists every console answer. A change to what the app stores or sends must update it and `docs/privacy.md`.
