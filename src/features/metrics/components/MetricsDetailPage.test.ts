import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

import { isRecord } from '../../../utils/isRecord'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })
Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })

type SavedFile = { path: string; shared: boolean }

const METRICS = {
  metric_type: 'run_duration',
  aggregation_period: 'hourly',
  total_count: 1,
  metrics: [{ timestamp: '2026-10-25T12:00:00Z', value: 42, count: 1, min: 42, max: 42, avg: 42 }],
}

const dashboard = (data: typeof METRICS) => ({
  data,
  observed: data.metrics.map(() => true),
  stats: { average: 42, latest: 42, max: 42, observed: data.metrics.length, total: data.metrics.length },
  table_indices: data.metrics.map((_, index) => index),
  ranges: [],
  widest_range: '24h',
  aggregation_type: 'avg',
})

let metricsResponse: () => Promise<unknown> = async () => dashboard(METRICS)
let pending = Promise.withResolvers<SavedFile>()
let saveResponse: () => Promise<SavedFile> = () => pending.promise
const exports: Array<{ fileName: string; contents: number[] }> = []

mockIPC((command, args) => {
  if (command === 'get_metrics_dashboard') {
    return metricsResponse()
  }
  assert.equal(command, 'save_export_file')
  assert.ok(isRecord(args) && typeof args.fileName === 'string' && Array.isArray(args.contents))
  exports.push({ fileName: args.fileName, contents: args.contents.filter((n): n is number => typeof n === 'number') })

  return saveResponse()
})

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifications } = await import('@mantine/notifications')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { iosTheme } = await import('../../../theme/ios')
const { MetricsDetailPage } = await import('./MetricsDetailPage')
const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const view = () => within(document.body)

const renderMetricsDetail = () => {
  render(
    createElement(
      MantineProvider,
      { theme: iosTheme, env: 'test' },
      createElement(
        QueryClientProvider,
        { client },
        createElement(
          MemoryRouter,
          null,
          createElement(MetricsDetailPage, {
            pipelineId: 'test',
            pipelineName: 'Build',
            metricType: 'run_duration',
            initialDateRange: '24h',
            initialAggregation: 'hourly',
          })
        )
      )
    )
  )

  return client
}

beforeEach(() => {
  metricsResponse = async () => dashboard(METRICS)
  pending = Promise.withResolvers<SavedFile>()
  saveResponse = () => pending.promise
  exports.length = 0
  notifyManager.setNotifyFunction((callback) => act(callback))
})

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
})

after(async () => {
  await GlobalRegistrator.unregister()
})

test('cancel stays silent, duplicate exports stay blocked, and success follows picker completion', async (context) => {
  const notices = context.mock.method(notifications, 'show', () => 'export-notification')
  renderMetricsDetail()
  const user = userEvent.setup({ document })
  const button = view().getByRole('button', { name: 'Export metrics' })
  assert.ok(button instanceof HTMLButtonElement)
  await waitFor(() => assert.equal(button.disabled, false))
  await user.click(button)
  await user.click(await view().findByRole('menuitem', { name: 'Export as CSV' }))
  await waitFor(() => assert.equal(exports.length, 1))
  assert.equal(button.disabled, true)
  assert.equal(button.getAttribute('aria-busy'), 'true')
  const [exported] = exports
  assert.ok(exported)
  assert.equal(exported.fileName, 'Build_run_duration_24h.csv')
  assert.match(new TextDecoder().decode(new Uint8Array(exported.contents)), /"Value \(seconds\)"/)
  await user.click(button)
  assert.equal(exports.length, 1)
  assert.equal(notices.mock.callCount(), 0)
  await act(async () => pending.resolve({ path: '', shared: false }))
  await waitFor(() => assert.equal(button.disabled, false))
  assert.equal(notices.mock.callCount(), 0)

  pending = Promise.withResolvers()
  await user.click(button)
  await user.click(await view().findByRole('menuitem', { name: 'Export as CSV' }))
  await waitFor(() => assert.equal(exports.length, 2))
  assert.equal(notices.mock.callCount(), 0)
  await act(async () => pending.resolve({ path: '', shared: true }))
  await waitFor(() => {
    assert.equal(button.disabled, false)
    assert.equal(notices.mock.callCount(), 1)
  })
})

test('a pending metrics request shows a loading state and keeps export unavailable', async () => {
  metricsResponse = () => new Promise(() => undefined)
  renderMetricsDetail()

  assert.ok(await view().findByText(/Loading run duration metrics/))
  assert.equal(view().getByRole('button', { name: 'Export metrics' }).hasAttribute('disabled'), true)
})

test('a rejected metrics request shows the error and recovers on retry', async () => {
  const user = userEvent.setup({ document })
  metricsResponse = async () => {
    throw new Error('metrics backend down')
  }
  renderMetricsDetail()

  assert.ok(await view().findByText(/Couldn't load run duration metrics/))
  metricsResponse = async () => dashboard(METRICS)
  await user.click(view().getByRole('button', { name: 'Retry' }))
  await waitFor(() =>
    assert.equal(view().getByRole('button', { name: 'Export metrics' }).hasAttribute('disabled'), false)
  )
})

test('an empty metrics response explains that no runs were recorded', async () => {
  metricsResponse = async () => dashboard({ ...METRICS, total_count: 0, metrics: [] })
  renderMetricsDetail()

  assert.ok(await view().findByText('No runs in this time range'))
  assert.equal(view().getByRole('button', { name: 'Export metrics' }).hasAttribute('disabled'), true)
})

test('a failed export reports the error and a retry can succeed', async (context) => {
  const user = userEvent.setup({ document })
  const notices = context.mock.method(notifications, 'show', () => 'export-notification')
  const noticeColor = (index: number) => {
    const options = notices.mock.calls[index]?.arguments[0]

    return isRecord(options) ? options.color : undefined
  }
  saveResponse = async () => {
    throw new Error('disk full')
  }
  renderMetricsDetail()
  const button = view().getByRole('button', { name: 'Export metrics' })
  await waitFor(() => assert.equal(button.hasAttribute('disabled'), false))

  await user.click(button)
  await user.click(await view().findByRole('menuitem', { name: 'Export as CSV' }))
  await waitFor(() => assert.equal(notices.mock.callCount(), 1))
  assert.equal(noticeColor(0), 'red')
  await waitFor(() => assert.equal(button.hasAttribute('disabled'), false))

  saveResponse = async () => ({ path: '/tmp/x.csv', shared: true })
  await user.click(button)
  await user.click(await view().findByRole('menuitem', { name: 'Export as CSV' }))
  await waitFor(() => assert.equal(notices.mock.callCount(), 2))
  assert.equal(noticeColor(1), 'green')
})

test('a range change keeps the old title while the new range loads, then shows the new one', async () => {
  const user = userEvent.setup({ document })
  renderMetricsDetail()
  assert.ok(await view().findByText(/per hour · Last 24 hours/))

  const gate = Promise.withResolvers<unknown>()
  metricsResponse = () => gate.promise
  await user.selectOptions(view().getByRole('combobox', { name: 'Time range' }), '7d')

  assert.ok(await view().findByText('Updating metrics…'))
  assert.ok(view().getByText(/per hour · Last 24 hours/))
  assert.equal(view().queryByText(/per hour · Last 7 days/), null)

  await act(async () => gate.resolve(dashboard(METRICS)))
  await waitFor(() => assert.ok(view().getByText(/per hour · Last 7 days/)))
  assert.equal(view().queryByText('Updating metrics…'), null)
  assert.equal(view().queryByText(/per hour · Last 24 hours/), null)
})

test('a range change after an empty result shows updating, not the empty state', async () => {
  const user = userEvent.setup({ document })
  metricsResponse = async () => dashboard({ ...METRICS, total_count: 0, metrics: [] })
  renderMetricsDetail()
  assert.ok(await view().findByText('No runs in this time range'))

  const gate = Promise.withResolvers<unknown>()
  metricsResponse = () => gate.promise
  await user.selectOptions(view().getByRole('combobox', { name: 'Time range' }), '7d')

  assert.ok(await view().findByText('Updating metrics…'))
  assert.equal(view().queryByText('No runs in this time range'), null)
  await act(async () => gate.resolve(dashboard(METRICS)))
  await waitFor(() => assert.ok(!view().queryByText('Updating metrics…')))
})

test('a failed refetch keeps the loaded chart and offers Retry', async () => {
  const user = userEvent.setup({ document })
  const queryClient = renderMetricsDetail()
  assert.ok(await view().findByText(/per hour · Last 24 hours/))

  metricsResponse = async () => {
    throw new Error('metrics backend down')
  }
  await act(async () => queryClient.invalidateQueries())

  assert.ok(await view().findByText(/Couldn't load run duration metrics/))
  assert.ok(view().getByText(/per hour · Last 24 hours/))

  metricsResponse = async () => dashboard(METRICS)
  await user.click(view().getByRole('button', { name: 'Retry' }))
  await waitFor(() => assert.ok(!view().queryByText(/Couldn't load run duration metrics/)))
})
