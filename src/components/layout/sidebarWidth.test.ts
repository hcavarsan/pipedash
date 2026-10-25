import assert from 'node:assert/strict'
import { test } from 'node:test'

import { SIDEBAR_DEFAULT_WIDTH, SIDEBAR_MAX_WIDTH, sidebarCssWidth, sizeAfterStep, sizeAtOffset } from './sidebarWidth'

const expanded = { width: 300, compact: false }
const rail = { width: 300, compact: true }
const wide = 1600

test('chooses the expanded release target within the width limits', () => {
  assert.deepEqual(sizeAtOffset(250.6, expanded, wide), { width: 251, compact: false })
  assert.deepEqual(sizeAtOffset(150, expanded, wide), { width: 200, compact: false })
  assert.deepEqual(sizeAtOffset(900, expanded, wide), { width: SIDEBAR_MAX_WIDTH, compact: false })
})

test('chooses the compact release target below the midpoint and preserves expanded width', () => {
  assert.deepEqual(sizeAtOffset(133, expanded, wide), { width: 300, compact: true })
  assert.deepEqual(sizeAtOffset(134, rail, wide), { width: 200, compact: false })
})

test('caps the width at 40% of a narrow window', () => {
  assert.deepEqual(sizeAtOffset(400, expanded, 800), { width: 320, compact: false })
  assert.deepEqual(sizeAtOffset(400, expanded, 300), { width: 200, compact: false })
})

test('keyboard steps shrink into the rail and grow back out of it', () => {
  assert.deepEqual(sizeAfterStep(-16, { width: 210, compact: false }, wide), { width: 210, compact: true })
  assert.deepEqual(sizeAfterStep(-16, rail, wide), rail)
  assert.deepEqual(sizeAfterStep(16, rail, wide), { width: 300, compact: false })
  assert.deepEqual(sizeAfterStep(16, { width: SIDEBAR_MAX_WIDTH, compact: false }, wide), {
    width: SIDEBAR_MAX_WIDTH,
    compact: false,
  })
  assert.deepEqual(sizeAfterStep(-16, { width: SIDEBAR_MAX_WIDTH, compact: false }, 900), {
    width: 344,
    compact: false,
  })
})

test('clamps a stored width that is out of range or not a number', () => {
  assert.equal(sidebarCssWidth(300), 'clamp(200px, 300px, 40vw)')
  assert.equal(sidebarCssWidth(5000), `clamp(200px, ${SIDEBAR_MAX_WIDTH}px, 40vw)`)
  assert.equal(sidebarCssWidth(Number.NaN), `clamp(200px, ${SIDEBAR_DEFAULT_WIDTH}px, 40vw)`)
})
