import assert from 'node:assert/strict'
import { test } from 'node:test'

import type { ProviderSummary } from '../types'

import { patchProviderList } from './patchProviderList'

const summary = (id: number, name: string): ProviderSummary => ({
  id,
  name,
  provider_type: 'github',
  icon: null,
  pipeline_count: 0,
  last_updated: null,
  refresh_interval: 30,
  configured_repositories: [],
  last_fetch_status: 'success',
  last_fetch_error: null,
  last_fetch_error_kind: null,
  last_fetch_at: null,
  last_success_at: null,
  paused_until: null,
  freshness: { state: 'never', stale_at: null },
  sign_in: false,
  health: { level: 'operational', signals: [], next_attempt_at: null, in_flight: false, status_page: null },
})

test('replaces the matching provider and keeps every other entry untouched', () => {
  const first = summary(1, 'one')
  const second = summary(2, 'two')
  const third = summary(3, 'three')
  const updated = summary(2, 'two-updated')

  const result = patchProviderList([first, second, third], updated)

  assert.ok(result)
  assert.equal(result[0], first)
  assert.equal(result[1], updated)
  assert.equal(result[2], third)
})

test('leaves the list as is when the provider is unknown or the list is not loaded', () => {
  const list = [summary(1, 'one')]

  assert.equal(patchProviderList(list, summary(9, 'new')), list)
  assert.equal(patchProviderList(undefined, summary(1, 'one')), undefined)
})
