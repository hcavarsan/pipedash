import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { dashboardFixture } from '../../../testing/metrics'
import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import type { MetricsConfig, MetricsDashboard, MetricsDashboardQuery } from '../../../types/metrics'
import type { RunHistoryPage, RunHistoryQuery } from '../../../types/run_history'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { PipelineMetricsView } = await import('./PipelineMetricsView')
const { notifications } = await import('../../../lib/notifications')
const { apiService } = await import('../../../services/api')
const { wsClient } = await import('../../../services/websocket')
const view = () => within(document.body)
const restoreRequestSubmit = installRequestSubmitShim()
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  restoreRequestSubmit()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('saving a new retention re-reads run history without deleting the cached runs', async (context) => {
  const user = userEvent.setup({ document })
  const pipelineId = 'fluxcd__1__apps'
  const config: MetricsConfig = {
    pipeline_id: pipelineId,
    enabled: true,
    retention_days: 30,
    created_at: '2026-10-25T00:00:00Z',
    updated_at: '2026-10-25T00:00:00Z',
  }
  const emptyHistory: RunHistoryPage = {
    runs: [],
    total_count: 0,
    has_more: false,
    can_scan_deeper: false,
    is_complete: true,
    scan_notice: null,
    sort: { column: 'run_number', direction: 'desc' },
    page: 1,
    page_size: 50,
    total_pages: 0,
    branches: [],
    actors: [],
  }
  const historyReads: number[] = []
  const requests: string[] = []
  context.mock.method(apiService, 'getPipelineMetricsConfig', async () => ({ ...config }))
  context.mock.method(
    apiService,
    'updatePipelineMetricsConfig',
    async (_id: string, enabled: boolean, days: number) => {
      config.enabled = enabled
      config.retention_days = days
    }
  )
  context.mock.method(apiService, 'getMetricsDashboard', async () =>
    dashboardFixture({ metric_type: 'run_duration', aggregation_period: 'hourly', total_count: 0, metrics: [] })
  )
  context.mock.method(apiService, 'getMetricsStorageStats', async () => ({
    total_metrics_count: 0,
    estimated_size_bytes: 0,
    estimated_size_mb: 0,
    last_cleanup_at: null,
    by_pipeline: [],
  }))
  context.mock.method(apiService, 'fetchRunHistory', async (_id: string, query: RunHistoryQuery) => {
    historyReads.push(query.page)

    return emptyHistory
  })
  context.mock.method(globalThis, 'fetch', async (url: string, init?: RequestInit) => {
    requests.push(`${init?.method ?? 'GET'} ${url}`)

    return new Response(null, { status: 204 })
  })
  context.mock.method(wsClient, 'listen', async () => () => undefined)
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(MemoryRouter, null, createElement(PipelineMetricsView, { pipelineId, pipelineName: 'apps' }))
        )
      )
    )
  })

  await user.click(await view().findByText('Configure metrics'))
  await user.selectOptions(await view().findByRole('combobox', { name: 'Retention period' }), '14')
  historyReads.length = 0
  await user.click(view().getByRole('button', { name: 'Save' }))

  await waitFor(() => assert.deepEqual(historyReads, [1]))
  assert.equal(config.retention_days, 14)
  assert.deepEqual(requests, [])
})

test('a failed history fetch after saving reports it and still refreshes the metrics', async (context) => {
  const user = userEvent.setup({ document })
  const pipelineId = 'fluxcd__1__apps'
  const config: MetricsConfig = {
    pipeline_id: pipelineId,
    enabled: true,
    retention_days: 30,
    created_at: '2026-10-25T00:00:00Z',
    updated_at: '2026-10-25T00:00:00Z',
  }
  const emptyHistory: RunHistoryPage = {
    runs: [],
    total_count: 0,
    has_more: false,
    can_scan_deeper: false,
    is_complete: true,
    scan_notice: null,
    sort: { column: 'run_number', direction: 'desc' },
    page: 1,
    page_size: 50,
    total_pages: 0,
    branches: [],
    actors: [],
  }
  let saved = false
  let dashboardReads = 0
  context.mock.method(apiService, 'getPipelineMetricsConfig', async () => ({ ...config }))
  context.mock.method(
    apiService,
    'updatePipelineMetricsConfig',
    async (_id: string, enabled: boolean, days: number) => {
      saved = true
      config.enabled = enabled
      config.retention_days = days
    }
  )
  context.mock.method(apiService, 'getMetricsDashboard', async () => {
    dashboardReads += 1

    return dashboardFixture({ metric_type: 'run_duration', aggregation_period: 'hourly', total_count: 0, metrics: [] })
  })
  context.mock.method(apiService, 'getMetricsStorageStats', async () => ({
    total_metrics_count: 0,
    estimated_size_bytes: 0,
    estimated_size_mb: 0,
    last_cleanup_at: null,
    by_pipeline: [],
  }))
  context.mock.method(apiService, 'fetchRunHistory', async () => {
    if (saved) {
      throw new Error('Provider is unreachable')
    }

    return emptyHistory
  })
  context.mock.method(globalThis, 'fetch', async () => new Response(null, { status: 204 }))
  context.mock.method(wsClient, 'listen', async () => () => undefined)
  const shown = context.mock.method(notifications, 'show')
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(MemoryRouter, null, createElement(PipelineMetricsView, { pipelineId, pipelineName: 'apps' }))
        )
      )
    )
  })

  await user.click(await view().findByText('Configure metrics'))
  await user.selectOptions(await view().findByRole('combobox', { name: 'Retention period' }), '14')
  await waitFor(() => assert.ok(dashboardReads > 0))
  dashboardReads = 0
  await user.click(view().getByRole('button', { name: 'Save' }))

  await waitFor(() =>
    assert.ok(shown.mock.calls.some((call) => call.arguments[0]?.title === "Couldn't refresh metrics"))
  )
  await waitFor(() => assert.ok(dashboardReads > 0))
})

test('refreshing the mounted metrics page advances both time bounds without creating new cache entries', async (context) => {
  context.mock.timers.enable({ apis: ['Date'], now: new Date('2026-11-01T12:00:00Z').getTime() })
  const pipelineId = 'github__1__build'
  const listeners = new Map<string, (payload: unknown) => void>()
  let observed = 1
  const observations = [
    { timestamp: '2026-10-31T12:05:00Z', value: 120 },
    { timestamp: '2026-11-01T12:05:00Z', value: 60 },
  ]
  context.mock.method(wsClient, 'listen', async (name: string, handler: (payload: unknown) => void) => {
    listeners.set(name, handler)

    return () => listeners.delete(name)
  })
  context.mock.method(apiService, 'getPipelineMetricsConfig', async () => ({
    pipeline_id: pipelineId,
    enabled: true,
    retention_days: 7,
    created_at: '',
    updated_at: '',
  }))
  context.mock.method(apiService, 'fetchRunHistory', async () => ({
    runs: [],
    total_count: 0,
    has_more: false,
    can_scan_deeper: false,
    is_complete: true,
    page: 1,
    page_size: 50,
    total_pages: 0,
    branches: [],
    actors: [],
  }))
  context.mock.method(apiService, 'getMetricsDashboard', async (query: MetricsDashboardQuery) => {
    const metrics = observations.slice(0, observed).map(({ timestamp, value }) => ({
      timestamp,
      value,
      count: 1,
      min: value,
      max: value,
      avg: value,
    }))
    const latest = metrics.at(-1)?.value ?? null

    return dashboardFixture(
      {
        metric_type: query.metric_type,
        aggregation_period: query.aggregation_period,
        total_count: metrics.length,
        metrics,
      },
      { stats: { average: latest, latest, max: latest, observed: metrics.length, total: metrics.length } }
    )
  })
  notifyManager.setNotifyFunction((callback) => act(callback))
  render(
    createElement(
      MantineProvider,
      { env: 'test' },
      createElement(
        QueryClientProvider,
        { client },
        createElement(MemoryRouter, null, createElement(PipelineMetricsView, { pipelineId, pipelineName: 'Build' }))
      )
    )
  )

  await view().findByRole('button', { name: /Expand .*duration.*Latest 2m, average 2m/ })
  await waitFor(() => assert.ok(listeners.has('metrics-generated')))
  const hashes = client
    .getQueryCache()
    .findAll({ predicate: (query) => query.queryKey.includes('aggregated') })
    .map((query) => query.queryHash)
  await act(async () => {
    context.mock.timers.tick(10 * 60_000)
    observed = 2
    listeners.get('metrics-generated')?.(pipelineId)
  })

  await view().findByRole('button', { name: /Expand .*duration.*Latest 1m, average 1m/ })
  assert.deepEqual(
    client
      .getQueryCache()
      .findAll({ predicate: (query) => query.queryKey.includes('aggregated') })
      .map((query) => query.queryHash),
    hashes
  )
})

const PIPELINE_ID = 'github__1__build'

const loadedDashboard = async (query: MetricsDashboardQuery) =>
  dashboardFixture(
    {
      metric_type: query.metric_type,
      aggregation_period: query.aggregation_period,
      total_count: 1,
      metrics: [{ timestamp: '2026-11-01T12:05:00Z', value: 120, count: 1, min: 120, max: 120, avg: 120 }],
    },
    { stats: { average: 120, latest: 120, max: 120, observed: 1, total: 1 } }
  )

const enabledConfig = async (): Promise<MetricsConfig> => ({
  pipeline_id: PIPELINE_ID,
  enabled: true,
  retention_days: 7,
  created_at: '',
  updated_at: '',
})

function mountMetrics(
  context: TestContext,
  dashboard: (query: MetricsDashboardQuery) => Promise<MetricsDashboard>,
  config: () => Promise<MetricsConfig>
) {
  context.mock.method(wsClient, 'listen', async () => () => undefined)
  context.mock.method(apiService, 'getPipelineMetricsConfig', config)
  context.mock.method(apiService, 'getMetricsDashboard', dashboard)
  context.mock.method(apiService, 'fetchRunHistory', async () => ({
    runs: [],
    total_count: 0,
    has_more: false,
    can_scan_deeper: false,
    is_complete: true,
    page: 1,
    page_size: 50,
    total_pages: 0,
    branches: [],
    actors: [],
  }))
  notifyManager.setNotifyFunction((callback) => act(callback))
  render(
    createElement(
      MantineProvider,
      { env: 'test' },
      createElement(
        QueryClientProvider,
        { client },
        createElement(
          MemoryRouter,
          null,
          createElement(PipelineMetricsView, { pipelineId: PIPELINE_ID, pipelineName: 'Build' })
        )
      )
    )
  )
}

test('one failed metric card shows its own error while the others keep their data', async (context) => {
  const user = userEvent.setup({ document })
  let successRateFails = true
  mountMetrics(
    context,
    async (query) => {
      if (query.metric_type === 'success_rate' && successRateFails) {
        throw new Error('success rate is unavailable')
      }

      return loadedDashboard(query)
    },
    enabledConfig
  )

  await view().findByText(/Couldn't load success rate/)
  assert.ok(await view().findByRole('button', { name: /Expand .*run duration/ }))
  assert.ok(view().getByRole('button', { name: /Expand .*runs per hour/ }))
  assert.equal(view().queryByText(/Couldn't load pipeline metrics/), null)

  successRateFails = false
  await user.click(view().getByRole('button', { name: 'Retry' }))
  assert.ok(await view().findByRole('button', { name: /Expand .*success rate/ }))
  assert.equal(view().queryByText(/Couldn't load success rate/), null)
})

test('a failed settings refetch keeps the loaded cards and offers Retry', async (context) => {
  const user = userEvent.setup({ document })
  let settingsFail = false
  mountMetrics(context, loadedDashboard, async () => {
    if (settingsFail) {
      throw new Error('settings are unavailable')
    }

    return enabledConfig()
  })
  await view().findByRole('button', { name: /Expand .*run duration/ })

  settingsFail = true
  await act(async () => client.invalidateQueries())

  await view().findByText(/Couldn't load metrics settings/)
  assert.ok(view().getByRole('button', { name: /Expand .*run duration/ }))

  settingsFail = false
  await user.click(view().getByRole('button', { name: 'Retry' }))
  await waitFor(() => assert.ok(!view().queryByText(/Couldn't load metrics settings/)))
  assert.ok(view().getByRole('button', { name: /Expand .*run duration/ }))
})
