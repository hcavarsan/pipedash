import assert from 'node:assert/strict'
import { test } from 'node:test'

import { RowHeights } from './rowHeights'

function naiveIndexAt(heights: number[], y: number): number {
  if (heights.length === 0 || y <= 0) {
    return 0
  }

  let top = 0

  for (let index = 0; index < heights.length; index += 1) {
    const height = heights[index]
    if (height === undefined) {
      break
    }

    if (y < top + height) {
      return index
    }

    top += height
  }

  return heights.length - 1
}

function lcg(seed: number) {
  let state = seed

  return () => {
    state = (state * 1664525 + 1013904223) >>> 0

    return state / 2 ** 32
  }
}

test('prefix, total and indexAt match a naive array under random updates', () => {
  const random = lcg(7)

  for (const size of [1, 2, 3, 7, 64, 1000]) {
    const naive = Array.from({ length: size }, () => 10 + Math.floor(random() * 90))
    const tree = new RowHeights(size, (index) => naive[index] ?? 0)

    for (let step = 0; step < 300; step += 1) {
      const index = Math.floor(random() * size)
      const height = 10 + Math.floor(random() * 90)

      naive[index] = height
      tree.set(index, height)

      const probe = Math.floor(random() * size)
      const expected = naive.slice(0, probe).reduce((sum, value) => sum + value, 0)
      const total = naive.reduce((sum, value) => sum + value, 0)
      const y = Math.floor(random() * (total + 20)) - 10

      assert.equal(tree.prefix(probe), expected)
      assert.equal(tree.total(), total)
      assert.equal(tree.get(index), height)
      assert.equal(tree.indexAt(y), naiveIndexAt(naive, y))
    }
  }
})

test('indexAt handles empty trees, boundaries and the end', () => {
  const empty = new RowHeights(0, 18)

  assert.equal(empty.total(), 0)
  assert.equal(empty.indexAt(50), 0)

  const tree = new RowHeights(4, 10)

  assert.equal(tree.indexAt(-5), 0)
  assert.equal(tree.indexAt(0), 0)
  assert.equal(tree.indexAt(9.99), 0)
  assert.equal(tree.indexAt(10), 1)
  assert.equal(tree.indexAt(39.9), 3)
  assert.equal(tree.indexAt(40), 3)
  assert.equal(tree.indexAt(1e9), 3)
  assert.equal(tree.prefix(99), 40)
})
