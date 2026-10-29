import assert from 'node:assert/strict'
import { afterEach, test } from 'node:test'

import { shortcutLabel } from './shortcuts'

const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator')

afterEach(() => {
  if (originalNavigator) {
    Object.defineProperty(globalThis, 'navigator', originalNavigator)
  } else {
    Reflect.deleteProperty(globalThis, 'navigator')
  }
})

test('formats Mac commands with native modifier symbols', () => {
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { platform: 'MacIntel' } })
  assert.equal(shortcutLabel('mod+k'), '⌘K')
  assert.equal(shortcutLabel('mod+shift+r'), '⌘⇧R')
  assert.equal(shortcutLabel('mod+,'), '⌘,')
})

test('formats other platforms with readable modifier separators', () => {
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { platform: 'Linux x86_64' } })
  assert.equal(shortcutLabel('mod+k'), 'Ctrl+K')
  assert.equal(shortcutLabel('mod+\\'), 'Ctrl+\\')
})

test('falls back to Ctrl labels when there is no navigator', () => {
  Reflect.deleteProperty(globalThis, 'navigator')
  assert.equal(shortcutLabel('mod+k'), 'Ctrl+K')
})
