# Pipedash design

Pipedash is a CI/CD dashboard. People open it to find the pipeline that failed and act on it: open the run, rerun it, or cancel it. The interface is built around that job. Status is the loudest signal on screen, and everything else stays quiet.

## Theme

- Dark is the default. Developers often keep it open beside an editor and a terminal, which are usually dark.
- Light and System are available in Settings → General. Mantine stores the choice.
- Use only colors that follow the scheme: `var(--mantine-color-body)`, `--mantine-color-default`, `--mantine-color-default-border`, `--mantine-color-default-hover`, `--mantine-color-text`, `--mantine-color-dimmed`, and `--mantine-color-<color>-light`. In code, use `c="dimmed"` or leave the default text color. Never hardcode `--mantine-color-dark-*`, hex, or `rgba`.
- The brand palettes (blue, green, red, yellow, gray) are defined once in `src/theme/index.ts`. The desktop and web theme turns on `autoContrast` and uses shade 7 as the primary in both schemes, so text on filled buttons stays above 4.5:1. The macOS window buttons in `WindowControls.module.css` keep the system traffic-light colors as custom properties; that is the only other fixed color outside the status tokens.
- `src/theme/index.ts` exports `baseTheme`, shared with the phone theme, and `theme`, which adds the desktop and web defaults on top. Desktop-only defaults never reach the phone shell.
- Runtime Mantine styles reuse Tauri's nonce from `#app-color-scheme`. Keep `getStyleNonce` on the provider so layout and theme rules pass the packaged content security policy.

## Color roles

| Role | Color | Used for |
|---|---|---|
| Accent | `blue` (theme primary) | Primary buttons, selection, focus, and the running status |
| Success | `green` | Successful runs and successful outcomes (copied, connected, granted) |
| Danger | `red` | Failed runs and destructive actions |
| Warning | `orange` | Actions that change data and need care: migration, raw config edits, connection loss |
| Pending | `yellow` | Pending runs only |
| Neutral | `gray` | Everything else, including row actions and icon buttons |

Run status uses its own tokens, `--status-success`, `--status-failed`, `--status-running`, `--status-pending` and `--status-neutral`, defined once per scheme in `src/App.css`. They are OKLCH colors with the same lightness and chroma, so no status looks heavier than another: deep in light mode, soft pastel in dark mode, all above 5:1 contrast. `STATUS_META` in `src/theme/status.ts` maps each status to its token, label and icon. Render a status with `StatusBadge` (text) in tables, cards and modals, or `StatusIcon` (filled icon with a tooltip) on pinned cards. Never color a status by hand. Provider health and notification severity carry a glyph as well as a color, so they never rely on color alone.

## Tables

- A header is aligned exactly like its cells.
- Text, statuses and dates align left. Numbers and durations align right and use tabular figures.
- The actions column aligns right, header included.
- Run history columns get their alignment from the column data type in `src/utils/columnBuilder.ts`, so every provider looks the same.
- Counts are plain dimmed text, not badges.
- Tables render through `StandardTable`, including the metrics data table and the pipeline picker in Add provider. Its wrapper owns the host sizing.
- Rows that open something are reachable by keyboard. Run history rows take focus and open the run with Enter or Space; their action buttons stay separate tab stops.
- Right-click, Shift+F10 or the context menu key opens the same actions as the row buttons on pipeline rows, pinned cards and run history rows. `useContextMenu` in `src/components/common/ContextMenu.tsx` builds the menu from the shared action list, so the menu and the buttons never drift apart.

## Spacing

- Spacing uses the theme scale: `4xs` 2px, `3xs` 4px, `2xs` 6px, `xs` 10px, `sm` 12px, `md` 16px, `lg` 20px, `xl` 32px. No raw numbers in `gap`, `p`, `m` or `mt` props.
- APIs that need numbers, such as Monaco padding and chart margins, read `SPACING_PX` from `src/theme`.

## Shape and depth

- Radius: `sm` for controls, inputs, and badges (the theme default). `md` for cards, papers, and modals, set through theme defaults.
- Borders separate surfaces. Shadows appear only on floating layers: menus, popovers, comboboxes and tooltips, set as theme defaults.
- No hover lift. A clickable surface gets the `interactive-surface` class, which tints its background on hover and darkens it on press with no delay. A blocked control keeps its tooltip but sets `aria-disabled` and looks disabled.
- The desktop and web header is the one translucent surface: content scrolls under it, and its bottom border appears only once the page is scrolled. Reduce Transparency and Increase Contrast make it opaque.

## Type and icons

- Mantine's system font stack and size tokens. No raw font sizes.
- Desktop and web headings tighten as they grow: about -0.02em at h1 down to normal at h5. Body text keeps a 1.5 line height.
- Monospace only for real identifiers: commit SHAs, run IDs, paths, commands, and JSON.
- Icons come from Tabler. They are chosen for meaning, never as decoration. Sizes: 14 in dense badges, 16 by default, 20 in the header toolbar, 24 in empty states.

## States and feedback

- Loading: `LoadingState` with a message that names what is loading.
- Empty: `EmptyState` with the reason it is empty and the next action, as a button when one exists.
- Error: `ErrorState` with the operation that failed, the error, and a retry.
- Missing data is shown as `—` or "Unavailable". Never show it as `0`. A measured zero duration is `0s`.
- Failure titles read `Couldn't <verb> <object>`; the message gives the cause or the next step. `errorMessage(error, fallback)` in `src/utils/errorDisplay.tsx` picks the most specific message. Notification helpers always take an explicit title.
- A mutation hook owns its notifications. Callers do not toast the same outcome again.
- Ask for confirmation only before something that cannot be undone: removing a provider, deleting metrics, a factory reset, cancelling runs, discarding unsaved edits. Clearing caches and other refetchable work runs at once with progress and a completion message.
- Cancelling a run says "Cancellation requested" until the provider reports the run as cancelled. Run history, the logs dialog and bulk cancel share `useCancelRun`.

## Forms

- Fields validate on blur, then again on every change once they show an error. Requirements such as a minimum password length are visible as help text before anyone types.
- Numeric settings keep a draft: a blank field stays blank and shows an error instead of being replaced with a default. They save on blur or Enter and restore the saved value if the save fails.
- On desktop and web, Enter submits the form. Moving to the next field with Enter happens only on the phone shell.
- Dialogs with unsaved edits pass `dirty` to `StandardModal`, and footer buttons receive the guarded close through `footer={(close) => …}`, so Cancel, Escape, the close button and the overlay all ask before discarding.

## Motion

- Motion is reserved for feedback during real operations, such as loaders inside buttons.
- No decorative loops.
- Dialogs scale and fade in over about 160ms with `PRESENT_EASING` from `src/lib/motion.ts` and never overshoot. Menus, popovers and comboboxes fade in 120ms, tooltips 100ms. Desktop notifications fade without moving and cannot be dragged away.
- Dragging the sidebar edge tracks the pointer 1:1. On release it snaps to the rail or to its width with a critically damped spring that keeps the release velocity and can be grabbed again mid-flight.
- The app honors `prefers-reduced-motion`: spatial movement is removed and only fades remain.

## Accessibility

- Every icon button has an `aria-label` that matches its tooltip. Tooltips also open on keyboard focus. A copy button's tooltip and label both change to "Copied".
- Clickable things are buttons or links. Interactive elements are never nested.
- Every input has a label. Use a visible label, or an `aria-label` when space is tight.
- On desktop and web the focus ring shows only after keyboard navigation (`data-keyboard-focus`): 2px in the primary color with a 2px offset. Headings and the main region that receive focus for screen readers never show a ring.
- A "Skip to content" button comes first in the tab order. After a route change, focus moves to the page heading, and returning to the pipeline list restores its scroll position.
- Each route sets the document title, such as "Build and test · Pipedash" or "Providers Settings · Pipedash".
- Reduce Transparency and Increase Contrast apply on every surface. On desktop and web, Increase Contrast strengthens borders and dimmed text.

## Desktop and web

- Desktop and web keep desktop idioms: sidebar, toolbar, data tables, context menus, hover states and keyboard shortcuts. They never borrow phone navigation such as large titles, a bottom tab bar or pull to refresh.
- Shortcuts, shown in tooltips through `shortcutLabel` in `src/lib/shortcuts.ts`: `/` or Mod+K focuses the page search, Mod+, opens Settings or returns from it, Mod+\ collapses or expands the sidebar. The desktop app also binds Mod+R to refresh everything and Mod+L to lock. The web app leaves browser shortcuts alone.
- On macOS the app menu adds Settings… and Lock, and a Pipelines menu holds Refresh All, Toggle Sidebar and Find. Menu items and shortcuts run the same commands through the `menu-command` event.
- Blank space in the desktop title bar drags the window, and double-clicking it maximizes. Window buttons show the real maximized state.
- While Settings is open, the toolbar's Settings button becomes "Back to Pipelines".
- Settings uses the same `PageHeader` as other pages, with section actions in its action slot. The settings sidebar is a labeled navigation landmark and marks the current section with `aria-current`.
- A missing pipeline or an unknown address shows a not-found state with a way back instead of a silent redirect.

## iPhone

- The phone shell sets `data-ios` on the root. Desktop and iPad styles never read these tokens.
- Safe areas use the larger of WebKit's CSS inset and the native `WKWebView.safeAreaInsets` value. The native bridge updates on layout changes and navigation. The phone header reserves equal side insets to keep its title centered in landscape.
- Text uses the `--ios-fs-*` and `--ios-lh-*` tokens: large title, title1 to title3, headline, body, callout, subheadline, footnote, caption1 and caption2. The Mantine sizes map to them: `xs` footnote, `sm` subheadline, `md` body, `lg` title3, `xl` title2. Headline is 600, the navigation large title is 700, everything else is 400. Letter spacing stays `normal` because the system font tracks itself.
- Dynamic Type: when WebKit supports `font: -apple-system-body`, a hidden probe measures the body size and sets `--ios-text-scale` on the root. It updates live. Paddings next to text use `em` and containers use minimum heights, so rows grow with the text. Zoom stays enabled.
- `TYPOGRAPHY` roles (`headline`, `body`, `callout`, `subheadline`, `caption`) resolve to the same small Mantine sizes on desktop and to the iOS styles on the phone shell.
- Colors follow the iOS roles: `--ios-label`, `--ios-secondary-label`, `--ios-bg`, `--ios-grouped-bg`, `--ios-cell`, `--ios-tint`, `--ios-red` and `--ios-green`, each defined for light and dark. Root pages sit on the grouped background so white cells show in light mode. Inside sheets and popovers in dark mode, cells are `#2c2c2e` and the background is `#1c1c1e`. Run status colors keep their own scale.
- Bars, sheets and alerts use the `--ios-material-*` tokens with `--ios-material-filter`. Never stack a translucent surface on another.
- Reduce Transparency (system setting or the native bridge) sets `data-reduce-transparency`: materials become opaque and the blur is removed. Increase Contrast sets `data-increase-contrast`: the tint, red, green and secondary label switch to the accessible values, separators turn opaque, and materials get a visible edge through `--ios-material-edge`.
- Native accessibility preferences apply on wide iPads too and update when the system setting changes.
- Controls keep a 44pt touch area even when the visual is smaller. Keyboard focus shows a 2px tint ring. Touch shows none, and hover styles never apply on touch.
- Segmented controls look like the iOS ones: no border, a raised thumb and iOS colors. Theme and retention pickers use native selects without an idle border. Keyboard navigation keeps its focus ring.
- Every sheet uses one background for its header and body, and content starts 16pt below the header. Multi-step sheets show a compact progress row instead of a desktop stepper.
- Sheets resize with the keyboard before revealing the focused field. Inline forms also scroll focused fields into the visible viewport. Picker sheets use one scroll surface and stay above the keyboard and home indicator.
- The pressed highlight covers the whole row, including the chevron and the trailing … button.
- The search row on list pages, including run history, hides by default. A short pull reveals it, a longer pull refreshes. During refresh, the indicator stays fixed below the header and connection banner. Its spokes fade without rotating the container.
- With filters active, the filter button switches to the filled icon in the tint color, and its label carries the count.
- Run history edit mode uses 22pt selection circles and a full-width bottom toolbar with the bar material and the safe-area inset. The tab bar is hidden and inert while editing.
- Exports open the system document picker so you choose the destination. Chart PNG and CSV exports stay busy until the picker finishes. Cancelling shows no success message. Native exports attempt to delete their temporary folder after success, cancellation or picker errors. Cleanup failures are logged. Sharing a link opens the share sheet from a full-width 44pt row.
- The launch screen and the app switcher cover use the grouped background with no image, so opening the app never flashes a different color or a small icon.
- Motion on gestures uses springs from `src/lib/spring.ts`: damping 1 by default, a little bounce only after a flick. Reduce Motion replaces movement with a short fade.
- Haptics fire in the same frame as the visual change and only for meaningful moments: a pull to refresh arming, a full swipe arming, a long-press menu opening, and success, warning and error outcomes.
- Chrome and controls are not selectable. Content text such as errors, IDs, URLs and SHAs is.

### Build watching

- `PipedashBuildWatchEnabled` is `false` in the iOS release and also gates remote push. Both features stay hidden and their native entry points reject starts or registration. Remote push skips token synchronization and preserves saved pairings. Local notifications remain independent. The bundle omits their entitlements and background declarations, and release validation rejects reactivation. See [release restrictions](https://pipedash.app/docs/resources/ios-release#disabled-build-watching).
- Watch follows one exact pipeline and run number. A Watching section shows its pipeline above Pinned on the main pipelines page only. It uses the same row and card style as pinned pipelines, with actions to open the run or stop watching.
- Core owns watch scheduling and applies provider quota, rate-limit and concurrency controls. Desktop forwards observations to the native background task and cancels on lifecycle changes.
- A refresh waiting on a run-detail request retries through core policy after that request releases its claim. Run details never count as refreshed pipelines.
- The continued-processing task provides the only system Live Activity. Pipedash does not create a second ActivityKit activity or embed a custom widget extension.
- iOS controls the task's presentation and expansion. The app cannot force compact-only display. Title updates occur only when visible labels change.
- The task's single progress unit completes only when the provider reports a terminal result. Polling never invents build progress.
- iOS may interrupt monitoring. Locking Pipedash, removing the watched provider or resetting the app also interrupts it. Returning after a force quit never restarts a watch automatically.
- Monitoring state stays separate from the provider's build result. Terminal runs use the provider's conclusion time when available. Watch content contains no provider credentials.
