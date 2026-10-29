import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { LogViewerHandle } from '../logs/LogViewer'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render } = await import('@testing-library/react')
const { createPortal } = await import('react-dom')
const { createLogShortcutHandler } = await import('./logShortcuts')

afterEach(async () => {
  await act(async () => cleanup())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

function press(target: Element, init: KeyboardEventInit) {
  const event = new KeyboardEvent('keydown', { cancelable: true, bubbles: true, ...init })

  act(() => {
    target.dispatchEvent(event)
  })

  return event
}

function makeHandle(calls: string[]): LogViewerHandle {
  return {
    focusSearch: () => calls.push('search'),
    nextMatch: () => calls.push('next'),
    prevMatch: () => calls.push('prev'),
    nextError: () => calls.push('error'),
    prevError: () => calls.push('prevError'),
    scrollToStart: () => calls.push('start'),
    scrollToEnd: () => calls.push('end'),
    focusBody: () => calls.push('body'),
  }
}

test('shortcuts act inside the tabs and ignore events from portaled elements', () => {
  const calls: string[] = []
  const onKeyDown = createLogShortcutHandler(() => makeHandle(calls))

  const result = render(
    createElement(
      'div',
      { onKeyDown },
      createElement('input', { 'aria-label': 'inside' }),
      createPortal(createElement('button', { type: 'button', 'aria-label': 'portaled' }), document.body)
    )
  )

  const inside = press(result.getByRole('textbox', { name: 'inside' }), { key: 'f', ctrlKey: true })
  assert.deepEqual(calls, ['search'])
  assert.equal(inside.defaultPrevented, true)

  press(result.getByRole('textbox', { name: 'inside' }), { key: 'F8', shiftKey: true })
  assert.deepEqual(calls, ['search', 'prevError'])

  const portaled = press(result.getByRole('button', { name: 'portaled' }), { key: 'f', ctrlKey: true })
  assert.deepEqual(calls, ['search', 'prevError'])
  assert.equal(portaled.defaultPrevented, false)
})

test('each shortcut runs its viewer action and is prevented', () => {
  const calls: string[] = []
  const result = render(
    createElement(
      'div',
      { onKeyDown: createLogShortcutHandler(() => makeHandle(calls)) },
      createElement('input', { 'aria-label': 'inside' })
    )
  )
  const input = result.getByRole('textbox', { name: 'inside' })
  const cases: Array<[KeyboardEventInit, string]> = [
    [{ key: 'g', ctrlKey: true }, 'next'],
    [{ key: 'g', ctrlKey: true, shiftKey: true }, 'prev'],
    [{ key: 'F8' }, 'error'],
    [{ key: 'Home', ctrlKey: true }, 'start'],
    [{ key: 'End', ctrlKey: true }, 'end'],
  ]

  for (const [init, expected] of cases) {
    calls.length = 0
    assert.equal(press(input, init).defaultPrevented, true)
    assert.deepEqual(calls, [expected])
  }
})

test('shortcuts are inert while no viewer is mounted', () => {
  const result = render(
    createElement(
      'div',
      { onKeyDown: createLogShortcutHandler(() => null) },
      createElement('input', { 'aria-label': 'x' })
    )
  )

  assert.equal(press(result.getByRole('textbox', { name: 'x' }), { key: 'f', ctrlKey: true }).defaultPrevented, false)
})
