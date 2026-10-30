import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import type { MetricsConfig, PaginatedRunHistory } from '../../../types/domain'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { PipelineMetricsView } = await import('./PipelineMetricsView')
const { apiService } = await import('../../../services/api')
const { events } = await import('../../../services/service')
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
  const emptyHistory: PaginatedRunHistory = {
    runs: [],
    total_count: 0,
    has_more: false,
    is_complete: true,
    page: 1,
    page_size: 50,
    total_pages: 0,
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
  context.mock.method(apiService, 'queryAggregatedMetrics', async () => ({
    metric_type: 'run_duration',
    aggregation_period: 'hourly',
    total_count: 0,
    metrics: [],
  }))
  context.mock.method(apiService, 'getMetricsStorageStats', async () => ({
    total_metrics_count: 0,
    estimated_size_bytes: 0,
    estimated_size_mb: 0,
    last_cleanup_at: null,
    by_pipeline: [],
  }))
  context.mock.method(apiService, 'fetchRunHistory', async (_id: string, page?: number) => {
    historyReads.push(page ?? 1)

    return emptyHistory
  })
  context.mock.method(globalThis, 'fetch', async (url: string, init?: RequestInit) => {
    requests.push(`${init?.method ?? 'GET'} ${url}`)

    return new Response(null, { status: 204 })
  })
  context.mock.method(events, 'listen', async () => () => undefined)
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
