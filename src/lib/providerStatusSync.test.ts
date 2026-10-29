import assert from 'node:assert/strict'
import { test } from 'node:test'

import { QueryClient } from '@tanstack/react-query'

import type { ProviderConfig, ProviderSummary } from '../types'

import { applyProviderStatus } from './providerStatusSync'
import { queryKeys } from './queryKeys'

const summary = (name: string, level: ProviderSummary['health']['level']): ProviderSummary => ({
  id: 1,
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
  recovery: null,
  health: { level, signals: [], next_attempt_at: null, in_flight: false, status_page: null },
})

test('a list request that resolves after a status event does not overwrite it, and the detail cache is untouched', async () => {
  const queryClient = new QueryClient()
  const detail: ProviderConfig & { id: number } = {
    id: 1,
    name: 'editor copy',
    provider_type: 'github',
    token: 'secret',
    config: {},
    refresh_interval: 30,
  }

  queryClient.setQueryData(queryKeys.providers.list(), [summary('old', 'operational')])
  queryClient.setQueryData(queryKeys.providers.detail(1), detail)

  const { promise: delayed, resolve } = Promise.withResolvers<ProviderSummary[]>()
  const inFlight = queryClient
    .fetchQuery({ queryKey: queryKeys.providers.list(), queryFn: () => delayed, staleTime: 0 })
    .catch(() => undefined)

  await applyProviderStatus(queryClient, summary('fresh', 'major_outage'), () => true)
  resolve([summary('stale response', 'operational')])
  await inFlight

  const [first] = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? []

  assert.ok(first)
  assert.equal(first.name, 'fresh')
  assert.equal(first.health.level, 'major_outage')
  assert.equal(queryClient.getQueryData(queryKeys.providers.detail(1)), detail)
})

test('a status event whose subscription ended while cancelling leaves the list untouched', async () => {
  const queryClient = new QueryClient()
  queryClient.setQueryData(queryKeys.providers.list(), [summary('old', 'operational')])

  await applyProviderStatus(queryClient, summary('fresh', 'major_outage'), () => false)

  const [first] = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list()) ?? []

  assert.equal(first?.name, 'old')
})
