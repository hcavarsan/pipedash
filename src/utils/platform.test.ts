import assert from 'node:assert/strict'
import { test } from 'node:test'

Object.assign(globalThis, {
  document: { baseURI: 'http://localhost/' },
  localStorage: { getItem: () => null, setItem: () => undefined, removeItem: () => undefined },
  window: { location: { protocol: 'http:', host: 'localhost' } },
})

// Dynamic import: the browser globals above must exist before the module graph loads.
const { isDesktopTauri, isMobileOs, isMobilePlatform, normalizePlatform } = await import('./platform')

test('maps Tauri OS names to the platforms the UI knows', () => {
  assert.equal(normalizePlatform('ios'), 'ios')
  assert.equal(normalizePlatform('android'), 'android')
  assert.equal(normalizePlatform('macos'), 'macos')
  assert.equal(normalizePlatform('windows'), 'windows')
  assert.equal(normalizePlatform('freebsd'), 'linux')
})

test('only ios and android count as mobile', () => {
  assert.equal(isMobileOs('ios'), true)
  assert.equal(isMobileOs('android'), true)
  assert.equal(isMobileOs('macos'), false)
  assert.equal(isMobileOs('linux'), false)
})

test('the web is neither mobile nor desktop Tauri, whatever the browser', () => {
  assert.equal(isMobilePlatform(), false)
  assert.equal(isDesktopTauri(), false)
})
