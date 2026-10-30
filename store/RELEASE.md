# Releasing Muxa on the App Store and Google Play

Everything a human enters in App Store Connect and the Play Console, in order. The listing text is in this folder, in the fastlane `deliver` and `supply` layouts, so fastlane can upload it later. For now, copy it by hand.

| What | Where |
|---|---|
| App Store text | `store/apple/{en-US,ja}/*.txt`; review notes in `store/apple/review_information/notes.txt` |
| Play text | `store/play/{en-US,ja-JP}/*.txt`; reviewer steps in `store/play/app_access.txt` |
| Play icon and feature graphic | `store/play/{en-US,ja-JP}/images/` (`make app-icon`) |
| Screenshots | `build/screenshots/` (`make screenshots`, `make screenshots-android`) |
| Privacy policy | `docs/privacy.md`, URL `https://github.com/xdgetech/muxa/blob/main/docs/privacy.md` |

`make lint-store` (part of `make check`) checks every length limit and that the listing never says "demo" or "trial". App Review guideline 2.2 rejects trial versions, so the store text calls the built-in demo a "sample workspace".

## Price

**$9.99, paid once, on both stores.** In Japan: ¥1,500.

- muxa has no server, so there is no running cost for a subscription to pay for. Developers pushed back when Blink moved to a subscription.
- It costs less than Prompt 3 ($20 a year or $100 once) and Termius (about $10 a month). It costs more than the free tools (Happy, ServerCC). The listing names what they lack: the real pane at its size, the layout never changing, the Reader, and approval cards.
- The sample workspace lets people see the app before they pay.
- A paid Play app can later become free, but a free one can never become paid. So start paid.
- **Later, if few people buy:** a launch discount, or a free app with a one-time unlock. That needs StoreKit and Play Billing code, so it is not in 1.0.

Fees: Apple's Small Business Program and Google's 15% tier each take 15% instead of 30%. Both need enrolling (below).

## Before the first upload

### Apple (the Account Holder)

1. App Store Connect → Business: sign the **Paid Apps Agreement**, and complete the tax forms and the bank account. No price can be set before this.
2. Enroll in the **App Store Small Business Program** (developer.apple.com/app-store/small-business-program). It applies from the month after approval.
3. If the seller is in Japan: Apple collects and pays the Japanese consumption tax on sales in Japan.

### Google

1. Play Console → Settings → Payments profile: set up the **merchant account**. A paid app needs it.
2. Play Console → Account details → **15% service fee tier**: enroll, and list any associated developer accounts.
3. If the seller is in Japan: Google does not collect the Japanese consumption tax for Japan-based developers. Declare it yourself.
4. **The upload key.** Create it once and keep it, with its passwords, in your password manager:
   ```sh
   keytool -genkeypair -v -keystore ~/keys/muxa-upload.jks -alias upload \
     -keyalg RSA -keysize 4096 -validity 10000
   ```
   Then add to `~/.gradle/gradle.properties`, never to the repo:
   ```properties
   MUXA_UPLOAD_STORE_FILE=/Users/you/keys/muxa-upload.jks
   MUXA_UPLOAD_STORE_PASSWORD=...
   MUXA_UPLOAD_KEY_ALIAS=upload
   MUXA_UPLOAD_KEY_PASSWORD=...
   ```
   Google keeps the app signing key (Play App Signing). If the upload key is lost, Google can reset it; the app key never leaves Google.

## App Store Connect

### Build

1. `make check`.
2. `make archive-ios` (humans only: it signs with your account). It writes `build/release/ios/Muxa.ipa`.
3. Upload it with Xcode → Window → Organizer, or with the Transporter app.
4. In the Organizer, the archive's **Generate Privacy Report** must list no API that `apple/Muxa/Muxa/PrivacyInfo.xcprivacy` and the packages' own manifests do not declare.

### App information

| Field | Answer |
|---|---|
| Name, subtitle | `name.txt`, `subtitle.txt` |
| Primary category | Developer Tools |
| Secondary category | Productivity |
| Content rights | No third-party content. The app shows the user's own machine. |
| Age rating | Answer **No / None** to every question: 4+. Muxa contains no AI model or chatbot of its own; it shows the command-line tools the user runs on their own machine. |

### App privacy

- Privacy policy URL: `privacy_url.txt`.
- Data collection: **"No, we do not collect data from this app."** The label then reads **Data Not Collected**.

### Export compliance

Asked for the first build of each version.

| Question | Answer |
|---|---|
| Does your app use encryption? | **Yes.** SSH encrypts the connection. |
| Which type? | **Standard encryption algorithms instead of, or in addition to, Apple's.** The SSH library ships its own (Citadel and swift-crypto). |
| Available in France? | **No.** The App Store in France needs a French encryption declaration (ANSSI). 1.0 leaves France out of the availability list. To add France later, file the declaration and upload it in App Store Connect. |

Source: App Store Connect Help, "Overview of export compliance".

### Pricing and availability

- Price: **USD 9.99**. Apple's price point for Japan is ¥1,500.
- Availability: every country or region except **France** (see Export compliance).
- Family Sharing: on, which is the default for a paid app.

### Version 1.0.0

| Field | Source |
|---|---|
| Screenshots, iPad 13" | `build/screenshots/appstore-ipad/{en,ja}/` |
| Screenshots, iPhone 6.9" | `build/screenshots/appstore-iphone/{en,ja}/` |
| Promotional text, description, keywords | `promotional_text.txt`, `description.txt`, `keywords.txt` |
| Support URL, marketing URL | `support_url.txt`, `marketing_url.txt` |
| What's New | `release_notes.txt` |
| Copyright | `2026 <the seller's legal name>` |
| Sign-in required | **No** |
| Review notes | `store/apple/review_information/notes.txt` |
| Contact | the reviewer's contact: your name, phone and email |

The Japanese texts go under Japanese, after you add it as a localization.

## Google Play Console

### Create the app

- App name: `title.txt`. Default language: English (United States).
- App, not game. **Paid.**
- Confirm the declarations: developer program policies and US export laws.

### App content

| Section | Answer |
|---|---|
| Privacy policy | the URL above |
| Ads | No ads |
| App access | **All or some functionality is restricted.** Add one instruction: name "Built-in demo, no login", with the text of `store/play/app_access.txt`. No user name or password. |
| Content rating | Category: Utility, Productivity, Communication or Other. Answer **No** to every question: Everyone, PEGI 3. |
| Target audience | **18 and over**. Not designed for children. |
| News app | No |
| Data safety | Data collected: **No**. Data shared: **No**. |
| Government app, financial features, health | No |

### Store listing

| Field | Source |
|---|---|
| Short and full description | `short_description.txt`, `full_description.txt` |
| App icon (512x512) | `store/play/en-US/images/icon.png` |
| Feature graphic (1024x500) | `store/play/{en-US,ja-JP}/images/featureGraphic.png` |
| Phone screenshots | `build/screenshots/play-phone/{en,ja}/` |
| 7-inch and 10-inch tablet screenshots | `build/screenshots/play-tablet/{en,ja}/`, the same set in both slots |
| Category | Tools |
| Contact email | the support address shown on the listing |

Add Japanese under Store listing → Manage translations, with the `ja-JP` texts and images.

### Price and countries

- Price: **USD 9.99**. Set **Japan to ¥1,500** by hand; let Google convert the others.
- Countries: all, **except France**, for the same reason as on the App Store. Google does not ask about encryption, but the French rule is French law, not Apple's.
- Google Play Family Library: your choice. On matches Apple's Family Sharing.

### Release

1. `make check`, then `make bundle-android`. It writes `build/release/muxa.aab`, signed with the upload key.
2. Production → Create new release. Accept **Play App Signing** with a key Google generates.
3. Upload `muxa.aab`. Release notes: `changelogs/1.txt` for each language.
4. Review and roll out.

An organization account needs no closed test before production. A personal account opened after November 2023 does: 12 testers for 14 days.

## Every later release

1. Raise the build numbers:
   - iOS: `CURRENT_PROJECT_VERSION` in `apple/Muxa/project.yml`, and `MARKETING_VERSION` for a new version.
   - Android: `versionCode`, and `versionName`, in `android/app/build.gradle.kts`.
2. Add `store/play/*/changelogs/<versionCode>.txt`, and update both `release_notes.txt`.
3. `make screenshots` and `make screenshots-android` if the UI changed.
4. Build and upload as above.
