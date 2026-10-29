import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import type { LineState } from './lineCache'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { createElement } = await import('react')
const { LogRow } = await import('./LogRow')

after(async () => {
  await GlobalRegistrator.unregister()
})

afterEach(async () => {
  await act(async () => cleanup())
})

const noop = () => undefined
const renderLine = (data: LineState) =>
  render(
    createElement(
      MantineProvider,
      null,
      createElement(LogRow, {
        kind: 'line',
        line: 4,
        data,
        section: null,
        open: false,
        styleClasses: [],
        gutterCh: 3,
        timestamps: false,
        selected: false,
        matches: undefined,
        current: null,
        onToggle: noop,
        onSelect: noop,
        onFullLine: noop,
      })
    )
  )

test('a line that is still loading says so', () => {
  renderLine('loading')

  assert.ok(within(document.body).getByText(/loading/i))
})

test('a line that failed to load says so instead of showing a blank row', () => {
  renderLine('error')

  assert.ok(within(document.body).getByText(/couldn't load this line/i))
})

test('a loaded line shows its text and neither placeholder', () => {
  renderLine({ text: 'hello', kind: 'plain', ts: null, spans: [], omitted: 0 })

  assert.ok(within(document.body).getByText('hello'))
  assert.equal(within(document.body).queryByText(/loading/i), null)
})
