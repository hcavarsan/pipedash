import assert from 'node:assert/strict'
import { test } from 'node:test'

import { keyboardInset, keyboardOpen, scrollDeltaToReveal } from './useKeyboardInset'

test('inset is the part of the layout viewport covered by the keyboard', () => {
  assert.equal(keyboardInset(852, 852, 0), 0)
  assert.equal(keyboardInset(852, 520, 0), 332)
  assert.equal(keyboardInset(852, 520, 40), 292)
})

test('inset never goes negative', () => {
  assert.equal(keyboardInset(800, 852, 0), 0)
})

test('only a tall inset counts as the keyboard being open', () => {
  assert.equal(keyboardOpen(0), false)
  assert.equal(keyboardOpen(60), false)
  assert.equal(keyboardOpen(332), true)
})

test('a field already inside the visible area needs no scrolling', () => {
  assert.equal(scrollDeltaToReveal(200, 244, 100, 500, 12), 0)
})

test('a field below the visible area scrolls down just enough', () => {
  assert.equal(scrollDeltaToReveal(480, 524, 100, 500, 12), 36)
})

test('a field above the visible area scrolls up to the margin', () => {
  assert.equal(scrollDeltaToReveal(90, 134, 100, 500, 12), -22)
})

test('a field taller than the area aligns its top', () => {
  assert.equal(scrollDeltaToReveal(300, 900, 100, 500, 12), 188)
})
