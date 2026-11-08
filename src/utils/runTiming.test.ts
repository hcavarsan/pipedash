import assert from 'node:assert/strict'
import { test } from 'node:test'

import { runPaceFixture } from '../testing/presentation'
import { formatLiveRunDuration, isLiveRun } from './runTiming'

const START = '2026-10-29T12:00:00Z'
const at = (seconds: number) => Date.parse(START) + seconds * 1000
const run = (pace = {}) => ({ started_at: START, pace: runPaceFixture({ live: true, ...pace }) })

test('only runs the backend marks live tick', () => {
  assert.equal(isLiveRun(run()), true)
  assert.equal(isLiveRun(run({ live: false })), false)
})

test('shows the backend typical duration and past-usual flag', () => {
  assert.equal(formatLiveRunDuration(run({ typical_duration_seconds: 300 }), at(125)), '2m 5s · usually 5m')
  assert.equal(
    formatLiveRunDuration(run({ typical_duration_seconds: 300, past_usual: true }), at(420)),
    '7m · past usual 5m'
  )
  assert.equal(formatLiveRunDuration(run(), at(90)), '1m 30s')
  assert.equal(formatLiveRunDuration(run({ typical_duration_seconds: 300 }), at(-5)), '0s · usually 5m')
})
