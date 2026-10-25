import assert from 'node:assert/strict'
import { test } from 'node:test'

import { keyboardInset } from './useKeyboardInset'

test('inset is the part of the layout viewport covered by the keyboard', () => {
  assert.equal(keyboardInset(852, 852, 0), 0)
  assert.equal(keyboardInset(852, 520, 0), 332)
  assert.equal(keyboardInset(852, 520, 40), 292)
})

test('inset never goes negative', () => {
  assert.equal(keyboardInset(800, 852, 0), 0)
})
