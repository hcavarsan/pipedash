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

Run status colors come from `src/theme/status.ts` (`STATUS_META`). Always render a status with `StatusBadge`. Never color a status by hand.

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
