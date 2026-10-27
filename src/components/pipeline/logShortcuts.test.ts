import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { LogViewerHandle } from '../logs/LogViewer'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render } = await import('@testing-library/react')
const { createPortal } = await import('react-dom')
const { createLogShortcutHandler } = await import('./logShortcuts')

after(async () => {
  await act(async () => cleanup())
  GlobalRegistrator.unregister()
})

function press(target: Element, init: KeyboardEventInit) {
  const event = new KeyboardEvent('keydown', { cancelable: true, bubbles: true, ...init })

  act(() => {
    target.dispatchEvent(event)
  })

  return event
}

test('shortcuts act inside the tabs and ignore events from portaled elements', () => {
  const calls: string[] = []
  const handle = {
    focusSearch: () => calls.push('search'),
    nextMatch: () => calls.push('next'),
    prevMatch: () => calls.push('prev'),
    nextError: () => calls.push('error'),
    prevError: () => calls.push('prevError'),
    scrollToStart: () => calls.push('start'),
    scrollToEnd: () => calls.push('end'),
    focusBody: () => calls.push('body'),
  } satisfies LogViewerHandle
  const onKeyDown = createLogShortcutHandler(() => handle)

  const result = render(
    createElement(
      'div',
      { onKeyDown },
      createElement('input', { 'aria-label': 'inside' }),
      createPortal(createElement('button', { type: 'button', 'aria-label': 'portaled' }), document.body)
    )
  )

  const inside = press(result.getByLabelText('inside'), { key: 'f', ctrlKey: true })
  assert.deepEqual(calls, ['search'])
  assert.equal(inside.defaultPrevented, true)

  press(result.getByLabelText('inside'), { key: 'F8', shiftKey: true })
  assert.deepEqual(calls, ['search', 'prevError'])

  const portaled = press(result.getByLabelText('portaled'), { key: 'f', ctrlKey: true })
  assert.deepEqual(calls, ['search', 'prevError'])
  assert.equal(portaled.defaultPrevented, false)
})

test('shortcuts are inert while no viewer is mounted', () => {
  const result = render(
    createElement(
      'div',
      { onKeyDown: createLogShortcutHandler(() => null) },
      createElement('input', { 'aria-label': 'x' })
    )
  )

  assert.equal(press(result.getByLabelText('x'), { key: 'f', ctrlKey: true }).defaultPrevented, false)
})
