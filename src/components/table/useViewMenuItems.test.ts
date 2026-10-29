import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

GlobalRegistrator.register()
after(async () => {
  await GlobalRegistrator.unregister()
})

const { moveId } = await import('./useViewMenuItems')

const ids = ['a', 'b', 'c', 'd']

test('moving one step up swaps with the previous sibling', () => {
  assert.deepEqual(moveId(ids, 2, 1), ['a', 'c', 'b', 'd'])
})

test('moving one step down swaps with the next sibling', () => {
  assert.deepEqual(moveId(ids, 1, 2), ['a', 'c', 'b', 'd'])
})

test('moving to the first and last position', () => {
  assert.deepEqual(moveId(ids, 3, 0), ['d', 'a', 'b', 'c'])
  assert.deepEqual(moveId(ids, 0, 3), ['b', 'c', 'd', 'a'])
})

test('the input array is left untouched', () => {
  const input = [...ids]
  moveId(input, 0, 2)
  assert.deepEqual(input, ids)
})
