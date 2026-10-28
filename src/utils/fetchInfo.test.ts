import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { ProviderSummary } from '../types'

import { describeFetch, providerTone, worstTone } from './fetchInfo'
import { formatRelativeTime } from './formatRelativeTime'

const NOW = Date.parse('2026-10-28T12:00:00Z')

const provider = (overrides: Partial<ProviderSummary>) =>
  ({
    name: 'GitHub',
    last_fetch_status: 'success',
    last_fetch_error: null,
    last_fetch_at: '2026-10-28T11:59:00Z',
    refresh_interval: 30,
    paused_until: null,
    health: { level: 'operational' },
    ...overrides,
  }) as ProviderSummary

test('a provider that never fetched reads Never and Not fetched yet', () => {
  assert.deepEqual(describeFetch(provider({ last_fetch_status: 'never', last_fetch_at: null }), NOW), {
    name: 'GitHub',
    lastFetch: 'Never',
    status: 'Not fetched yet',
  })
})

test('a successful fetch shows the relative time first and the exact time after it', () => {
  const at = '2026-10-28T11:55:00Z'
  const { lastFetch, status } = describeFetch(provider({ last_fetch_at: at }), NOW)

  assert.ok(lastFetch.startsWith(`${formatRelativeTime(at, NOW)} (`))
  assert.equal(status, 'Success')
})

test('a failed fetch includes the error message when there is one', () => {
  assert.equal(
    describeFetch(provider({ last_fetch_status: 'error', last_fetch_error: 'bad token' }), NOW).status,
    'Failed: bad token'
  )
  assert.equal(describeFetch(provider({ last_fetch_status: 'error' }), NOW).status, 'Failed')
})

test('tone is down on a fetch error or an outage', () => {
  assert.equal(providerTone(provider({ last_fetch_status: 'error' }), NOW), 'down')
  assert.equal(providerTone(provider({ health: { level: 'major_outage' } } as Partial<ProviderSummary>), NOW), 'down')
  assert.equal(providerTone(provider({ health: { level: 'partial_outage' } } as Partial<ProviderSummary>), NOW), 'down')
})

test('tone is never when nothing was fetched and fresh after a recent success', () => {
  assert.equal(providerTone(provider({ last_fetch_status: 'never', last_fetch_at: null }), NOW), 'never')
  assert.equal(providerTone(provider({}), NOW), 'fresh')
})

test('tone turns stale after max(3 intervals, 10 minutes) or while paused', () => {
  assert.equal(providerTone(provider({ last_fetch_at: '2026-10-28T11:51:00Z' }), NOW), 'fresh')
  assert.equal(providerTone(provider({ last_fetch_at: '2026-10-28T11:49:00Z' }), NOW), 'stale')
  assert.equal(providerTone(provider({ refresh_interval: 600, last_fetch_at: '2026-10-28T11:35:00Z' }), NOW), 'fresh')
  assert.equal(providerTone(provider({ refresh_interval: 600, last_fetch_at: '2026-10-28T11:25:00Z' }), NOW), 'stale')
  assert.equal(providerTone(provider({ paused_until: '2026-10-28T12:05:00Z' }), NOW), 'stale')
  assert.equal(providerTone(provider({ paused_until: '2026-10-28T11:00:00Z' }), NOW), 'fresh')
})

test('the worst tone wins', () => {
  assert.equal(worstTone(['fresh', 'never', 'stale']), 'stale')
  assert.equal(worstTone(['fresh', 'down', 'stale']), 'down')
  assert.equal(worstTone(['fresh', 'never']), 'never')
  assert.equal(worstTone([]), 'fresh')
})
