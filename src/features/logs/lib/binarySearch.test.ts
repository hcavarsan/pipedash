import assert from 'node:assert/strict'
import { test } from 'node:test'

import { partitionPoint } from './binarySearch'

const values = [1, 3, 3, 5, 8]

test('partitionPoint returns the first index where the predicate turns false', () => {
  assert.equal(
    partitionPoint(values.length, (index) => (values[index] ?? Infinity) < 3),
    1
  )
  assert.equal(
    partitionPoint(values.length, (index) => (values[index] ?? Infinity) <= 3),
    3
  )
})

test('partitionPoint handles empty and fully matching ranges', () => {
  assert.equal(
    partitionPoint(0, () => true),
    0
  )
  assert.equal(
    partitionPoint(values.length, () => true),
    values.length
  )
  assert.equal(
    partitionPoint(values.length, () => false),
    0
  )
})
