import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  decidePull,
  decideRowSwipe,
  decideSheetDismiss,
  decideSwipeBack,
  lockAxis,
  releaseVelocity,
  rowPosition,
  rubberBand,
} from './gestures'

test('axis stays undecided inside the slop and then follows the dominant direction', () => {
  assert.equal(lockAxis(3, 4), null)
  assert.equal(lockAxis(12, 4), 'x')
  assert.equal(lockAxis(-4, 12), 'y')
  assert.equal(lockAxis(10, 10), 'y')
})

test('release velocity uses only the last 100ms', () => {
  const samples = [
    { t: 0, x: 0, y: 0 },
    { t: 200, x: 100, y: 0 },
    { t: 250, x: 150, y: 0 },
    { t: 300, x: 200, y: 0 },
  ]

  assert.equal(releaseVelocity(samples).vx, 1)
  assert.deepEqual(releaseVelocity([]), { vx: 0, vy: 0 })
  assert.deepEqual(releaseVelocity([{ t: 5, x: 1, y: 1 }]), { vx: 0, vy: 0 })
})

test('rubber band resists and never exceeds the limit', () => {
  assert.equal(rubberBand(0, 100), 0)
  assert.equal(rubberBand(-10, 100), 0)
  assert.ok(rubberBand(50, 100) < 50)
  assert.ok(rubberBand(10_000, 100) < 100)
  assert.ok(rubberBand(200, 100) > rubberBand(100, 100))
})

test('swipe back completes on distance or a forward flick and cancels otherwise', () => {
  assert.equal(decideSwipeBack(170, 0, 393), 'complete')
  assert.equal(decideSwipeBack(100, 0, 393), 'cancel')
  assert.equal(decideSwipeBack(60, 0.8, 393), 'complete')
  assert.equal(decideSwipeBack(200, -0.5, 393), 'cancel')
  assert.equal(decideSwipeBack(10, 0.9, 393), 'cancel')
})

test('sheet dismisses on distance or a downward flick and snaps back otherwise', () => {
  assert.equal(decideSheetDismiss(320, 0, 800), 'dismiss')
  assert.equal(decideSheetDismiss(100, 0, 800), 'cancel')
  assert.equal(decideSheetDismiss(60, 0.9, 800), 'dismiss')
  assert.equal(decideSheetDismiss(500, -0.6, 800), 'cancel')
  assert.equal(decideSheetDismiss(10, 0.9, 800), 'cancel')
})

test('pull to refresh triggers at the threshold', () => {
  assert.equal(decidePull(49), 'cancel')
  assert.equal(decidePull(50), 'refresh')
})

test('row swipe opens, closes or runs the primary action', () => {
  assert.equal(decideRowSwipe(200, 0, 160, 360), 'open')
  assert.equal(decideRowSwipe(60, 0, 160, 360), 'closed')
  assert.equal(decideRowSwipe(100, 0.6, 160, 360), 'open')
  assert.equal(decideRowSwipe(120, -0.6, 160, 360), 'closed')
  assert.equal(decideRowSwipe(230, 0, 160, 360), 'full')
  assert.equal(decideRowSwipe(200, 1.4, 160, 360), 'full')
  assert.equal(decideRowSwipe(150, 1.4, 160, 360), 'open')
})

test('row position resists pulling past the closed edge and stops at the row width', () => {
  assert.equal(rowPosition(80, 160, 360), 80)
  assert.equal(rowPosition(900, 160, 360), 360)
  assert.ok(rowPosition(-80, 160, 360) > -80)
  assert.ok(rowPosition(-80, 160, 360) < 0)
})
