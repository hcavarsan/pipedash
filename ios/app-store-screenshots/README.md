# Pipedash App Store screenshots

Edit the English iPhone and iPad decks without changing the production app.

Use Node.js `^22.22.3 || >=24.8.0` and Bun. The E2E runner sets the Node.js minimum.

From this directory:

```sh
bun install --frozen-lockfile
bun dev --hostname 127.0.0.1 --port 3000
```

Open [the editor](http://127.0.0.1:3000). Select iPhone or iPad, edit the copy or drag the device, and click "Export bundle". Each deck has five framed product captures and a feature-list closer. Changes save to `app-store-screenshots.json`. Keep that file and `public/screenshots/` together.

## Design

The customized `midnight-glow-pro` preset appears as "Pipedash Flat" in the editor. It follows [Pipedash design](../../DESIGN.md): system typography, solid surfaces, quiet separators and blue emphasis. There are no decorative beams, halos, textures or glass cards. The first two slides share a thin baseline across the connected canvas. Headlines and product content stay inside their own crops.

Palette values come from `src/theme/index.ts` and the iOS surface roles in `src/App.css`. The editor's theme entry in `src/lib/constants.ts` holds the screenshot palette: iOS cell background, gray foregrounds and blue shade 3 for text emphasis. Shade 7 is the light-background accent. Captured UI retains the app's own status tokens.

The template supplies the physical iPhone and iPad bezels. Apple licenses those assets for Apple-platform app mockups. Android and Mac decks stay empty.

## Captures

The images show the real Pipedash frontend with synthetic repositories, runs, provider incidents and notifications. They are Chromium captures with iOS user agents, not native iOS simulator captures. They contain no real credentials or customer data. The capture script allows only a loopback app URL, blocks external HTTP requests and rejects unexpected API calls or mutations.

From the repository root, start the application frontend:

```sh
bun install --frozen-lockfile
bun run dev --host 127.0.0.1 --port 5198 --strictPort
```

In another terminal, from the repository root:

```sh
node ios/app-store-screenshots/scripts/capture-app.mjs
```

The script uses the editor's Playwright dependency and `/usr/bin/chromium`. Set `CHROMIUM_PATH` for another Chromium executable. `CAPTURE_APP_URL` accepts another loopback frontend URL. It checks output dimensions, populated workflow inputs, unexpected requests and page errors.

Captures live in `public/screenshots/apple/{iphone|ipad}/en/01.png` through `05.png`, in this order:

1. Unified pipelines across GitHub, GitLab, Jenkins and Argo CD.
2. Workflow execution with release parameters.
3. Failed workflow and run history.
4. Provider health and an incident from its status page.
5. Failure, outage and recovery notifications.

Reload the editor after recapturing so it reads the updated images. The source sizes are 1320×2868 for iPhone and 2064×2752 for iPad.

## Exports and checks

`exports/` contains the generated bundles and thumbnail previews. Git ignores this directory because the editor can regenerate it.

| Deck | Slides | Export sizes | PNGs |
|---|---|---|---|
| iPhone | 6 | 1320×2868, 1284×2778, 1206×2622, 1125×2436 | 24 |
| iPad | 6 | 2064×2752, 2048×2732 | 12 |

TypeScript and the production build pass with `bun run typecheck` and `bun run build`. Export checks cover exact dimensions, opaque RGB pixels and solid corner pixels. All 12 headlines fit their canvas widths. At the solid background, headline contrast is 15.46:1, blue emphasis is 8.01:1 and eyebrow text is 6.70:1. Visual review used 160px-wide thumbnails for both decks.

Before submitting, check the final files in App Store Connect and compare the browser captures with the shipped native app. Native system bars, WebKit rendering and device-specific behavior remain unverified. This setup did not upload or submit anything.

## Dependency maintenance

The 2026-10-25 update upgrades all direct dependencies to their latest stable releases and refreshes `bun.lock`. Next.js is `16.4.3`, sharp is `0.35.5` and React is `19.3.1`.

Tailwind CSS 4 uses `@tailwindcss/postcss` and the theme in `src/app/globals.css`. The migration removes `tailwind.config.ts` and Autoprefixer, updates utility names and keeps the animation plugin. The editor requires Safari 16.4+, Chrome 111+ or Firefox 128+.

The `zod` override selects `^4.6.7` because `e2e@0.21.3` pins vulnerable `4.6.1`. Remove the override when the runner uses a patched version. PostCSS now selects patched Nano ID without an override.

```sh
bun update --latest
bun outdated
bun audit
bun run typecheck
bun run test:export-fastlane
bun run build
```

Validation on 2026-10-25 found no outdated direct dependencies and no audit findings across 277 packages. TypeScript, the production build and all 5 export-validation tests passed. A disposable production editor exported both saved decks, producing 12 validated opaque RGB screenshots without changing the canonical project.

All 58 production browser tests passed, including 56 regression groups. Development-mode device switching also passed a browser smoke check. Download tests use the runner's `absolutePath`, and font-upload tests read Next.js's bundled font without depending on page preloads.

`allowedDevOrigins` explicitly permits `127.0.0.1` for the documented development command. The loopback smoke check confirmed a `200` font response and a `101` HMR WebSocket handshake.

## Fastlane publishing

From `ios/`, run `bundle exec fastlane ios prepare_screenshots` to build the editor and export both saved decks without Apple credentials. It installs Playwright Chromium unless `CHROMIUM_PATH` points to an existing browser. For example, Linux can use `CHROMIUM_PATH=/usr/bin/chromium bundle exec fastlane ios prepare_screenshots`.

The local smoke run used installed Chromium because the managed browser download timed out. It produced 12 upload-ready images in `ios/fastlane/screenshots/en-US/` and complete bundles in `exports/iphone.zip` and `exports/ipad.zip`. The five export edge tests pass with `bun run test:export-fastlane`.

The script exports through the same editor controls. Autosaves stay in browser memory. Validation checks that the canonical JSON remains unchanged and both decks are complete before replacing generated output. Supported decks use English, portrait orientation and 1 to 10 slides each. Uploads use 1320×2868 and 2064×2752 to avoid duplicate iPad display slots.

`bundle exec fastlane ios upload_screenshots` generates and uploads images to the existing editable iOS version without a binary, metadata changes or review submission. `release` includes these images in the existing App Store submission flow. `beta` leaves App Store screenshots alone.

Both publishing lanes replace all screenshot sets for `en-US`. Other locales stay unchanged. Preparation rejects extra directories in the upload folder instead of sending them. The screenshot-only lane requires the existing App Store Connect API key variables. See [iOS release instructions](../README.md#app-store-screenshots) for commands, CI and credentials.

## Copy alternatives

The decks use the outcome column. These alternatives keep the same feature order and can replace the headline in the inspector.

| Slide | Moment | Outcome | Pain |
|---|---|---|---|
| Overview | Check your pipelines. Wherever you are. | Every pipeline, one place. | Skip the dashboard hopping. |
| Run | Start a run. Away from your desk. | Run workflows. From anywhere. | No laptop. Still in control. |
| Failure | Open the failed run. | Find the failure. See the details. | Stop guessing which run failed. |
| Status | Check before you retry. | Provider status. In context. | Fewer status pages to check. |
| Notifications | See what changed. | Outage alerts. In one inbox. | Less hunting for outage updates. |
| Closer | Check. Run. Review. | Your pipelines. Under control. | Fewer dashboards to manage. |
