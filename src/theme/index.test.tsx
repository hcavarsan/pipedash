import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { ActionIcon, DEFAULT_THEME, getContrastColor, luminance, MantineProvider, mergeMantineTheme } = await import(
  '@mantine/core'
)
const { cleanup, render, within } = await import('@testing-library/react')
const { theme } = await import('./index')

afterEach(() => {
  cleanup()
})

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

test('icon-only buttons render the transparent subtle gray treatment by default', () => {
  render(
    <MantineProvider theme={theme}>
      <ActionIcon aria-label="Refresh">
        <span aria-hidden>↻</span>
      </ActionIcon>
    </MantineProvider>
  )

  const button = within(document.body).getByRole('button', { name: 'Refresh' })

  assert.equal(button.getAttribute('data-variant'), 'subtle')
  assert.match(button.style.getPropertyValue('--ai-color'), /gray/)
  assert.equal(button.style.getPropertyValue('--ai-bg'), 'transparent')
})
