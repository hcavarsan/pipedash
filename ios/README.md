# iOS release

The iOS app is the `crates/pipedash-desktop` Tauri app built for iOS. The Xcode project is generated on every
build (`crates/pipedash-desktop/gen` is gitignored), so everything iOS needs lives here or in
`crates/pipedash-desktop/Info.ios.plist`.

| Path | What it is |
| --- | --- |
| `fastlane/Fastfile` | Lanes: `check`, `screenshots`, `beta`, `release` |
| `xcode/PrivacyInfo.xcprivacy` | Privacy manifest copied into the app target |
| `xcode/pipedash.entitlements` | App entitlements (`aps-environment` for push) |
| `fastlane/metadata/en-US/release_notes.txt` | "What's New" sent with `release` |
| `fastlane/review_notes.txt` | App Review notes; `%{demo_token}` and `%{demo_repository}` are filled at release time |
| `../crates/pipedash-desktop/icons/ios/app-icon-1024.png` | Full-bleed App Store icon without alpha |

## Versions

- App version: the release version with the major bumped, because the App Store already shipped `1.0`.
  Release `v0.4.0` ships as `1.4.0`. `mise run bump:*` keeps working; the Fastfile derives the iOS version.
- Build number: the latest build in App Store Connect plus one.

## Workflows

- `ios-check.yml` runs on pull requests that touch the app. It builds an unsigned IPA and runs `verify_ipa`
  (Info.plist keys, export compliance flag, app icon, privacy manifest, and every required-reason API found in
  the binary is declared in the manifest). A second job launches the app on iPhone and iPad simulators and
  uploads screenshots as the `ios-simulator-screenshots` artifact.
- `ios.yml` runs on every `v*` tag with the `beta` lane (TestFlight) and manually with `beta` or `release`
  (submits for App Review). It uses the `app-store` environment, which only allows `main` and `v*` tags.

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

## Push notifications

The app registers its APNs device token with the push relay and pairs with a self-hosted pipedash-web server,
which sends failure events to the relay. Deploying the relay: `docs/push-relay.md`.
