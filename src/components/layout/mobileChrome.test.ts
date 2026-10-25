import assert from 'node:assert/strict'
import { test } from 'node:test'

import { mobileChrome } from './mobileChrome'

test('tabs follow the top-level route and the pinned view', () => {
  assert.equal(mobileChrome('/pipelines', '').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '?provider=3').tab, 'pipelines')
  assert.equal(mobileChrome('/pipelines', '?view=pinned').tab, 'pinned')
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
  assert.equal(mobileChrome('/pipelines', '?view=pinned').title, 'Pinned')
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
