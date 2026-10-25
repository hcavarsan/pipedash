# iOS release

The iOS app is the `crates/pipedash-desktop` Tauri app built for iOS. The Xcode project is generated on every
build (`crates/pipedash-desktop/gen` is gitignored), so everything iOS needs lives here or in
`crates/pipedash-desktop/Info.ios.plist`.

| Path | What it is |
| --- | --- |
| `fastlane/Fastfile` | Build, simulator capture, screenshot export/upload and release lanes |
| `xcode/PrivacyInfo.xcprivacy` | Privacy manifest copied into the app target |
| `xcode/pipedash.entitlements` | App entitlements (`aps-environment` for push) |
| `fastlane/metadata/en-US/release_notes.txt` | "What's New" sent with `release` |
| `fastlane/review_notes.txt` | App Review notes; `%{demo_token}` and `%{demo_repository}` are filled at release time |
| `BuildActivity/` | WidgetKit and Dynamic Island presentation, embedded as `PipedashBuildActivity.appex` |
| `fastlane/build_activity_project.rb` | Generated target, shared Swift package, version and export profile setup |
| `../crates/pipedash-desktop/icons/ios/app-icon-1024.png` | Full-bleed App Store icon without alpha |
| `app-store-screenshots/` | Screenshot editor, saved decks, mock captures and export scripts |
| `fastlane/screenshots/en-US/` | Generated iPhone and iPad images for upload, gitignored |

## Versions

- App version: the release version with the major bumped, because the App Store already shipped `1.0`.
  Release `v0.4.0` ships as `1.4.0`. `mise run bump:*` keeps working; the Fastfile derives the iOS version.
- Build number: the latest build in App Store Connect plus one.
- The app and build activity extension use the same marketing version and build number.

## Workflows

- `ios-check.yml` runs on pull requests that touch the app and supports manual runs on a feature branch.
  It runs the Ruby helper tests and publishing guards, builds an unsigned IPA and checks the embedded extension,
  matching versions, background declarations, Info.plist keys, export compliance, app icon and required-reason APIs.
  A second job builds the same extension for the simulator, launches the app on iPhone and iPad
  and uploads screenshots as the `ios-simulator-screenshots` artifact.
- `ios.yml` runs on every `v*` tag with the `beta` lane (TestFlight). Manual runs also offer `release`
  (App Review submission with screenshots) and `upload_screenshots` (screenshots only, no submission).
  It uses the `app-store` environment, which only allows `main` and `v*` tags. Screenshot runs retain
  the generated PNGs and full bundles in the `ios-app-store-screenshots` artifact for 14 days.

## Secrets and variables (`app-store` environment unless noted)

| Name | Kind | Used by |
| --- | --- | --- |
| `MATCH_PASSWORD`, `MATCH_DEPLOY_KEY` | secret | `match` reads the certificate and profile from `pipedash-certificates-v2` |
| `ASC_KEY_ID`, `ASC_ISSUER_ID`, `ASC_KEY_CONTENT` | secret | App Store Connect API key |
| `APPLE_TEAM_ID` | repository secret | Signing team |
| `APP_REVIEW_DEMO_TOKEN` | secret | `release` only: GitHub token for the demo repository, filled into the review notes |
| `APP_REVIEW_DEMO_REPOSITORY` | variable | `release` only: `owner/repo` of the demo repository |
| `PIPEDASH_PUSH_RELAY_URL` | repository variable | Push relay URL compiled into the app; push is shown as unavailable when unset |

## Run locally (macOS)

```bash
cd ios
bundle install
bundle exec fastlane ios check
```

Renewing the certificate or profile (they expire 2027-10-24) runs `match` without `readonly` from this
directory with the App Store Connect API key; see the history of `pipedash-certificates-v2`.

## App Store screenshots

The [editor and capture instructions](app-store-screenshots/README.md) live under `app-store-screenshots/`.
Commit its project JSON and source PNGs after approving the compositions. Release exports those saved
compositions with their device mockups. It does not recapture the app or replace approved source images.

From `ios/`, generate upload-ready images without Apple credentials or an upload:

```sh
bundle exec fastlane ios prepare_screenshots
```

This installs the editor dependencies and Playwright Chromium, builds the editor and drives its export
button. Set `CHROMIUM_PATH` to use an installed Chromium instead. The script validates both decks before
replacing `fastlane/screenshots/en-US/` and leaves the canonical project JSON unchanged.

The upload directory contains one size per device: 1320×2868 for iPhone and 2064×2752 for iPad.
Uploading both exported iPad sizes would put 12 images into the same Fastlane display slot, above Apple's
10-image limit. The complete multi-size bundles remain in `app-store-screenshots/exports/`.

To update only the screenshots on the existing editable iOS version:

```sh
bundle exec fastlane ios upload_screenshots
```

This command needs `ASC_KEY_ID`, `ASC_ISSUER_ID` and `ASC_KEY_CONTENT`. It does not need signing or match
credentials. It skips the binary, metadata, version update and review submission.

Both `upload_screenshots` and `release` replace all existing screenshot sets for `en-US` before uploading
the generated iPhone and iPad sets. Other locales stay unchanged. A failed export stops either lane before
Apple authentication or upload. `beta` does not generate or upload App Store screenshots. The existing
`screenshots` lane remains the native simulator first-launch check.

These captures use the real frontend with synthetic data in Chromium. Check them against the shipped iOS
app and in App Store Connect before publishing. Native simulator rendering is a separate check.

### Tests

From `ios/`, run the colocated Ruby tests:

```sh
bundle exec ruby -e 'Dir["fastlane/*_test.rb"].sort.each { |file| require_relative file }'
```

The `ios-check.yml` workflow runs this suite before the build. Screenshot tests use temporary files and
replace external Apple actions. They check export failure gates, locale isolation, screenshot-only delivery
and TestFlight isolation. The delivery checks exercise Fastlane's runner without authenticating or uploading.

## Build activity extension

`prepare_project` adds `PipedashBuildActivity` to Tauri's generated `project.yml` before running XcodeGen.
The app embeds the extension with bundle ID `com.henrique.pipedash.BuildActivity`.
The extension targets iOS 16.2 and links only `PipedashActivityModels` from the native plugin's local Swift package.
The native plugin links the same shared product.
The helper reuses an existing package entry for that path and stages Tauri's complete iOS API package before Xcode resolves dependencies.
Swift Package Manager also needs network access for Tauri's remote `SwiftRs` dependency.

The app declares `NSSupportsLiveActivities`, the background task identifier
`com.henrique.pipedash.build-watch` and the `processing` background mode.
Build watching requires a physical device on iOS 26 or later with Live Activities and background execution available.
The simulator can check compilation and presentation, but cannot validate the continued-processing scheduler.
iOS can expire monitoring or stop it after a force quit.
The app reports interruption and does not restart monitoring automatically.
Stopping monitoring does not cancel the provider's build.

### Signing prerequisites

The signed lanes use `match` with `readonly: true` for both bundle IDs:

- `com.henrique.pipedash`
- `com.henrique.pipedash.BuildActivity`

Before the first signed build, an Apple Developer team administrator must register the extension's explicit App ID.
Keep the app's existing Push Notifications capability and provisioning entitlements.
Create an App Store distribution profile for the extension using an available Apple Distribution certificate.
Store that profile in `pipedash-certificates-v2` alongside the app's profile and certificate private key.
An extension does not need a separate App Store Connect app record.
This local ActivityKit flow does not require an App Group or a push-to-start entitlement.

An authorized maintainer can create or renew both profiles from macOS with writable access to the certificate repository:

```bash
cd ios
bundle exec fastlane match appstore --app_identifier com.henrique.pipedash,com.henrique.pipedash.BuildActivity --readonly false
```

Use the certificate repository's existing authentication and `MATCH_PASSWORD`.
The Apple account or API key must have permission to manage identifiers and distribution profiles.
The CI deploy key and read-only `match` step do not create missing profiles.
If the extension profile is absent or expired, the signed lane fails instead of switching signing modes.

Tauri archives the app with per-target manual signing settings.
Fastlane exports that archive with an explicit provisioning-profile mapping for both bundle IDs.
This avoids Tauri's single-app export mapping dropping the extension profile.
`verify_ipa` checks the embedded extension, matching versions and the signed extension's application identifier.

Run the focused Ruby checks without building or publishing:

```bash
cd ios
ruby -I fastlane -e 'Dir["fastlane/*_test.rb"].sort.each { |path| require_relative path }'
```
## Push notifications

The app registers its APNs device token with the push relay and pairs with a self-hosted pipedash-web server,
which sends failure events to the relay. Deploying the relay: `docs/push-relay.md`.
