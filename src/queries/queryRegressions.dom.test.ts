import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { runHistoryQuery } from '../lib/runHistoryQuery'
import { pipelineViewFixture, providerSummaryFixture } from '../testing/fixtures'
import { viewPresentationFixture } from '../testing/presentation'
import type { RefreshResult } from '../types/health'
import type { MetricsDashboard } from '../types/metrics'
import type { ProviderDraft } from '../types/provider_draft'
import type { ProviderSummary } from '../types/providers'
import type { RunHistoryPage } from '../types/run_history'
import type { PipelineViewPage, PipelineViewQuery } from '../types/views'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { notifyManager, QueryClient, QueryClientProvider, QueryObserver } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { apiService } = await import('../services/api')
const { tauriService } = await import('../services/tauri')
const { queryClient } = await import('../lib/queryClient')
const { notifications } = await import('../lib/notifications')
const { queryKeys } = await import('../lib/queryKeys')
const { usePlugins } = await import('./usePluginsQueries')
const { useMetricsDashboard } = await import('../features/metrics/queries/useMetricsQueries')
const { useRunHistory } = await import('./useRunHistoryQueries')
const { useSaveTablePreferences } = await import('../features/runs/queries/useTablePreferences')
const { useSaveProviderDraft } = await import('./useProvidersQueries')
const { useRefreshProviders } = await import('./useRefreshQueries')
const { useFactoryReset } = await import('../features/settings/queries/usePlatformQueries')
const { useLogViewerPrefs } = await import('../stores/logViewerPrefsStore')
const { useNotificationCenterStore } = await import('../stores/notificationCenterStore')

notifyManager.setNotifyFunction((callback) => act(callback))

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(MemoryRouter, null, createElement(QueryClientProvider, { client }, children))

type Stubbed =
  | 'fetchRunHistory'
  | 'getMetricsDashboard'
  | 'saveTablePreferences'
  | 'saveProviderDraft'
  | 'refreshProvider'
  | 'factoryReset'

function stub<K extends Stubbed>(context: TestContext, method: K, impl: (typeof tauriService)[K]) {
  context.mock.method(tauriService, method, impl)

  return context.mock.method(apiService, method, impl)
}

beforeEach(() => {
  client.clear()
})
afterEach(async () => {
  await act(async () => cleanup())
})
after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const historyOfSize = (size: number): RunHistoryPage => ({
  runs: [],
  total_count: size,
  has_more: false,
  can_scan_deeper: false,
  is_complete: true,
  scan_notice: null,
  page: 1,
  page_size: size,
  total_pages: 1,
  branches: [],
  actors: [],
})

const metrics: MetricsDashboard = {
  data: { metrics: [], total_count: 0, metric_type: 'run_duration', aggregation_period: 'daily' },
  observed: [],
  stats: { average: null, latest: null, max: null, observed: 0, total: 0 },
  table_indices: [],
  ranges: [],
  widest_range: '24h',
  aggregation_type: 'avg',
}

const viewQuery: PipelineViewQuery = { view: 'v', show_hidden: false, utc_offset_minutes: 0 }
const viewPage = (): PipelineViewPage => ({
  view: pipelineViewFixture({ id: 'v' }),
  pinned: [],
  pinned_blocks: [],
  sections: [],
  total: 1,
  hidden_count: 0,
  filters_active: false,
  section_count: 0,
  facets: { organizations: [], provider_ids: [] },
  presentation: viewPresentationFixture(),
})

const providerDraft = (providerId: number | null): ProviderDraft => ({
  provider_type: 'stub',
  name: 'New',
  auth_mode: 'token',
  values: { token: 'secret' },
  selected_pipeline_ids: null,
  provider_id: providerId,
})

test('run history with a different page size is fetched separately', async (context) => {
  const fetchRuns = stub(context, 'fetchRunHistory', async (_id, query) => historyOfSize(query.page_size))

  const small = renderHook(() => useRunHistory('p1', runHistoryQuery(1, 20)), { wrapper })
  await waitFor(() => assert.equal(small.result.current.data?.total_count, 20))
  const large = renderHook(() => useRunHistory('p1', runHistoryQuery(1, 50)), { wrapper })
  await waitFor(() => assert.equal(large.result.current.data?.total_count, 50))
  assert.equal(fetchRuns.mock.callCount(), 2)
})

test('aggregated metrics with a different limit are fetched separately', async (context) => {
  const query = stub(context, 'getMetricsDashboard', async () => metrics)
  const params = { metricType: 'run_duration', aggregationPeriod: 'daily', pipelineId: 'p1', dateRange: '24h' } as const

  const first = renderHook(() => useMetricsDashboard({ ...params, limit: 10 }), { wrapper })
  await waitFor(() => assert.equal(first.result.current.isSuccess, true))
  const second = renderHook(() => useMetricsDashboard({ ...params, limit: 500 }), { wrapper })
  await waitFor(() => assert.equal(second.result.current.isSuccess, true))
  assert.equal(query.mock.callCount(), 2)
})

test('failed table preference save without a cached value leaves no optimistic entry', async (context) => {
  stub(context, 'saveTablePreferences', async () => {
    throw new Error('disk full')
  })
  const { result } = renderHook(() => useSaveTablePreferences(), { wrapper })

  await act(async () => {
    await result.current
      .mutateAsync({ providerId: 1, tableId: 'runs', preferences: { columnOrder: ['a'] } })
      .catch(() => undefined)
  })

  assert.equal(client.getQueryData(queryKeys.tablePreferences.detail(1, 'runs')), undefined)
})

test('saving a provider draft invalidates the provider list and cached pipelines', async (context) => {
  stub(context, 'saveProviderDraft', async () => 7)
  client.setQueryData(queryKeys.pipelines.page(viewQuery), viewPage())
  client.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), [providerSummaryFixture({ id: 1 })])
  const { result } = renderHook(() => useSaveProviderDraft(), { wrapper })

  await act(async () => {
    await result.current.mutateAsync(providerDraft(null))
  })

  assert.equal(client.getQueryState(queryKeys.pipelines.page(viewQuery))?.isInvalidated, true)
  assert.equal(client.getQueryState(queryKeys.providers.list())?.isInvalidated, true)
})

test('saving an edited provider draft keeps credentials out of the cached summaries', async (context) => {
  const save = stub(context, 'saveProviderDraft', async () => 1)
  client.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), [providerSummaryFixture({ id: 1 })])
  const { result } = renderHook(() => useSaveProviderDraft(), { wrapper })

  await act(async () => {
    await result.current.mutateAsync(providerDraft(1))
  })

  const summary = client.getQueryData<ProviderSummary[]>(queryKeys.providers.list())?.[0]
  assert.equal(save.mock.callCount() > 0, true)
  assert.equal(summary !== undefined && 'token' in summary, false)
})

test('refreshing providers keeps in-flight ids marked when another provider fails', async (context) => {
  const slow = Promise.withResolvers<RefreshResult>()
  stub(context, 'refreshProvider', (id) => (id === 1 ? Promise.reject(new Error('down')) : slow.promise))
  const { result } = renderHook(() => useRefreshProviders(), { wrapper })
  let batch: Promise<void> = Promise.resolve()

  await act(async () => {
    batch = result.current.refresh([1, 2])
  })

  await waitFor(() => assert.equal(result.current.refreshing.has(2), true))
  assert.equal(result.current.refreshing.has(1), false)
  slow.resolve({ provider_id: 2, state: 'ok', kind: null, message: null })
  await act(async () => batch)
  await waitFor(() => assert.equal(result.current.refreshing.size, 0))
})

test('one fulfilled and two rejected provider refreshes give one summary naming both failures', async (context) => {
  const shown = context.mock.method(notifications, 'show', () => 'refresh-summary')
  client.setQueryData(queryKeys.providers.list(), [
    providerSummaryFixture({ id: 1, name: 'GitHub' }),
    providerSummaryFixture({ id: 2, name: 'GitLab' }),
    providerSummaryFixture({ id: 3, name: 'Jenkins' }),
  ])
  stub(context, 'refreshProvider', async (id) => {
    if (id === 1) {
      return { provider_id: 1, state: 'ok', kind: null, message: null }
    }

    throw new Error(id === 2 ? 'token rejected by host' : 'host unreachable')
  })
  const { result } = renderHook(() => useRefreshProviders(), { wrapper })

  await act(async () => result.current.refresh([1, 2, 3]))

  assert.equal(shown.mock.callCount(), 1)
  const options = shown.mock.calls[0]?.arguments[0]
  assert.equal(options?.color, 'orange')
  assert.equal(options?.message, 'Refreshed 1 of 3 · GitLab: token rejected by host · Jenkins: host unreachable')
})

test('factory reset also resets log preferences and the notification center', async (context) => {
  stub(context, 'factoryReset', async () => ({
    providers_removed: 0,
    caches_cleared: true,
    tokens_cleared: true,
    metrics_cleared: true,
  }))
  useLogViewerPrefs.setState({ wrap: false, timestamps: true })
  useNotificationCenterStore.getState().push({
    key: 'k',
    at: 1,
    tone: 'danger',
    level: null,
    title: 't',
    body: 'b',
    providerIds: [],
    pipelineId: null,
    link: null,
  })
  const { result } = renderHook(() => useFactoryReset(), { wrapper })

  await act(async () => {
    await result.current.mutateAsync()
  })

  assert.deepEqual([useLogViewerPrefs.getState().wrap, useLogViewerPrefs.getState().timestamps], [true, false])
  assert.equal(useNotificationCenterStore.getState().items.length, 0)
})

async function settle(context: TestContext, error: Error): Promise<{ calls: number; status: string }> {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  const production = new QueryClient({ defaultOptions: queryClient.getDefaultOptions() })
  let calls = 0
  const observer = new QueryObserver(production, {
    queryKey: ['retry-policy'],
    queryFn: async () => {
      calls += 1
      throw error
    },
  })
  const unsubscribe = observer.subscribe(() => undefined)

  for (let step = 0; step < 4; step += 1) {
    const turn = Promise.withResolvers<void>()

    setImmediate(turn.resolve)
    await turn.promise
    context.mock.timers.tick(1000)
  }
  unsubscribe()
  production.clear()

  return { calls, status: observer.getCurrentResult().status }
}

test('a permanent failure reaches the screen after one request', async (context) => {
  const result = await settle(context, Object.assign(new Error('missing'), { status: 404, code: 'NOT_FOUND' }))

  assert.deepEqual(result, { calls: 1, status: 'error' })
})

test('a transient failure is retried exactly once before it reaches the screen', async (context) => {
  const result = await settle(context, Object.assign(new Error('down'), { status: 503, code: 'SERVICE_UNAVAILABLE' }))

  assert.deepEqual(result, { calls: 2, status: 'error' })
})

test('plugins report a query error for a 503 NOT_INITIALIZED response instead of empty data', async (context) => {
  const rejection = () =>
    Promise.reject(Object.assign(new Error('not initialized'), { status: 503, code: 'NOT_INITIALIZED' }))

  context.mock.method(apiService, 'getAvailablePlugins', rejection)
  context.mock.method(tauriService, 'getAvailablePlugins', rejection)
  const { result } = renderHook(() => usePlugins(), { wrapper })

  await waitFor(() => assert.equal(result.current.isError, true))
  assert.equal(result.current.data, undefined)
})
