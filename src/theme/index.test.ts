import assert from 'node:assert/strict'
import { test } from 'node:test'

import { DEFAULT_THEME, getContrastColor, luminance, mergeMantineTheme } from '@mantine/core'

import { theme } from './index'

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

test('icon-only buttons default to the transparent subtle gray treatment', () => {
  const { ActionIcon } = desktopTheme.components

  assert.ok(ActionIcon)
  assert.deepEqual(ActionIcon.defaultProps, { variant: 'subtle', color: 'gray' })
})
