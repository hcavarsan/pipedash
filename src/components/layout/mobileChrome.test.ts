import assert from 'node:assert/strict'
import { test } from 'node:test'

import { BROWSE_PATH, fromOf, groupPath, mobileChrome, providerPath, routeKey, splitLocation } from './mobileChrome'

test('tabs follow the top-level route and the pinned view', () => {
  assert.equal(mobileChrome(BROWSE_PATH, '').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '?provider=3').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '?view=pinned').tab, 'pinned')
  assert.equal(mobileChrome('/pipelines/abc', '?view=pinned').tab, 'pipelines')
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
  assert.equal(mobileChrome('/pipelines', '?view=pinned').title, 'Pinned')
  assert.equal(mobileChrome(BROWSE_PATH, '').title, 'Browse')
  assert.equal(mobileChrome('/pipelines', '').title, 'All pipelines')
  assert.equal(mobileChrome('/pipelines', '?provider=1').title, 'Provider')
  assert.equal(mobileChrome('/pipelines', '?view=7').title, 'View')
  assert.equal(mobileChrome('/notifications', '').title, 'Notifications')
})

test('every pipelines list sits under Browse, and a history page under All pipelines', () => {
  assert.deepEqual(mobileChrome(BROWSE_PATH, ''), { title: 'Browse', back: null, tab: 'pipelines' })
  assert.deepEqual(mobileChrome('/pipelines', '').back, { to: BROWSE_PATH, label: 'Browse' })
  assert.deepEqual(mobileChrome('/pipelines', '?view=7').back, { to: BROWSE_PATH, label: 'Browse' })
  assert.deepEqual(mobileChrome('/pipelines/abc', ''), {
    title: 'History',
    back: { to: '/pipelines', label: 'All pipelines' },
    tab: 'pipelines',
  })
})

test('a provider list, its repository and history push one after another under Browse', () => {
  const provider = splitLocation(providerPath(1))
  const repo = groupPath('repo:acme/mobile')

  assert.deepEqual(mobileChrome(provider.pathname, provider.search, { from: BROWSE_PATH }), {
    title: 'Provider',
    back: { to: BROWSE_PATH, label: 'Browse' },
    tab: 'pipelines',
  })
  assert.deepEqual(mobileChrome(repo, provider.search, { from: providerPath(1) }), {
    title: 'mobile',
    back: { to: providerPath(1), label: 'Provider' },
    tab: 'pipelines',
  })
  assert.deepEqual(mobileChrome('/pipelines/abc', '', { from: repo }), {
    title: 'History',
    back: { to: repo, label: 'mobile' },
    tab: 'pipelines',
  })
  assert.equal(mobileChrome(groupPath('status:failed'), '').title, 'Failed')
})

test('a detail opened from pinned keeps the pinned tab and label', () => {
  assert.deepEqual(mobileChrome('/pipelines/abc', '', { from: '/pipelines?view=pinned' }), {
    title: 'History',
    back: { to: '/pipelines?view=pinned', label: 'Pinned' },
    tab: 'pinned',
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
  assert.deepEqual(mobileChrome(BROWSE_PATH, '', { from: '/notifications' }), mobileChrome(BROWSE_PATH, ''))
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
  assert.notEqual(routeKey('/pipelines/abc', '', { from: '/pipelines?view=pinned' }), routeKey('/pipelines/abc', ''))
  assert.equal(routeKey('/pipelines', '?provider=1'), routeKey('/pipelines', ''))
})
