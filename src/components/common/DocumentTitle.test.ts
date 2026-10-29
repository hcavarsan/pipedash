import assert from 'node:assert/strict'
import { after, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register({ width: 1400, height: 900 })
after(async () => {
  await GlobalRegistrator.unregister()
})

const { cleanup, render } = await import('@testing-library/react')
const { createElement } = await import('react')
const { DocumentTitle } = await import('./DocumentTitle')

beforeEach(() => {
  cleanup()
  document.title = 'Pipedash'
})

test('sets the desktop document title', () => {
  render(createElement(DocumentTitle, { title: 'Pipelines' }))
  assert.equal(document.title, 'Pipelines · Pipedash')
})

test('leaves the phone shell document title unchanged', () => {
  const original = Object.getOwnPropertyDescriptor(navigator, 'userAgent')
  Object.defineProperty(navigator, 'userAgent', { configurable: true, value: 'iPhone' })
  try {
    render(createElement(DocumentTitle, { title: 'Pipelines' }))
    assert.equal(document.title, 'Pipedash')
  } finally {
    if (original) {
      Object.defineProperty(navigator, 'userAgent', original)
    } else {
      Reflect.deleteProperty(navigator, 'userAgent')
    }
  }
})
