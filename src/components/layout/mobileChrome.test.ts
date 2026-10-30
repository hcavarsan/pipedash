import assert from 'node:assert/strict'
import { test } from 'node:test'

import { fromOf, mobileChrome, routeKey, splitLocation } from './mobileChrome'

test('tabs follow the top-level route', () => {
  assert.equal(mobileChrome('/pipelines', '').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '?provider=3').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '?view=pinned').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines/abc', '?view=pinned').tab, 'pipelines')
  assert.equal(mobileChrome('/providers', '').tab, 'providers')
  assert.equal(mobileChrome('/notifications', '').tab, 'notifications')
  assert.equal(mobileChrome('/settings/storage', '').tab, 'settings')
})

test('settings sections push a back button to the settings root', () => {
  assert.deepEqual(mobileChrome('/settings', ''), { title: 'Settings', back: null, tab: 'settings' })
  assert.deepEqual(mobileChrome('/settings/storage', ''), {
    title: 'Storage',
    back: { to: '/settings', label: 'Settings' },
    tab: 'settings',
  })
  assert.equal(mobileChrome('/settings/unknown', '').back, null)
})

test('titles name the screen', () => {
  assert.equal(mobileChrome('/pipelines', '?view=pinned').title, 'Pipelines')
  assert.equal(mobileChrome('/pipelines', '').title, 'Pipelines')
  assert.equal(mobileChrome('/providers', '').title, 'Providers')
  assert.equal(mobileChrome('/notifications', '').title, 'Notifications')
})

test('a pipeline route pushes a back button to the list', () => {
  assert.deepEqual(mobileChrome('/pipelines/abc', ''), {
    title: 'History',
    back: { to: '/pipelines', label: 'Pipelines' },
    tab: 'pipelines',
  })
})

test('a detail opened from alerts belongs to the notifications tab', () => {
  const chrome = mobileChrome('/pipelines/abc', '', { from: '/notifications' })

  assert.equal(chrome.tab, 'notifications')
  assert.deepEqual(chrome.back, { to: '/notifications', label: 'Notifications' })
})

test('invalid or self-referencing origins fall back to the canonical parent', () => {
  const canonical = mobileChrome('/pipelines/abc', '')

  assert.deepEqual(mobileChrome('/pipelines/abc', '', { from: 5 }), canonical)
  assert.deepEqual(mobileChrome('/pipelines/abc', '', { from: 'pipelines' }), canonical)
  assert.deepEqual(mobileChrome('/pipelines/abc', '', { from: '/pipelines/abc' }), canonical)
  assert.deepEqual(mobileChrome('/pipelines/abc', '', null), canonical)
})

test('roots ignore an origin', () => {
  assert.deepEqual(mobileChrome('/providers', '', { from: '/notifications' }), mobileChrome('/providers', ''))
})

test('fromOf only accepts absolute paths', () => {
  assert.equal(fromOf({ from: '/settings' }), '/settings')
  assert.equal(fromOf({ from: 'settings' }), null)
  assert.equal(fromOf(undefined), null)
})

test('locations split into path and query', () => {
  assert.deepEqual(splitLocation('/pipelines?view=pinned'), { pathname: '/pipelines', search: '?view=pinned' })
  assert.deepEqual(splitLocation('/settings'), { pathname: '/settings', search: '' })
})

test('route keys separate the same path under different tabs', () => {
  assert.notEqual(routeKey('/pipelines/abc', '', { from: '/notifications' }), routeKey('/pipelines/abc', ''))
  assert.equal(routeKey('/pipelines', '?provider=1'), routeKey('/pipelines', ''))
})
