# Editor end-to-end verification

The product under test is this Next.js editor template. `promo/` demo applications are outside this suite. Every scaffold copies the config, tests, fixtures, harnesses and its own GitHub Actions workflow.

## Run it

Requires Node.js >=22.12, Bun 1.3.9 and installed Google Chrome. App runtime itself supports Node >=20.9. Tester Army is pinned to `e2e@0.16.0`, `@e2e-dev/web@0.11.2`, `playwright@1.63.0`. No `agent.*` steps or model credentials are used in this deterministic suite.

```bash
bun install --frozen-lockfile
bun run typecheck
bun run build
bun run test:e2e:list
bun run test:e2e
SCREENSHOTS_E2E_PRODUCTION=1 bun run test:e2e
```

`bun run test:e2e` runs 58 tests: 55 tests that directly drive/inspect the product with Tester Army (30 in `tests/editor.e2e.ts`, 11 for Style Lab, Scene Playground and the magnifier in `tests/scene-style-lab.e2e.ts`, 14 for iPhone Duo, App Store creatives and the issue 41 export regression in `tests/targets.e2e.ts`), plus three tests that run 56 existing regression groups (35 browser/export, 12 UI, nine API). Nested loops cover every advertised device size, layout, orientation, theme and built-in font. Counts refer to test/group definitions, not every loop iteration.

For one direct regression:

```bash
E2E_TELEMETRY_DISABLED=1 npx e2e run tests/editor.e2e.ts --grep 'watchos opens'
```

For the existing harnesses against a server you have already started in a **disposable** project:

```bash
bun run test:regression http://localhost:4312
BUG_BASH_FILTER='connected overlays' node tests/harness/bug-bash.cjs http://localhost:4312
```

These manual commands write the supplied server's disk project and uploads; the normal Tester Army command creates that disposable server automatically.

To exercise cold dev compilation after API-only traffic, start `node e2e-army/server.cjs 4312` from this template in one terminal, then run `node scripts/api-bug-bash.cjs http://localhost:4312` from the repository root in another. The server makes a disposable copy; stop it with Ctrl-C afterward. The equivalent standalone-scaffold command is `node tests/harness/api-bug-bash.cjs http://localhost:4312`. Run this before a browser requests the page so it covers the native ESM Tailwind config-loading regression.

## Isolation and artifacts

The runner starts `e2e-army/server.cjs` on port 4312. It makes a unique system temporary directory containing the app, symlinks only `node_modules`, then starts Next with that directory as cwd. `/api/project`, image uploads, font uploads and runtime asset routes therefore read/write the disposable copy. Teardown removes the copy. An already occupied port causes a startup failure: the config never silently reuses a live editor. Run suites sequentially because API and export harnesses mutate their isolated server project. Tests restore the API project after mutations too.

The browser provider uses Playwright's `channel: 'chrome'` to launch installed **Google Chrome**, then leases its CDP endpoint through the published `BrowserProvider` contract. No Chromium fallback. `SCREENSHOTS_E2E_HEADED=1` opens visible Chrome. Dev mode uses `next dev --webpack`; Turbopack refuses dependency symlinks outside the temporary copy. Production mode copies `.next` from the built template and runs `next start`.

The default `.e2e/` output contains:

- `report.json`: selected tests, status, steps, errors, counts and artifact links.
- `summary.md` and `junit.xml`: readable and CI summaries.
- `artifacts/`: real exported ZIP downloads, failure screenshots and retained failure traces.
- `logs/app.log`, `logs/{api-bug-bash,ui-bug-bash,bug-bash}.log`: application and individual regression group output.
- `evidence/red/report.json`: the initial failing native run proving the undo regression. Only its report and summary were retained; its original screenshots/traces were superseded by later runs.

Use the default output directory for this suite's ZIP-content checks; the published download fixture returns an attempt-relative path, so the ZIP helper finds that path beneath `.e2e/artifacts` without assuming runner-generated test IDs. CI uploads the output directory on success and failure and runs dev and production with Chrome. Dependencies install with the frozen Bun lockfile.

## Flow matrix

| Product flow | Verification |
| --- | --- |
| iOS, Mac, Android tabs and last selected device | Direct tab round-trip verifies device and preserved copy; UI harness verifies all combinations. |
| iPhone, iPad, iPhone Duo (4 decks), Apple TV, Watch, CarPlay, Mac, Android phone, 7-inch/10-inch tablets, feature graphic, App Store creatives (3 decks) | Seventeen direct deck tests edit/create/undo on each device; browser harness exports every device at every advertised size and checks screenshot pixels. |
| iPhone Duo orientations and displays | Direct test switches outer portrait → landscape → inner landscape → inner portrait → outer, edits copy, checks each deck keeps its own copy and a hand-placed rotated transform, the saved device/orientation, and reload. UI harness iterates every layout on both Duo displays in both orientations. |
| iPhone Duo and creative export sizes | Seven direct ZIP tests check exact 1398×2034, 2034×1398, 2007×2853, 2853×2007, 5244×2950, 3840×1646, 3840×2560 and 1920×1280 PNGs, file paths, PNG color type 2 and no alpha. |
| Legacy project upgrade | Direct v2 project with a hand-placed caption saves as v3 with the existing deck unchanged and every new deck present. |
| Creative guides | Direct test finds the safe-area guide and both placement previews on the editable canvas only, never in the export canvas. |
| Issue 41: cleared copy in exports | Direct test confirms the editable canvas shows the `LABEL` / `Headline goes here` hints, then exports a cleared screen and a zero-width-space screen and requires them to be pixel-identical. Against the unfixed CSS it failed with 14,379 differing bytes; it passes with the fix. |
| Portrait/landscape and every layout | UI harness iterates devices/layouts and both supported tablet orientations, checks editing/persistence. |
| Screen create, select, duplicate, delete, empty state, reorder | Direct CRUD and keyboard reorder assertions; UI harness pointer/keyboard ordering, selected copy, final-slide deletion/undo and usable empty states. |
| Undo/redo, cross-deck editing, shortcuts | Direct regression proves rapid copy edit + Add screen preserves copy after Undo on ten decks; harness checks typing, menu arrows, cross-deck history and undo/redo. |
| Inspector/canvas label, headline/tagline, copy ideas | Direct editable headline formula, locale edits and deck editing; harness inline clearing/retyping, plain paste, fallback behavior and caption appearance. |
| Text/image elements, size, transforms, layering | UI harness adds/selects/deletes text and image elements; actual pointer drag/resize; rotation, fit, fade, stacking and undo; draft-size Escape cancels. |
| Layout/background/text contrast/theme/font menus | UI harness explores layouts, secondary screenshot preservation, custom backgrounds and draft hex, typography reset; iterates built-in themes/fonts and preserves content; browser harness verifies feature-graphic contrast. |
| Screenshot picker/upload, secondary screenshots, icons | Actual PNG/JPEG round trips, inspector picker, feature graphic icon PNG composition, reloading uploaded assets; invalid files retain previous selection; race/clear latest upload wins. |
| Font import, errors and overlapping imports | Valid real WOFF2 upload/browser decode/reload/export; invalid/truncated containers rejected; latest response wins; font-load timeout unlocks controls. |
| Locale and RTL | Direct EN/DE/AR editing and computed Arabic direction; browser harness checks inline locale clearing/fallback and independent Hebrew/Latin direction. ZIP tests assert per-locale names/counts. |
| Autosave, reload, local cache, retry, concurrent revision conflict | Direct real disk edit/reload/cache and stale API revision; browser harness serialized slow saves, queued revision advance, two real tabs, invalid project load, failed-save retry, beforeunload warning. |
| Connected/isolated crops and off-canvas transforms | Direct mode toggle/undo/redo; browser harness decodes exported PNG pixels across a boundary in both modes and compares out-of-bounds composition. |
| Export ZIP/files/count/dimensions/RGB and screenshot composition | Direct real 24-PNG Watch ZIP validates filenames, EN/DE, advertised dimensions, PNG color type 2 and nonuniform pixels; harness every device/file resolution and screenshot/overlay/icon pixels. |
| Export lock, progress, failure and fallback | Harness editor becomes inert immediately through ZIP completion, downloads once, unlocks; missing/corrupt/stalled assets/font fail with actionable messages; failing/silent/throwing workers use inline encoder; encoding failure is observed safely. |
| Narrow viewport, touch, keyboard access | Direct 390px layout and keyboard reordering; UI harness 390–1440px screenshots/layout, scrollable inspector, touch actions without hover, keyboard canvas image focus and Fit active screen recentering. |
| API inputs, origin, limits, asset serving and migration | Direct invalid project/no mutation, stale conflict, corrupt image/font and foreign origins; API harness exact JSON type/origin, request byte limits including chunked upload, concurrent atomic uploads, runtime filename/traversal rejection and legacy migration. Root repository reads the live skill migration; standalone scaffold uses the bundled recipe fixture. |
| Scene Playground | Direct backdrop/flow/decoration/tilt/case edits render on every screen, persist, undo/redo as one step and reset to the classic scene; ZIP test decodes a tilted, glowing, spotlit export at exact size; a flowing spotlight deck is checked for equal brightness on every exported screen. |
| Style Lab looks, locks, saved looks, comparison | Direct four-look listing, Apply as one undo step keeping copy and layouts restorable, Colors/Layout locks across Shuffle, all-locked disables Shuffle, per-screen headline sizes survive a look, the Apply toast's Undo refuses to revert a later edit, saved looks survive reload/apply/remove, comparison PNG download is a real tall PNG. |
| Magnifier | Direct add, keyboard focus, zoom, shape, remove; ZIP export with a magnifier; partial callouts accepted by the API and completed; malformed scene/savedLooks/callout rejected; out-of-range values clamped on load. |
| Reset and cancel reset | Harness reset-all preserves app/theme/locales, editor confirmation allows cancellation; structural reset has separate undo boundary. |

## Bugs fixed and evidence

1. **Rapid structural actions erased preceding copy during undo.** Type a headline, immediately Add screen, then Undo. All ten direct deck regressions failed with original headline `First` instead of edited copy. The save/history hook previously coalesced every change within the typing window. Add, duplicate, delete, reorder and reset now start an independent history step and close the current coalescing group. Subsequent typing still coalesces. The retained initial red report contains the exact expected/observed values; the final suite verifies the repaired behavior on all ten devices.
2. **Default screen selection jumped after keyboard reordering.** Initial rendering derived the selected slide from index zero but left its ID unset. Moving that initially selected slide changed the inspector to the new first slide. Hydration now pins the actual screen ID; reordering preserves its selection. A direct keyboard regression exposed this case, which the existing harness missed because it explicitly clicked a screen first.
3. **Template framework dependency predates the September 30 security release.** Updated Next from 16.3.6 to 16.3.8 and regenerated the Bun lock. The [official release](https://github.com/vercel/next.js/releases/tag/v16.3.8) lists the repaired advisories. `bun audit --json` returned `{}` before and after; that result alone is not a security clearance. Build, types and dev/production editor tests validate compatibility; these UI tests do not prove absence of every security vulnerability.
4. **Cold dev page compilation crashed after API-only traffic.** An independent Luna follow-up exposed `ReferenceError: require is not defined` at `tailwind.config.ts:65` when the root API harness first fetched the page for its bundled font, after six API groups had passed. The same exact isolated invocation reproduced the failure and terminated Next. Replacing the animation plugin's CommonJS `require` with a static ESM import passed all nine root API groups on a fresh dev server. Both cold-start logs are retained in `.e2e/evidence/root-dev-{red,green}/`; typecheck, build and complete dev/production suites were rerun after the fix.

Harness integration also corrected trailing-slash URLs from `app.baseUrl`: `//api/upload` redirected the streaming request and Node fetch could not replay its body. All harness entry points now normalize the supplied URL. The test locator/artifact mistakes found during authoring were corrected without weakening product expectations.

## Honest limits

- Apple's iPhone Duo bezels are not in the repository (licence) and were not used: bezel measurement and the real-bezel render/export path were verified with a synthetic bezel PNG of known geometry, not Apple's files.
- Universal creative header/search crops are centred estimates, not Apple's published crop. The art safe areas come from Apple's templates.

- Chrome on a local desktop and emulated touch/viewport sizes are verified. Real iOS/Android hardware, WebKit/Firefox rendering and installed macOS system fonts on other operating systems are not covered.
- This is flow/regression coverage, not a complete accessibility audit, visual pixel-baseline suite, load test, or exhaustive security scan. Keyboard actions and responsive overflow are exercised; screen readers, every contrast combination and every tab sequence are not exhaustively judged.
- Export exposes a busy/progress state and error recovery. The product has no export-cancel control; no cancellation test is presented as covered. Reset dialog cancellation is a separate flow.
- Font coverage uses valid bundled WOFF2 and corrupt containers. It does not enumerate every licensed TTF/OTF/WOFF binary or font family.
- The suite never submits to App Store Connect or Google Play. It validates the editor's declared export contract; hosted store acceptance is not verified.
- Existing harnesses retain fixed short waits from the previous regression work. New direct Tester Army flows use readiness assertions/responses/download events. All selected tests must pass; there are no skip declarations disguising untested areas.

## Recorded execution

Validated locally on 2026-10-03 with Node 24.15.0, Bun 1.3.9 and Google Chrome 154.0.8037.97.

| Check | Result | Evidence |
| --- | --- | --- |
| Frozen install | Passed, no lock changes | `bun install --frozen-lockfile` |
| TypeScript and production build | Passed (Next 16.3.8) | `bun run typecheck`, `bun run build` |
| Final dev Tester Army suite after ESM fix | 26/26 passed, zero skips/retries, 263.30s | `.e2e/evidence/dev-esm/report.json` |
| Shipped groups within final dev suite | 35 browser/export +12 UI +9 API passed | `.e2e/evidence/dev-esm/logs/` |
| Default-selection regression repeat | 3/3 uncached repeats passed | `.e2e/evidence/selection-green/report.json` |
| Final production Tester Army suite after ESM fix | 26/26 passed, zero skips/retries, 191.81s | `.e2e/evidence/production-esm/report.json` |
| Shipped groups within final production suite | 35 browser/export +12 UI +9 API passed | `.e2e/evidence/production-esm/logs/` |
| Cold root dev API before ESM fix | Failed after 6 groups; Next crashed at config require | `.e2e/evidence/root-dev-red/{harness,server}.log` |
| Cold root dev API after ESM fix | 9/9 groups passed, including live migration | `.e2e/evidence/root-dev-green/{harness,server}.log` |

The initial native red run recorded the undo bug on all ten devices. A later direct keyboard regression exposed the separate default-selection bug; its failing screenshot/trace and report are preserved in `.e2e/evidence/selection-red/`. Final dev and production artifacts, including ZIP output and screenshots at 320, 390, 820, 1024 and 1440 pixels, were retained in `.e2e/evidence/{dev-esm,production-esm}/artifacts/`. Earlier passing implementation runs remain in `.e2e/evidence/{dev,production}/`.

Before the ESM fix, independent Luna verification passed both 26-test suites and a separate production root API run (9/9), inspected real Watch ZIPs and narrow-screen output, checked occupied-port refusal, workspace cleanup and canonical-file preservation. Its additional cold root dev invocation then exposed the config bug; the final implementation reruns above include the fix. The independent report is at `/Users/parthjadhav/research/e2e-rollout-2026-10-03/screenshots-luna.md`, with its untouched evidence under `/tmp/app-store-screenshots-rollout-verification-20261003/`.

The independent Luna follow-up passed the repaired cold root dev API sequence (9/9), a fresh dev suite (26/26 plus all 56 groups, zero failures/skips/flakes/retries/model use and all cleanups complete), typecheck and build. It verified the built animation plugin CSS, reviewed the retained final production 26/26 report, and validated a copied standalone scaffold with a webpack build plus all nine API groups using its bundled migration fixture. Production was not repeated during this follow-up; it was independently run before the config fix and fully rerun by the implementation afterward. No additional findings remained. The follow-up report is `/Users/parthjadhav/research/e2e-rollout-2026-10-03/screenshots-luna-followup.md`; cold-dev logs are under `/tmp/screenshots-luna-followup-20261003-root-dev/`. Both independent loops confirmed Chrome use, freed port 4312, workspace cleanup and unchanged canonical project/assets.

The canonical `app-store-screenshots.json` and source `public/` assets remained unchanged, and the original untracked `promo/` directory was preserved. No commits, pushes, PRs, store uploads or external feedback were created.

### iPhone Duo, App Store creatives and issue 41 (2026-10-07)

Validated locally with installed Google Chrome and Playwright WebKit.

| Check | Result |
| --- | --- |
| `bun run typecheck`, `bun run build` | Passed |
| Dev Tester Army suite | 54/55 passed; all 56 harness groups (35 browser/export, 12 UI, 9 API) passed |
| Production Tester Army suite | 54/55 passed; all 56 harness groups passed |
| The one failure in both | `keyboard screen reorder…`: the ArrowDown press doesn't move the item within the timeout when it runs after the rest of `editor.e2e.ts`. It also fails on unmodified `main` (`editor.e2e.ts` alone: 22/23) and passes when run by itself (2/2). It predates this change and is still open. |
| `tests/targets.e2e.ts` | 11/11 passed (12/12 after the Duo design pass below). The issue 41 test failed against the old `[contenteditable]:empty::before` rule (14,379 differing bytes) and passes with the fix. |
| Manual exports (Chrome, dev and production servers) | All four Duo sizes and 5244×2950: exact dimensions, color type 2, no alpha. Guides absent from PNGs. Mismatched capture letterboxed and named in the warning. |
| Repeated exports, en/de/ar | `creative-universal` (12 PNGs) and `duo-inner-landscape` (18 PNGs), three times in Chrome and twice in WebKit, all exact, opaque and with every locale folder; about 6 s and 5 s per export, with no slowdown across runs. |
| Synthetic bezel in `public/frames/` | `/api/frames` measured the cutout exactly (51, 63, 1398×2034, radius 150). Portrait and turned-landscape frames rendered and exported correctly. |

Apple's iPhone Duo bezel pack was downloaded but not opened. Opening it requires accepting Apple's licence, and that licence forbids redistribution. The real-bezel path was therefore verified only with a synthetic bezel. Apple's creative templates (universal, header, search) were downloaded from developer.apple.com and their "Art Safe Area" layers measured. No commits, pushes, uploads or external contact were made.

### iPhone Duo design pass (7 October 2026)

This pass added Duo-specific placements, the **Folded + open** pair and new starter decks (see `../iphone-duo.md`), plus one test: `duo folded + open pairs the other display and never reuses the front capture`.

| Check | Result |
| --- | --- |
| `typecheck`, `build` | Passed |
| Dev Tester Army suite | 56/56 passed, including the keyboard reorder test that failed in the earlier run; all 56 harness groups passed. The UI harness now selects **Folded + open** on Duo decks. |
| Manual exports | All four Duo decks, with every layout and with a designed example deck, exported from realistic synthetic captures at exact sizes. Folded + open drew each display in its own frame, and export reported no letterbox or placeholder warnings when both captures matched. |

### Apple bezels (7 October 2026)

The editor now draws iPhone, iPad, Apple Watch, Apple TV, Mac, App Store creatives and iPhone Duo under Apple's own bezels when they are in `public/frames/` (see the template README, Real bezels). The bezels are now bundled in `public/frames/`, so the test server copies them and every suite run covers the shipped frames. With them in place, Scene Playground steps first took longer than the 500 ms undo-grouping window. The cause: `DeviceDepth` changed its element tree when shadow, glow or tilt toggled, remounting every frame and re-decoding its bezel. The wrapper is now always the same shape, and the undo test passes with the bezels in place.

| Check | Result |
| --- | --- |
| Dev Tester Army suite | 56/56 passed; all 56 harness groups passed |
| Manual exports with all nine bezels | iPhone, iPad, Watch, TV, Mac, universal creative and all four Duo decks exported at every listed size, color type 2 and no alpha, with no warnings |

### Real iPhone Duo captures (7 October 2026)

Apple's Food Truck sample app was built with Xcode 27.1 RC and captured on the iPhone Duo simulator (iOS 27.1) with `../scripts/capture-iphone-duo.sh` in every pose: closed, closed and turned, open, Book, and open and turned. Every capture came out at the exact App Store size. The captures were then exported through the editor under Apple's bezels: all four Duo decks, opaque RGB, no warnings.

Two fixes came out of this:

- **Bezel cutouts are now measured per corner** (`radii` in `/api/frames`) and the capture is clipped to that shape. The Duo's outer display is square on its hinge side and rounded on the other, so a single radius let the capture poke past the bezel's body.
- **Turned poses are captured with the phone turned left.** That orientation matches Apple's landscape bezel, which has the camera top left.

### Duo capture checks and bezel measurement (9 October 2026)

Fixes from the review of PR #44.

- **Duo captures are matched within 0.5 %.** The two displays differ by 2.35 %, and the earlier 3 % tolerance let an outer capture pass as inner, so it was cropped without a warning. Other devices keep 3 %, which covers every Apple Watch size: they differ from the Ultra's screen by up to 2.6 %.
- **Inline (data URI) captures are decoded on load,** so their size is known after a reload.
- **Bezels are measured with a scanline flood fill over the alpha channel only.** All nine bundled bezels now measure in 250 ms instead of 2,065 ms (Node 24), with peak memory at 258 MB instead of 561 MB. The results are identical.

| Check | Result |
| --- | --- |
| New tests: an outer-display capture on the inner deck is flagged, served and inline | Both fail on the old code. The inline test still fails with only the tolerance fixed. Both pass with the fixes. |
| Dev Tester Army suite | 58/58 passed; all 56 harness groups passed |
