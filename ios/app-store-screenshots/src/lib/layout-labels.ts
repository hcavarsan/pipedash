import type { Device, SlideLayout } from './types'

// Friendly labels for slide layouts (used in dropdowns)
export const LAYOUT_LABEL: Record<SlideLayout, string> = {
  hero: 'Hero',
  'device-bottom': 'Device bottom',
  'device-top': 'Device top',
  'two-devices': 'Two devices',
  'no-device': 'No device',
  'split-landscape': 'Split (landscape)',
  'feature-graphic': 'Feature graphic',
}

// Short description shown under each layout name
export const LAYOUT_HINT: Record<SlideLayout, string> = {
  hero: 'Headline above, device at bottom',
  'device-bottom': 'Headline top, device anchored below',
  'device-top': 'Flipped — device on top',
  'two-devices': 'Layered back + front phones',
  'no-device': 'Big standalone headline',
  'split-landscape': 'Caption left, device right',
  'feature-graphic': '1024×500 Play Store banner',
}

// On iPhone Duo the second device is the same phone in its other state.
const DUO_PAIR_LABEL = 'Folded + open'
const DUO_PAIR_HINT = 'The same phone closed and opened, side by side'

/** Layout name for a device's deck. */
export function layoutLabel(layout: SlideLayout, device: Device): string {
  return layout === 'two-devices' && device.startsWith('duo-') ? DUO_PAIR_LABEL : LAYOUT_LABEL[layout]
}

/** Layout description for a device's deck. */
export function layoutHint(layout: SlideLayout, device: Device): string {
  return layout === 'two-devices' && device.startsWith('duo-') ? DUO_PAIR_HINT : LAYOUT_HINT[layout]
}
