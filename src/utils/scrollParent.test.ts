import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { findScrollParent } from './scrollParent'

GlobalRegistrator.register({ width: 393, height: 852 })
after(async () => {
  await GlobalRegistrator.unregister()
})

afterEach(() => {
  document.body.replaceChildren()
})

test('the nearest scrollable ancestor wins over outer scrollers', () => {
  const outer = document.createElement('div')
  const inner = document.createElement('div')
  const plain = document.createElement('div')
  const node = document.createElement('div')

  outer.style.overflowY = 'scroll'
  inner.style.overflowY = 'auto'
  plain.append(node)
  inner.append(plain)
  outer.append(inner)
  document.body.append(outer)

  assert.equal(findScrollParent(node), inner)
})

test('the document scroller is used when no ancestor scrolls', () => {
  const node = document.createElement('div')

  document.body.append(node)

  assert.equal(findScrollParent(node), document.scrollingElement ?? document.documentElement)
})
