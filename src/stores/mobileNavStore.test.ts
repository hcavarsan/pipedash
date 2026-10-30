import assert from 'node:assert/strict'
import { test } from 'node:test'

import { rememberRoute, rememberScroll, resumeRoute, type TabRoutes } from './mobileNavStore'

test('each tab remembers its last route, including a pushed detail', () => {
  let tabs: TabRoutes = {}

  tabs = rememberRoute(tabs, 'pinned', { path: '/pipelines?view=pinned', from: null })
  tabs = rememberRoute(tabs, 'pinned', { path: '/pipelines/abc', from: '/pipelines?view=pinned' })
  tabs = rememberRoute(tabs, 'settings', { path: '/settings/storage', from: null })

  assert.deepEqual(resumeRoute(tabs, 'pinned', '/pipelines?view=pinned'), {
    path: '/pipelines/abc',
    from: '/pipelines?view=pinned',
  })
  assert.deepEqual(resumeRoute(tabs, 'settings', '/settings'), { path: '/settings/storage', from: null })
})

test('an unvisited tab resumes at its root', () => {
  assert.deepEqual(resumeRoute({}, 'settings', '/settings'), { path: '/settings', from: null })
})

test('remembering an identical route keeps the same object', () => {
  const tabs = rememberRoute({}, 'settings', { path: '/settings', from: null })

  assert.equal(rememberRoute(tabs, 'settings', { path: '/settings', from: null }), tabs)
})

test('scroll offsets are stored per route and dropped at the top', () => {
  let scrolls: Record<string, number> = {}

  scrolls = rememberScroll(scrolls, 'pipelines|/pipelines', 320)
  scrolls = rememberScroll(scrolls, 'providers|/providers', 40)
  assert.deepEqual(scrolls, { 'pipelines|/pipelines': 320, 'providers|/providers': 40 })

  scrolls = rememberScroll(scrolls, 'pipelines|/pipelines', 0)
  assert.deepEqual(scrolls, { 'providers|/providers': 40 })
})

test('the scroll memory evicts the oldest routes', () => {
  let scrolls: Record<string, number> = {}

  for (let index = 0; index < 45; index += 1) {
    scrolls = rememberScroll(scrolls, `route-${index}`, index + 1)
  }

  const keys = Object.keys(scrolls)

  assert.equal(keys.length, 40)
  assert.equal(keys[0], 'route-5')
  assert.equal(scrolls['route-44'], 45)
})
