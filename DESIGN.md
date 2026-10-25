# Pipedash design

Pipedash is a CI/CD dashboard. People open it to find the pipeline that failed and act on it: open the run, rerun it, or cancel it. The interface is built around that job. Status is the loudest signal on screen, and everything else stays quiet.

## Theme

- Dark is the default. Developers often keep it open beside an editor and a terminal, which are usually dark.
- Light and System are available in Settings → General. Mantine stores the choice.
- Use only colors that follow the scheme: `var(--mantine-color-body)`, `--mantine-color-default`, `--mantine-color-default-border`, `--mantine-color-default-hover`, `--mantine-color-text`, `--mantine-color-dimmed`, and `--mantine-color-<color>-light`. In code, use `c="dimmed"` or leave the default text color. Never hardcode `--mantine-color-dark-*`, hex, or `rgba`.

## Color roles

| Role | Color | Used for |
|---|---|---|
| Accent | `blue` (theme primary) | Primary buttons, selection, focus, and the running status |
| Success | `green` | Successful runs and successful outcomes (copied, connected, granted) |
| Danger | `red` | Failed runs and destructive actions |
| Warning | `orange` | Actions that change data and need care: migration, raw config edits, connection loss |
| Pending | `yellow` | Pending runs only |
| Neutral | `gray` | Everything else, including row actions and icon buttons |

Run status uses its own tokens, `--status-success`, `--status-failed`, `--status-running`, `--status-pending` and `--status-neutral`, defined once per scheme in `src/App.css`. They are OKLCH colors with the same lightness and chroma, so no status looks heavier than another: deep in light mode, soft pastel in dark mode, all above 5:1 contrast. These are the only fixed colors in the app. `STATUS_META` in `src/theme/status.ts` maps each status to its token, label and icon. Render a status with `StatusBadge` (text) in tables, cards and modals, or `StatusIcon` (filled icon with a tooltip) on pinned cards. Never color a status by hand.

## Tables

- A header is aligned exactly like its cells.
- Text, statuses and dates align left. Numbers and durations align right and use tabular figures.
- The actions column aligns right, header included.
- Run history columns get their alignment from the column data type in `src/utils/columnBuilder.ts`, so every provider looks the same.
- Counts are plain dimmed text, not badges.

## Shape and depth

- Radius: `sm` for controls, inputs, and badges (the theme default). `md` for cards, papers, and modals, set through theme defaults.
- Borders separate surfaces. Shadows appear only on floating layers: menus, popovers, and tooltips.
- No hover lift. A clickable surface gets the `interactive-surface` class, which tints its background on hover.

## Type and icons

- Mantine's system font stack and size tokens. No raw font sizes.
- Monospace only for real identifiers: commit SHAs, run IDs, paths, commands, and JSON.
- Icons come from Tabler. They are chosen for meaning, never as decoration. Sizes: 14 in dense badges, 16 by default, 20 in the header toolbar, 24 in empty states.

## States

- Loading: `LoadingState` with a message that names what is loading.
- Empty: `EmptyState` with the reason it is empty and the next action.
- Error: `ErrorState` with the operation that failed, the error, and a retry.
- Missing data is shown as `—` or "Unavailable". Never show it as `0`.

## Motion

- Motion is reserved for feedback during real operations, such as loaders inside buttons.
- No decorative loops.
- The app honors `prefers-reduced-motion`.

## Accessibility

- Every icon button has an `aria-label` that matches its tooltip.
- Clickable things are buttons or links. Interactive elements are never nested.
- Every input has a label. Use a visible label, or an `aria-label` when space is tight.

## iPhone

- The phone shell sets `data-ios` on the root. Desktop and iPad styles never read these tokens.
- Text uses the `--ios-fs-*` and `--ios-lh-*` tokens: large title, title1 to title3, headline, body, callout, subheadline, footnote, caption1 and caption2. The Mantine sizes map to them: `xs` footnote, `sm` subheadline, `md` body, `lg` title3, `xl` title2. Headline is 600, the navigation large title is 700, everything else is 400. Letter spacing stays `normal` because the system font tracks itself.
- Dynamic Type: when WebKit supports `font: -apple-system-body`, a hidden probe measures the body size and sets `--ios-text-scale` on the root. It updates live. Paddings next to text use `em` and containers use minimum heights, so rows grow with the text. Zoom stays enabled.
- `TYPOGRAPHY` roles (`headline`, `body`, `callout`, `subheadline`, `caption`) resolve to the same small Mantine sizes on desktop and to the iOS styles on the phone shell.
- Colors follow the iOS roles: `--ios-label`, `--ios-secondary-label`, `--ios-bg`, `--ios-grouped-bg`, `--ios-cell`, `--ios-tint`, `--ios-red` and `--ios-green`, each defined for light and dark. Root pages sit on the grouped background so white cells show in light mode. Inside sheets and popovers in dark mode, cells are `#2c2c2e` and the background is `#1c1c1e`. Run status colors keep their own scale.
- Bars, sheets and alerts use the `--ios-material-*` tokens with `--ios-material-filter`. Never stack a translucent surface on another.
- Reduce Transparency (system setting or the native bridge) sets `data-reduce-transparency`: materials become opaque and the blur is removed. Increase Contrast sets `data-increase-contrast`: the tint, red, green and secondary label switch to the accessible values, separators turn opaque, and materials get a visible edge through `--ios-material-edge`.
- Controls keep a 44pt touch area even when the visual is smaller. Keyboard focus shows a 2px tint ring. Touch shows none.
- Motion on gestures uses springs from `src/lib/spring.ts`: damping 1 by default, a little bounce only after a flick. Reduce Motion replaces movement with a short fade.
- Haptics fire in the same frame as the visual change and only for meaningful moments: a pull to refresh arming, a full swipe arming, a long-press menu opening, and success, warning and error outcomes.
- Chrome and controls are not selectable. Content text such as errors, IDs, URLs and SHAs is.
