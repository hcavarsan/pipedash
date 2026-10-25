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
| `../crates/pipedash-desktop/icons/ios/app-icon-1024.png` | Full-bleed App Store icon without alpha |
| `app-store-screenshots/` | Screenshot editor, saved decks, mock captures and export scripts |
| `fastlane/screenshots/en-US/` | Generated iPhone and iPad images for upload, gitignored |

## Versions

- App version: the release version with the major bumped, because the App Store already shipped `1.0`.
  Release `v0.4.0` ships as `1.4.0`. `mise run bump:*` keeps working; the Fastfile derives the iOS version.
- Build number: the latest build in App Store Connect plus one.

## Workflows

- `ios-check.yml` runs on pull requests that touch the app. It builds an unsigned IPA and runs `verify_ipa`
  (Info.plist keys, export compliance flag, app icon, privacy manifest, and every required-reason API found in
  the binary is declared in the manifest). A second job launches the app on iPhone and iPad simulators and
  uploads screenshots as the `ios-simulator-screenshots` artifact.
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

## Push notifications

The app registers its APNs device token with the push relay and pairs with a self-hosted pipedash-web server,
which sends failure events to the relay. Deploying the relay: `docs/push-relay.md`.
