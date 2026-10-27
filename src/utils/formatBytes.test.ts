import assert from 'node:assert/strict'
import { test } from 'node:test'

import { formatBytes } from './formatBytes'

test('formats bytes in 1024 steps with one decimal from KB up', () => {
  assert.equal(formatBytes(0), '0 B')
  assert.equal(formatBytes(1023), '1023 B')
  assert.equal(formatBytes(1024), '1.0 KB')
  assert.equal(formatBytes(1536), '1.5 KB')
  assert.equal(formatBytes(1024 * 1024 - 1), '1.0 MB')
  assert.equal(formatBytes(300 * 1024 * 1024), '300.0 MB')
  assert.equal(formatBytes(5 * 1024 ** 3), '5.0 GB')
  assert.equal(formatBytes(2048 * 1024 ** 3), '2048.0 GB')
  assert.equal(formatBytes(-5), '0 B')
})
