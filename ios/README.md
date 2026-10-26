# iOS release

The iOS app is the `crates/pipedash-desktop` Tauri app built for iOS. The Xcode project is generated on every
build (`crates/pipedash-desktop/gen` is gitignored), so everything iOS needs lives here or in
`crates/pipedash-desktop/Info.ios.plist`.

| Path | What it is |
| --- | --- |
| `fastlane/Fastfile` | Build, simulator capture, screenshot export/upload and release lanes |
| `xcode/PrivacyInfo.xcprivacy` | Privacy manifest copied into the app target |
| `xcode/pipedash.entitlements` | App entitlements without APNs access |
| `fastlane/metadata/en-US/release_notes.txt` | "What's New" sent with `release` |
| `fastlane/metadata/en-US/description.txt` | App description sent with `release`, including foreground-refresh limits |
| `fastlane/review_notes.txt` | App Review instructions with release-time demo token, repository and workflow interpolation |
| `fastlane/app_review_demo.rb` | Read-only GitHub access checks before release signing or submission |
| `fastlane/ios_project.rb` | App version, disabled-feature checks and export profile setup |
| `../crates/pipedash-desktop/icons/ios/app-icon-1024.png` | Full-bleed App Store icon without alpha |
| `app-store-screenshots/` | Screenshot editor, saved decks, mock captures and export scripts |
| `fastlane/screenshots/en-US/` | Generated iPhone and iPad images for upload, gitignored |

## Versions

- App version: the release version with the major bumped, because the App Store already shipped `1.0`.
  Release `v0.4.0` ships as `1.4.0`. `mise run bump:*` keeps working; the Fastfile derives the iOS version.
- Build number: the latest build in App Store Connect plus one.
- Minimum iOS: `15.0`, set by `bundle.iOS.minimumSystemVersion` in [`tauri.ios.conf.json`](../crates/pipedash-desktop/tauri.ios.conf.json).
  Tauri maps this value to `IPHONEOS_DEPLOYMENT_TARGET`. Its installed CLI schema also defaults to `15.0`.

## Workflows

- `ios-check.yml` runs on pull requests that touch the app and supports manual runs on a feature branch.
  It runs the Ruby helper tests and publishing guards, builds an unsigned IPA and checks the
  disabled background capabilities, Info.plist keys, export compliance, app icon and required-reason APIs.
  A second job builds for the simulator, launches the app on iPhone and iPad
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
| `PIPEDASH_PUSH_RELAY_URL` | optional repository variable | Cannot activate remote push or Watch in this disabled-feature build |

## App Review demo access

`release` checks the configured GitHub demo before Apple authentication, signing or upload. It makes only authenticated `GET` requests.
The repository must be public, active and writable by the token's user. Public access lets reviewers open its GitHub page without repository invitations.
GitHub may still require sign-in to read workflow logs in the browser.

The check finds an active `workflow_dispatch` workflow on the default branch with defaults for required inputs, a successful manual run and available jobs.
It reads at most the first 100 workflows, matching the app's workflow list. It fills that workflow's name into the review instructions.
HTTP failures stop release with the endpoint and permission guidance. Network failures omit raw exception details to protect credentials.
The lane registers the token for secret masking and rejects `--verbose`, because Fastlane's HTTP debug output can include request credentials.

Complete this checklist before each submission:

1. Use a dedicated public demo repository. Keep production code, credentials, organization secrets, deployments and production network access out of its workflows.
2. Create a fine-grained token with "Only select repositories" set to that demo repository. Grant Actions read/write and Contents read only.
   Do not use a production token or an unrestricted classic token. Set an expiry that covers review and keep the token valid until review finishes.
3. Keep a harmless `workflow_dispatch` workflow on the default branch with defaults for any required inputs. Give it enough runtime to test "Stop workflow" and retain a successful manual run with jobs.
4. Test the review instructions on an iPhone or iPad, including provider setup, run details, browser logs, "Run workflow" > "Trigger" and "Stop workflow".
   Provider export should open the native Files picker and allow a destination of your choice.

GitHub's read endpoints cannot prove the token's selected-repository restriction, Actions write permission or the workflow's harmlessness.
The repository's `permissions.push` field checks the user's repository permission, not the token's full permissions.
The checklist is required. The preflight does not dispatch or cancel a workflow to test write access.

Run the offline preflight cases without GitHub or Apple credentials:

```sh
bundle exec ruby fastlane/app_review_demo_test.rb
```

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

## Local build watching

The release build disables Watch, remote push and Live Activities. `PipedashBuildWatchEnabled` is `false` in `crates/pipedash-desktop/Info.ios.plist`.
The native plugin rejects watch starts, skips background task registration and reports Watch as unsupported.
The frontend hides Watch and remote push controls. Existing snapshots remain stored and do not restart automatically.

The app declares no build-watch background task identifiers or `processing`/`remote-notification` background modes.
It declares only the `com.henrique.pipedash.notifications-refresh` task with the `fetch` mode; `verify_ipa` requires exactly that.
It bundles no WidgetKit extension or Live Activity support. `verify_ipa` rejects these capabilities and any enabled build-watch flag before upload.
A relay URL in the environment cannot activate either feature. Changing the flag alone is not a supported release configuration.

On upgrade, the native plugin dismisses custom activities left by older versions. It retains their attribute type only for cleanup.
The disabled monitoring implementation uses iOS 26 APIs behind availability checks. It does not change the app's iOS 15.0 minimum.

### Signing prerequisites

The signed lanes use read-only `match` for `com.henrique.pipedash` only. The certificate repository must contain its App Store profile and certificate private key.
The app does not request the Push Notifications entitlement. `verify_ipa` rejects an APNs entitlement in the signed app.
An authorized maintainer can renew the app profile from macOS with writable access to the certificate repository:

```bash
cd ios
bundle exec fastlane match appstore --app_identifier com.henrique.pipedash --readonly false
```

Use the certificate repository's existing authentication and `MATCH_PASSWORD`.
The Apple account or API key must have permission to manage identifiers and distribution profiles.
The CI deploy key and read-only `match` step do not create missing profiles.

Tauri archives the app with manual signing settings. Fastlane exports the archive with the app's
provisioning-profile mapping. Fastlane's `build_app` requires `export_method: "app-store"`; keep
`app-store-connect` for the Tauri CLI and the Xcode `export_options[:method]` value.
`verify_ipa` checks the disabled-feature metadata and rejects bundled app extensions as part of its App Store checks.

## Notifications

Local failure, overdue-run and outage notifications work independently of the Watch and remote push flag.
One Notifications switch under Settings > Notifications controls all of them. On desktop and web, older separate failure and outage switches migrate to it: either one on turns it on. On iOS it starts off, because background access needs consent.
The app checks current native permission before delivery, including after permission changes in iOS Settings.

The Tauri frontend uses core's `pipelines-failed` events for new failures, including consecutive failed runs.
The web frontend compares cached pipeline status and run timestamps, including snapshots fetched after WebSocket reconnection.
Neither platform replays existing failures on first load.
Provider outages use `provider-health-alert` events. The in-app notification list does not require system notification permission and resets when the app restarts.

### Background checks on iOS

Turning notifications on asks for consent, then copies the provider configurations and tokens, the proxy URL and the status-page setting into a separate Keychain item available after first unlock on this device only.
The vault and app lock stay locked: background checks read only that copy. Provider and general-setting changes while unlocked update it, and removing a provider drops its entry. Turning notifications off or a factory reset deletes it; revoking permission in iOS Settings only stops delivery.
The current pipeline and provider state is saved as a silent baseline, so turning notifications on never reports failures or outages that already exist.

A `BGAppRefreshTask` asks iOS for a run every 15 minutes. iOS decides when it actually runs, may skip it in Low Power Mode or with Background App Refresh off, and never runs it after a force quit until the app opens again.
Each run has about 23 seconds: status pages go first, then each provider's controller check and pipelines, then at most 25 overdue-run checks across all providers, rotated so later runs cover the rest.
The run cannot use the foreground fetch scheduler, because no core context exists while the vault is locked. It reuses core's provider construction, fetch timeouts, quota pause and controller checks, and it never writes the pipeline cache.
Background and foreground share one delivery record, so a failure, overdue run, outage or recovery is notified once whichever side sees it first. A foreground resume cancels a running background check.

## Push notifications

Remote push uses the disabled `PipedashBuildWatchEnabled` flag described in [local build watching](#local-build-watching).
The app skips token synchronization and rejects pairing, unpairing and test-push commands before accessing saved settings or the network.
The native plugin unregisters from remote notifications on launch and rejects registration without requesting notification permission.
Saved pairings remain on the device. Optional local notifications remain independent and need no APNs entitlement.
The server's push APIs remain separate from this disabled iOS feature.
