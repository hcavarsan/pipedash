import assert from 'node:assert/strict'
import { test } from 'node:test'

import { MAX_SCROLL_PX, needsRebase, planWindow, WINDOW_ROWS } from './logViewport'

test('three million rows of 2000px stay under the scroll cap around the anchor', () => {
  const window = planWindow(3_000_000, 1_500_000, () => 2000)

  assert.ok(window.size * 2000 < MAX_SCROLL_PX)
  assert.ok(window.size >= 3990)
  assert.ok(window.base <= 1_500_000 && window.base + window.size > 1_500_000)
  assert.ok(Math.abs(1_500_000 - window.base - (window.base + window.size - 1_500_000)) <= 2)
})

test('the row cap applies to small rows', () => {
  const window = planWindow(3_000_000, 10, () => 10)

  assert.equal(window.size, WINDOW_ROWS)
  assert.equal(window.base, 0)
})

test('windows clamp at both ends and handle tiny inputs', () => {
  assert.deepEqual(
    planWindow(0, 5, () => 18),
    { base: 0, size: 0 }
  )
  assert.deepEqual(
    planWindow(10, 99, () => 18),
    { base: 0, size: 10 }
  )
  assert.deepEqual(
    planWindow(1, 0, () => 18),
    { base: 0, size: 1 }
  )

  const end = planWindow(1_000_000, 999_999, () => 18)

  assert.equal(end.base + end.size, 1_000_000)
  assert.equal(end.size, WINDOW_ROWS)
})

test('one row taller than the cap is still returned', () => {
  assert.deepEqual(
    planWindow(5, 2, () => MAX_SCROLL_PX * 2),
    { base: 2, size: 1 }
  )
})

test('needsRebase only fires in the outer 15% when rows exist beyond the window', () => {
  assert.equal(needsRebase(1000, 1000, 10_000, 1000), true)
  assert.equal(needsRebase(1000, 1000, 10_000, 1149), true)
  assert.equal(needsRebase(1000, 1000, 10_000, 1150), false)
  assert.equal(needsRebase(1000, 1000, 10_000, 1849), false)
  assert.equal(needsRebase(1000, 1000, 10_000, 1850), true)
  assert.equal(needsRebase(0, 1000, 10_000, 10), false)
  assert.equal(needsRebase(9000, 1000, 10_000, 9990), false)
  assert.equal(needsRebase(0, 1000, 1000, 500), false)
})
