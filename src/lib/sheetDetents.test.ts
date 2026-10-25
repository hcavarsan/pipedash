import assert from 'node:assert/strict'
import { test } from 'node:test'

import { nearestStop } from './gestures'
import { detentHeights, displayPosition, expandLevel, scrimLevel, sheetFrame, stopPositions } from './sheetDetents'

test('detent heights are unique and ascending', () => {
  assert.deepEqual(detentHeights(['large', 'medium'], 800), [400, 800])
  assert.deepEqual(detentHeights(['medium', 'medium'], 801), [401])
})

test('stops run from the tallest detent down to dismissal', () => {
  assert.deepEqual(stopPositions([400, 800], true), [0, 400, 800])
  assert.deepEqual(stopPositions([400, 800], false), [0, 400])
  assert.deepEqual(stopPositions([300], true), [0, 300])
})

test('a slow release settles on the nearest detent, a fast one projects onward', () => {
  const stops = stopPositions([400, 800], true)

  assert.equal(nearestStop(150, 0, stops), 0)
  assert.equal(nearestStop(250, 0, stops), 400)
  assert.equal(nearestStop(10, 1500, stops), 800)
  assert.equal(nearestStop(390, -1500, stops), 0)
})

test('position rubber-bands above the top and below a non-dismissible floor only', () => {
  assert.equal(displayPosition(100, 800, Infinity), 100)
  assert.ok(displayPosition(-100, 800, Infinity) > -100)
  assert.ok(displayPosition(-100, 800, Infinity) < 0)
  assert.ok(displayPosition(500, 800, 400) < 500)
  assert.ok(displayPosition(500, 800, 400) > 400)
  assert.equal(displayPosition(900, 800, Infinity), 900)
})

test('frame grows with the position above the lowest detent and translates below it', () => {
  const heights = [400, 800]

  assert.deepEqual(sheetFrame(0, heights), { height: 800, offset: 0 })
  assert.deepEqual(sheetFrame(200, heights), { height: 600, offset: 0 })
  assert.deepEqual(sheetFrame(600, heights), { height: 400, offset: 200 })
  assert.deepEqual(sheetFrame(-50, heights), { height: 850, offset: 0 })
})

test('scrim and expansion follow the same frame', () => {
  const heights = [400, 800]

  assert.equal(scrimLevel(sheetFrame(0, heights), heights), 1)
  assert.equal(scrimLevel(sheetFrame(600, heights), heights), 0.5)
  assert.equal(scrimLevel(sheetFrame(800, heights), heights), 0)
  assert.equal(expandLevel(600, heights), 0.5)
  assert.equal(expandLevel(300, heights), 0)
  assert.equal(expandLevel(400, [400]), null)
})
