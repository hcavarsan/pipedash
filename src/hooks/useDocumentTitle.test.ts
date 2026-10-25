import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(() => GlobalRegistrator.unregister())

const { cleanup, renderHook } = await import('@testing-library/react')
const { useDocumentTitle } = await import('./useDocumentTitle')

beforeEach(() => {
  cleanup()
  document.title = 'Pipedash'
})

test('updates the desktop document title when the page title changes', () => {
  const { rerender } = renderHook((title: string) => useDocumentTitle(title), { initialProps: 'Pipelines' })
  assert.equal(document.title, 'Pipelines · Pipedash')
  rerender('General Settings')
  assert.equal(document.title, 'General Settings · Pipedash')
})

test('leaves the phone shell document title unchanged', () => {
  const original = Object.getOwnPropertyDescriptor(navigator, 'userAgent')
  Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'iPhone' })
  try {
    renderHook(() => useDocumentTitle('Pipelines'))
    assert.equal(document.title, 'Pipedash')
  } finally {
    if (original) {
      Object.defineProperty(navigator, 'userAgent', original)
    } else {
      Reflect.deleteProperty(navigator, 'userAgent')
    }
  }
})
