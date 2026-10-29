import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 393, height: 852 })

const { act, cleanup, render, waitFor } = await import('@testing-library/react')
const { useKeyboardInset, useKeyboardOpen } = await import('./useKeyboardInset')

const viewport = Object.assign(new EventTarget(), { height: 852, offsetTop: 0 })
Object.defineProperty(window, 'visualViewport', { configurable: true, value: viewport })

function Form({ active = true }: { active?: boolean }) {
  useKeyboardInset(active)
  return createElement('main', { style: { overflowY: 'auto' } }, createElement('input', { 'aria-label': 'Setting' }))
}

function OpenProbe() {
  return createElement('output', null, useKeyboardOpen() ? 'keyboard open' : 'keyboard closed')
}

const rootInset = () => document.documentElement.style.getPropertyValue('--keyboard-inset')

async function setViewportHeight(height: number) {
  await act(async () => {
    viewport.height = height
    viewport.dispatchEvent(new Event('resize'))
  })
}

afterEach(async () => {
  await act(async () => cleanup())
  viewport.height = 852
})

after(async () => {
  await act(async () => cleanup())
  await GlobalRegistrator.unregister()
})

test('keyboard opening reveals focused inline fields within the visual viewport', async () => {
  const result = render(createElement(Form))
  const field = result.getByRole('textbox')
  assert.ok(field instanceof HTMLInputElement)
  const main = field.parentElement
  assert.ok(main)
  Object.defineProperties(main, {
    scrollHeight: { configurable: true, value: 1200 },
    clientHeight: { configurable: true, value: 852 },
  })
  main.getBoundingClientRect = () => new DOMRect(0, 0, 393, 852)
  field.getBoundingClientRect = () => new DOMRect(10, 700 - main.scrollTop, 373, 44)
  main.scrollBy = (options?: ScrollToOptions | number) => {
    assert.equal(typeof options, 'object')
    if (typeof options === 'object') {
      main.scrollTop += options.top ?? 0
    }
  }
  field.focus()
  await act(async () => {
    viewport.height = 520
    viewport.dispatchEvent(new Event('resize'))
  })
  await waitFor(() => {
    assert.equal(document.documentElement.style.getPropertyValue('--keyboard-inset'), '332px')
    assert.equal(field.getBoundingClientRect().bottom, 508)
  })
})

test('closing the keyboard resets the inset and the keyboard flag', async () => {
  render(createElement(Form))
  await setViewportHeight(520)
  await waitFor(() => assert.equal(rootInset(), '332px'))
  assert.equal(document.documentElement.hasAttribute('data-keyboard'), true)

  await setViewportHeight(852)
  await waitFor(() => assert.equal(rootInset(), '0px'))
  assert.equal(document.documentElement.hasAttribute('data-keyboard'), false)
})

test('an inactive hook leaves the viewport alone', async () => {
  render(createElement(Form, { active: false }))
  await setViewportHeight(520)

  assert.equal(rootInset(), '')
})

test('unmounting clears the inset and drops a pending reveal', async () => {
  const result = render(createElement(Form))
  const field = result.getByRole('textbox')
  const main = field.parentElement
  assert.ok(main)
  let scrolls = 0
  Object.defineProperties(main, {
    scrollHeight: { configurable: true, value: 1200 },
    clientHeight: { configurable: true, value: 852 },
  })
  main.getBoundingClientRect = () => new DOMRect(0, 0, 393, 852)
  field.getBoundingClientRect = () => new DOMRect(10, 700, 373, 44)
  main.scrollBy = () => {
    scrolls += 1
  }
  field.focus()

  await act(async () => {
    viewport.height = 520
    viewport.dispatchEvent(new Event('resize'))
    result.unmount()
  })
  await act(async () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve())))

  assert.equal(scrolls, 0)
  assert.equal(rootInset(), '')
  assert.equal(document.documentElement.hasAttribute('data-keyboard'), false)
})

test('keyboard subscribers see the keyboard open and close', async () => {
  render(createElement(Form))
  const probe = render(createElement(OpenProbe))
  assert.ok(probe.getByText('keyboard closed'))

  await setViewportHeight(520)
  await waitFor(() => assert.ok(probe.getByText('keyboard open')))

  await setViewportHeight(852)
  await waitFor(() => assert.ok(probe.getByText('keyboard closed')))
})
