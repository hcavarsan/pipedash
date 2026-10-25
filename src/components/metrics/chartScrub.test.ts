import assert from 'node:assert/strict'
import { test } from 'node:test'

import { nearestIndex } from './chartScrub'

const span = { left: 50, width: 200 }

test('point scale snaps to the nearest datum and clamps outside the plot', () => {
  assert.equal(nearestIndex(0, span, 5, false), 0)
  assert.equal(nearestIndex(50, span, 5, false), 0)
  assert.equal(nearestIndex(149, span, 5, false), 2)
  assert.equal(nearestIndex(250, span, 5, false), 4)
  assert.equal(nearestIndex(999, span, 5, false), 4)
})

test('band scale picks the band under the finger', () => {
  assert.equal(nearestIndex(50, span, 4, true), 0)
  assert.equal(nearestIndex(99, span, 4, true), 0)
  assert.equal(nearestIndex(101, span, 4, true), 1)
  assert.equal(nearestIndex(250, span, 4, true), 3)
})

test('a single datum and an empty plot are handled', () => {
  assert.equal(nearestIndex(120, span, 1, false), 0)
  assert.equal(nearestIndex(120, span, 0, false), null)
  assert.equal(nearestIndex(120, { left: 0, width: 0 }, 3, false), null)
})
