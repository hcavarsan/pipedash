import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { pipelineViewFixture, providerSummaryFixture, viewPipelineFixture } from '../testing/fixtures'
import type {
  AggregatedMetrics,
  PaginatedRunHistory,
  PipelineViewPage,
  PipelineViewQuery,
  ProviderConfig,
  ProviderSummary,
  RefreshResult,
} from '../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { apiService } = await import('../services/api')
const { tauriService } = await import('../services/tauri')
const { queryKeys } = await import('../lib/queryKeys')
const { useAggregatedMetrics } = await import('../features/metrics/queries/useMetricsQueries')
const { useRunHistory, useSaveTablePreferences } = await import('./useRunHistoryQueries')
const { useToggleHidden, useTogglePin } = await import('../features/pipelines/queries/usePipelineViewQueries')
const { useAddProvider, useUpdateProvider } = await import('./useProvidersQueries')
const { useRefreshProviders } = await import('./useRefreshQueries')
const { useRerunWorkflow } = await import('../features/runs/queries/useRunDetailsQuery')
const { useFactoryReset } = await import('../features/settings/queries/usePlatformQueries')
const { useLogViewerPrefs } = await import('../stores/logViewerPrefsStore')
const { useNotificationCenterStore } = await import('../stores/notificationCenterStore')

notifyManager.setNotifyFunction((callback) => act(callback))

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(MemoryRouter, null, createElement(QueryClientProvider, { client }, children))

type Stubbed =
  | 'fetchRunHistory'
  | 'queryAggregatedMetrics'
  | 'saveTablePreferences'
  | 'setPipelineViewMembership'
  | 'setPipelineHidden'
  | 'updateProvider'
  | 'addProvider'
  | 'refreshProvider'
  | 'triggerPipeline'
  | 'getWorkflowRunDetails'
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

const historyOfSize = (size: number): PaginatedRunHistory => ({
  runs: [],
  total_count: size,
  has_more: false,
  is_complete: true,
  page: 1,
  page_size: size,
  total_pages: 1,
})

const metrics: AggregatedMetrics = {
  metrics: [],
  total_count: 0,
  metric_type: 'run_duration',
  aggregation_period: 'daily',
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
  facets: { organizations: [], provider_ids: [] },
})

const providerConfig: ProviderConfig = {
  name: 'New',
  provider_type: 'github',
  token: 'secret',
  config: {},
  refresh_interval: 60,
}

test('run history with a different page size is fetched separately', async (context) => {
  const fetchRuns = stub(context, 'fetchRunHistory', async (_id, _page, size) => historyOfSize(size ?? 0))

  const small = renderHook(() => useRunHistory('p1', 1, 20), { wrapper })
  await waitFor(() => assert.equal(small.result.current.data?.total_count, 20))
  const large = renderHook(() => useRunHistory('p1', 1, 50), { wrapper })
  await waitFor(() => assert.equal(large.result.current.data?.total_count, 50))
  assert.equal(fetchRuns.mock.callCount(), 2)
})

test('aggregated metrics with a different limit are fetched separately', async (context) => {
  const query = stub(context, 'queryAggregatedMetrics', async () => metrics)
  const params = { metricType: 'run_duration', aggregationPeriod: 'daily', pipelineId: 'p1' } as const

  const first = renderHook(() => useAggregatedMetrics({ ...params, limit: 10 }), { wrapper })
  await waitFor(() => assert.equal(first.result.current.isSuccess, true))
  const second = renderHook(() => useAggregatedMetrics({ ...params, limit: 500 }), { wrapper })
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

for (const [name, useToggle, method] of [
  ['pin', useTogglePin, 'setPipelineViewMembership'],
  ['hide', useToggleHidden, 'setPipelineHidden'],
] as const) {
  test(`a rejected ${name} toggle restores the cached view page`, async (context) => {
    stub(context, method, async () => {
      throw new Error('nope')
    })
    const row = viewPipelineFixture({ id: 'a', pinned: false, hidden: false })
    const initial: PipelineViewPage = {
      ...viewPage(),
      sections: [
        {
          id: 's',
          kind: 'view',
          title: 'S',
          organization: null,
          provider_ids: [],
          status: null,
          status_pipeline_id: null,
          last_run: null,
          pending_fetch: false,
          pipelines: [row],
        },
      ],
    }
    client.setQueryData(queryKeys.pipelines.page(viewQuery), initial)
    const { result } = renderHook(() => useToggle(), { wrapper })

    await act(async () => {
      await result.current.mutateAsync(row).catch(() => undefined)
    })

    const restored = client.getQueryData<PipelineViewPage>(queryKeys.pipelines.page(viewQuery))?.sections[0]
      ?.pipelines[0]
    assert.deepEqual([restored?.pinned, restored?.hidden], [false, false])
  })
}

test('provider edit does not seed an absent list and keeps credentials out of summaries', async (context) => {
  stub(context, 'updateProvider', async () => undefined)
  const { result } = renderHook(() => useUpdateProvider(), { wrapper })

  await act(async () => {
    await result.current.mutateAsync({ id: 1, config: providerConfig })
  })
  assert.equal(client.getQueryData(queryKeys.providers.list()), undefined)

  client.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), [providerSummaryFixture({ id: 1 })])
  const pending = Promise.withResolvers<undefined>()
  stub(context, 'updateProvider', () => pending.promise)
  let mutation: Promise<void> = Promise.resolve()
  await act(async () => {
    mutation = result.current.mutateAsync({ id: 1, config: providerConfig })
  })
  const summary = client.getQueryData<ProviderSummary[]>(queryKeys.providers.list())?.[0]
  assert.equal(summary?.name, 'New')
  assert.equal(summary !== undefined && 'token' in summary, false)
  pending.resolve(undefined)
  await act(async () => mutation)
})

test('adding a provider invalidates cached pipelines', async (context) => {
  stub(context, 'addProvider', async () => 7)
  client.setQueryData(queryKeys.pipelines.page(viewQuery), viewPage())
  const { result } = renderHook(() => useAddProvider(), { wrapper })

  await act(async () => {
    await result.current.mutateAsync(providerConfig)
  })

  assert.equal(client.getQueryState(queryKeys.pipelines.page(viewQuery))?.isInvalidated, true)
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

test('rerun succeeds as soon as the trigger succeeds even if details are not available yet', async (context) => {
  stub(context, 'triggerPipeline', async () => JSON.stringify({ run_number: 9 }))
  const details = stub(context, 'getWorkflowRunDetails', () => Promise.reject(new Error('not yet')))
  const { result } = renderHook(() => useRerunWorkflow(), { wrapper })
  const started = Date.now()
  let created: { newRunNumber: number } | undefined

  await act(async () => {
    created = await result.current.mutateAsync({ pipelineId: 'github__1__o/r' })
  })

  assert.ok(Date.now() - started < 1500)
  assert.equal(created?.newRunNumber, 9)
  assert.ok(details.mock.callCount() <= 1)
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
