import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatMetricValue } from '../features/metrics/lib/metricsUtils'
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

test('rounds once so durations never show 60 seconds or 60 minutes', () => {
  assert.equal(formatDuration(59.6), '1m')
  assert.equal(formatDuration(119.6), '2m')
  assert.equal(formatDuration(3599.6), '1h')
  assert.equal(formatDuration(7199), '2h')
})

test('a measured zero metric duration is shown as zero seconds', () => {
  assert.equal(formatMetricValue(0, 'run_duration'), '0s')
  assert.equal(formatMetricValue(null, 'run_duration'), '—')
})
