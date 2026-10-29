import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

import { isRecord } from '../../utils/isRecord'

if (process.env.PIPEDASH_EXPORT_TEST !== '1') {
  test('chart exports wait for the native destination picker in an isolated runtime', () => {
    const result = spawnSync(process.execPath, ['test', import.meta.filename], {
      env: { ...process.env, PIPEDASH_EXPORT_TEST: '1' },
      encoding: 'utf8',
      timeout: 30000,
    })
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`)
  })
} else {
  GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })
  Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'ios' } })

  type SavedFile = { path: string; shared: boolean }

  const METRICS = {
    metric_type: 'run_duration',
    aggregation_period: 'hourly',
    total_count: 1,
    metrics: [{ timestamp: '2026-10-25T12:00:00Z', value: 42, count: 1, min: 42, max: 42, avg: 42 }],
  }

  let metricsResponse: () => Promise<unknown> = async () => METRICS
  let pending = Promise.withResolvers<SavedFile>()
  let saveResponse: () => Promise<SavedFile> = () => pending.promise
  const exports: Array<{ fileName: string; contents: number[] }> = []

  mockIPC((command, args) => {
    if (command === 'query_aggregated_metrics') {
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
  const { iosTheme } = await import('../../theme/ios')
  const { MetricsDetailPage } = await import('./MetricsDetailPage')
  const view = () => within(document.body)

  const renderMetricsDetail = () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })

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
              retentionDays: 30,
            })
          )
        )
      )
    )

    return client
  }

  beforeEach(() => {
    metricsResponse = async () => METRICS
    pending = Promise.withResolvers<SavedFile>()
    saveResponse = () => pending.promise
    exports.length = 0
    notifyManager.setNotifyFunction((callback) => act(callback))
  })

  afterEach(async () => {
    await act(async () => cleanup())
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
    metricsResponse = async () => METRICS
    await user.click(view().getByRole('button', { name: 'Retry' }))
    await waitFor(() =>
      assert.equal(view().getByRole('button', { name: 'Export metrics' }).hasAttribute('disabled'), false)
    )
  })

  test('an empty metrics response explains that no runs were recorded', async () => {
    metricsResponse = async () => ({ ...METRICS, total_count: 0, metrics: [] })
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
}
