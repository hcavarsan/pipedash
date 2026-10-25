import assert from 'node:assert/strict'
import { test } from 'node:test'

import { DEFAULT_THEME, getContrastColor, luminance, mergeMantineTheme } from '@mantine/core'

import { baseTheme, SPACING_PX, theme } from './index'

const desktopTheme = mergeMantineTheme(DEFAULT_THEME, theme)

test('filled primary buttons retain white text with AA contrast in both schemes', () => {
  for (const colorScheme of ['light', 'dark'] as const) {
    const shade =
      typeof desktopTheme.primaryShade === 'number' ? desktopTheme.primaryShade : desktopTheme.primaryShade[colorScheme]
    const background = desktopTheme.colors.blue[shade]

    assert.equal(getContrastColor({ color: 'blue', theme: desktopTheme, colorScheme }), 'var(--mantine-color-white)')
    assert.ok(1.05 / (luminance(background) + 0.05) >= 4.5)
  }
})

test('shared compact spacing extends the scale without changing existing phone spacing', () => {
  const phoneBase = mergeMantineTheme(DEFAULT_THEME, baseTheme)

  for (const name of ['xs', 'sm', 'md', 'lg', 'xl'] as const) {
    assert.equal(phoneBase.spacing[name], DEFAULT_THEME.spacing[name])
  }
  assert.deepEqual(SPACING_PX, { '4xs': 2, '3xs': 4, '2xs': 6, xs: 10, sm: 12, md: 16, lg: 20, xl: 32 })
  assert.equal(phoneBase.autoContrast, DEFAULT_THEME.autoContrast)
})
