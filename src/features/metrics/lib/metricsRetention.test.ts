import assert from 'node:assert/strict'
import { test } from 'node:test'

import { validateRetentionDays } from './metricsRetention'

test('retention accepts whole days within the supported range', () => {
  for (const value of [1, 7, 90, '1', '90']) {
    assert.equal(validateRetentionDays(value), null)
  }
})

test('retention rejects blank, fractional and out-of-range drafts', () => {
  for (const value of ['', ' ', 0, 91, 1.5, 'abc', Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.notEqual(validateRetentionDays(value), null)
  }
})
