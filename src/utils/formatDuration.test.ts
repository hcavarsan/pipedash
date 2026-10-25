import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatDuration } from './formatDuration'

test('distinguishes measured zero from missing or invalid durations', () => {
  assert.equal(formatDuration(0), '0s')
  for (const value of [null, undefined, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.equal(formatDuration(value), '—')
  }
})

test('keeps the existing second, minute and hour duration display', () => {
  assert.equal(formatDuration(12), '12s')
  assert.equal(formatDuration(60), '1m')
  assert.equal(formatDuration(75), '1m 15s')
  assert.equal(formatDuration(3600), '1h')
  assert.equal(formatDuration(5400), '1h 30m')
})
