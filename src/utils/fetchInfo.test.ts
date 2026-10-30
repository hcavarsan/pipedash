import assert from 'node:assert/strict'
import { test } from 'node:test'

import { providerSummaryFixture } from '../testing/fixtures'
import type { ProviderSummary } from '../types/domain'

import { dataAge, describeFetch, providerTone, worstTone } from './fetchInfo'
import { formatRelativeTime } from './formatRelativeTime'

const NOW = Date.parse('2026-10-28T12:00:00Z')

const provider = (overrides: Partial<ProviderSummary>) =>
  providerSummaryFixture({
    name: 'GitHub',
    last_fetch_status: 'success',
    last_fetch_error: null,
    last_fetch_at: '2026-10-28T11:59:00Z',
    refresh_interval: 30,
    paused_until: null,
    freshness: { state: 'fresh', stale_at: null },
    ...overrides,
  })

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

test('tone passes the server state through', () => {
  for (const state of ['down', 'never', 'stale'] as const) {
    assert.equal(providerTone(provider({ freshness: { state, stale_at: null } }), NOW), state)
  }
})

test('a fresh provider turns stale once stale_at has passed', () => {
  const fresh = (staleAt: string | null) => provider({ freshness: { state: 'fresh', stale_at: staleAt } })

  assert.equal(providerTone(fresh('2026-10-28T12:05:00Z'), NOW), 'fresh')
  assert.equal(providerTone(fresh('2026-10-28T12:00:00Z'), NOW), 'fresh')
  assert.equal(providerTone(fresh('2026-10-28T11:59:59Z'), NOW), 'stale')
  assert.equal(providerTone(fresh(null), NOW), 'fresh')
})

test('a non-fresh state ignores stale_at', () => {
  assert.equal(providerTone(provider({ freshness: { state: 'down', stale_at: '2026-10-28T11:00:00Z' } }), NOW), 'down')
})

test('the worst tone wins', () => {
  assert.equal(worstTone(['fresh', 'never', 'stale']), 'stale')
  assert.equal(worstTone(['fresh', 'down', 'stale']), 'down')
  assert.equal(worstTone(['fresh', 'never']), 'never')
  assert.equal(worstTone([]), 'fresh')
})

test('data age counts from the last successful fetch and names a stale or down state', () => {
  const at = '2026-10-28T10:00:00Z'
  const ago = formatRelativeTime(at, NOW)
  const withAge = (overrides: Partial<ProviderSummary>) => provider({ last_success_at: at, ...overrides })
  const stale = withAge({ freshness: { state: 'fresh', stale_at: '2026-10-28T11:00:00Z' } })
  const down = withAge({
    last_fetch_status: 'error',
    last_fetch_at: '2026-10-28T11:59:00Z',
    freshness: { state: 'down', stale_at: null },
  })

  assert.equal(dataAge(withAge({}), NOW).label, `Data checked ${ago}`)
  assert.equal(dataAge(withAge({}), NOW).tone, 'fresh')
  assert.equal(dataAge(stale, NOW).label, `Stale · data checked ${ago}`)
  assert.equal(dataAge(down, NOW).label, `Down · data checked ${ago}`)
  assert.equal(
    dataAge(withAge({ last_success_at: null, freshness: { state: 'never', stale_at: null } }), NOW).label,
    'Never fetched'
  )
  assert.equal(
    dataAge(withAge({ last_success_at: null, freshness: { state: 'down', stale_at: null } }), NOW).label,
    'Down'
  )
})

test('a missing success time only claims no data when the provider never fetched', () => {
  const missing = (state: 'never' | 'down') =>
    dataAge(provider({ last_success_at: null, freshness: { state, stale_at: null } }), NOW).detail

  assert.equal(missing('never'), 'No pipeline data fetched yet')
  assert.equal(missing('down'), 'Last successful fetch time unknown')
})
