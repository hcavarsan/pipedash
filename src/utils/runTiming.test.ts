import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatLiveRunDuration, isLiveRun } from './runTiming'

const START = '2026-10-29T12:00:00Z'
const at = (seconds: number) => Date.parse(START) + seconds * 1000

test('only running runs with a valid start time tick', () => {
  assert.equal(isLiveRun({ status: 'running', started_at: START }), true)
  assert.equal(isLiveRun({ status: 'running', started_at: '' }), false)
  assert.equal(isLiveRun({ status: 'running', started_at: 'soon' }), false)
  assert.equal(isLiveRun({ status: 'pending', started_at: START }), false)
  assert.equal(isLiveRun({ status: 'success', started_at: START }), false)
})

test('elapsed time is shown with the usual duration while under it', () => {
  assert.equal(formatLiveRunDuration({ started_at: START }, at(125), 300), '2m 5s · usually 5m')
})

test('past the usual duration the text says so without changing the elapsed format', () => {
  assert.equal(formatLiveRunDuration({ started_at: START }, at(420), 300), '7m · past usual 5m')
  assert.equal(formatLiveRunDuration({ started_at: START }, at(300), 300), '5m · usually 5m')
})

test('without a usable typical duration only the elapsed time is shown', () => {
  assert.equal(formatLiveRunDuration({ started_at: START }, at(90)), '1m 30s')
  assert.equal(formatLiveRunDuration({ started_at: START }, at(90), null), '1m 30s')
  assert.equal(formatLiveRunDuration({ started_at: START }, at(90), 0), '1m 30s')
})

test('a start time ahead of the local clock never shows negative elapsed time', () => {
  assert.equal(formatLiveRunDuration({ started_at: START }, at(-5), 300), '0s · usually 5m')
})
