import assert from 'node:assert/strict'
import { test } from 'node:test'

import {
  decidePull,
  decideRowSwipe,
  decideSheetDismiss,
  decideSwipeBack,
  fullSwipeArmed,
  lockAxis,
  nearestStop,
  releaseVelocity,
  resist,
  rowPosition,
} from './gestures'

test('axis stays undecided inside the slop and then follows the dominant direction', () => {
  assert.equal(lockAxis(3, 4), null)
  assert.equal(lockAxis(12, 4), 'x')
  assert.equal(lockAxis(-4, 12), 'y')
  assert.equal(lockAxis(10, 10), 'y')
})

test('release velocity is measured in px/s over the 100ms before release', () => {
  const samples = [
    { t: 0, x: 0, y: 0 },
    { t: 200, x: 100, y: 0 },
    { t: 250, x: 150, y: 0 },
    { t: 300, x: 200, y: 0 },
  ]

  assert.equal(releaseVelocity(samples, 300).vx, 1000)
  assert.deepEqual(releaseVelocity([], 0), { vx: 0, vy: 0 })
  assert.deepEqual(releaseVelocity([{ t: 5, x: 1, y: 1 }], 5), { vx: 0, vy: 0 })
})

test('holding still before lifting the finger drops the flick velocity', () => {
  const samples = [
    { t: 0, x: 0, y: 0 },
    { t: 50, x: 120, y: 0 },
    { t: 400, x: 120, y: 0 },
  ]

  assert.deepEqual(releaseVelocity(samples, 400), { vx: 0, vy: 0 })
})

test('resistance grows past the edge and never reaches the dimension', () => {
  assert.equal(resist(0, 800), 0)
  assert.equal(resist(-10, 800), 0)
  assert.ok(resist(50, 800) < 50)
  assert.ok(resist(10_000, 800) < 800)
  assert.ok(resist(200, 800) > resist(100, 800))
})

test('swipe back completes where the gesture is heading and cancels on reversal', () => {
  assert.equal(decideSwipeBack(220, 0, 393), 'complete')
  assert.equal(decideSwipeBack(100, 0, 393), 'cancel')
  assert.equal(decideSwipeBack(60, 800, 393), 'complete')
  assert.equal(decideSwipeBack(300, -400, 393), 'cancel')
  assert.equal(decideSwipeBack(10, 100, 393), 'cancel')
})

test('sheet dismisses where the drag is heading and snaps back on an upward flick', () => {
  assert.equal(decideSheetDismiss(420, 0, 800), 'dismiss')
  assert.equal(decideSheetDismiss(100, 0, 800), 'cancel')
  assert.equal(decideSheetDismiss(60, 900, 800), 'dismiss')
  assert.equal(decideSheetDismiss(600, -600, 800), 'cancel')
})

test('nearest stop follows the projection and never moves against a flick', () => {
  const stops = [0, 400, 800]

  assert.equal(nearestStop(380, 0, stops), 400)
  assert.equal(nearestStop(380, 1200, stops), 800)
  assert.equal(nearestStop(420, -1200, stops), 0)
  assert.equal(nearestStop(390, -200, stops), 0)
})

test('pull to refresh triggers at the threshold', () => {
  assert.equal(decidePull(63), 'cancel')
  assert.equal(decidePull(64), 'refresh')
})

test('row swipe opens, closes, or runs the primary action only when armed', () => {
  assert.equal(decideRowSwipe(200, 0, 160, 360), 'open')
  assert.equal(decideRowSwipe(60, 0, 160, 360), 'closed')
  assert.equal(decideRowSwipe(60, 600, 160, 360), 'open')
  assert.equal(decideRowSwipe(120, -600, 160, 360), 'closed')
  assert.equal(decideRowSwipe(230, 0, 160, 360), 'full')
  assert.equal(decideRowSwipe(150, 1400, 160, 360), 'open')
  assert.equal(decideRowSwipe(260, -600, 160, 360), 'closed')
  assert.equal(fullSwipeArmed(215, 360), false)
  assert.equal(fullSwipeArmed(216, 360), true)
})

test('row position resists past both edges', () => {
  assert.equal(rowPosition(80, 160, 360), 80)
  assert.ok(rowPosition(900, 160, 360) > 360)
  assert.ok(rowPosition(900, 160, 360) < 900)
  assert.ok(rowPosition(-80, 160, 360) > -80)
  assert.ok(rowPosition(-80, 160, 360) < 0)
})
