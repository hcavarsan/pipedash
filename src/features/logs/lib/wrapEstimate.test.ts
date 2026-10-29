import assert from 'node:assert/strict'
import { test } from 'node:test'

import { visualLines } from './wrapEstimate'

test('ascii wraps by column count and never drops below one line', () => {
  assert.equal(visualLines('', 80), 1)
  assert.equal(visualLines('a'.repeat(80), 80), 1)
  assert.equal(visualLines('a'.repeat(81), 80), 2)
  assert.equal(visualLines('a'.repeat(240), 80), 3)
  assert.equal(visualLines('abc', 0), 3)
})

test('tabs advance to the next multiple of eight', () => {
  assert.equal(visualLines('\t', 8), 1)
  assert.equal(visualLines('\t\t', 8), 2)
  assert.equal(visualLines('abc\tdef', 8), 2)
  assert.equal(visualLines('abcdefgh\t', 8), 2)
  assert.equal(visualLines('a\tb', 10), 1)
})

test('wide CJK characters count as two columns', () => {
  assert.equal(visualLines('日本語', 6), 1)
  assert.equal(visualLines('日本語', 5), 2)
  assert.equal(visualLines('한국어', 3), 2)
  assert.equal(visualLines('ａｂ', 4), 1)
  assert.equal(visualLines('日a', 3), 1)
})

test('surrogate pairs count as two units', () => {
  assert.equal(visualLines('😀😀', 4), 1)
  assert.equal(visualLines('😀😀', 3), 2)
})
