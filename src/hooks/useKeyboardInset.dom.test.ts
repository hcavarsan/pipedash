import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 393, height: 852 })

const { act, cleanup, render, waitFor } = await import('@testing-library/react')
const { useKeyboardInset } = await import('./useKeyboardInset')

const viewport = Object.assign(new EventTarget(), { height: 852, offsetTop: 0 })
Object.defineProperty(window, 'visualViewport', { configurable: true, value: viewport })

function Form() {
  useKeyboardInset(true)
  return createElement('main', { style: { overflowY: 'auto' } }, createElement('input', { 'aria-label': 'Setting' }))
}

after(async () => {
  await act(async () => cleanup())
  GlobalRegistrator.unregister()
})

test('keyboard opening reveals focused inline fields within the visual viewport', async () => {
  const result = render(createElement(Form))
  const field = result.getByRole('textbox') as HTMLInputElement
  const main = field.parentElement as HTMLElement
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
