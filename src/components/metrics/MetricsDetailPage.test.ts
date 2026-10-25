import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC } from '@tauri-apps/api/mocks'
import { createElement } from 'react'

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
  let pending = Promise.withResolvers<{ path: string; shared: boolean }>()
  const exports: Array<{ fileName: string; contents: number[] }> = []
  mockIPC((command, args) => {
    if (command === 'query_aggregated_metrics') {
      return {
        metric_type: 'run_duration',
        aggregation_period: 'hourly',
        total_count: 1,
        metrics: [{ timestamp: '2026-10-25T12:00:00Z', value: 42, count: 1, min: 42, max: 42, avg: 42 }],
      }
    }
    assert.equal(command, 'save_export_file')
    exports.push(args as { fileName: string; contents: number[] })
    return pending.promise
  })

  const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
  const { MantineProvider } = await import('@mantine/core')
  const { notifications } = await import('@mantine/notifications')
  const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
  const { MemoryRouter } = await import('react-router-dom')
  const { iosTheme } = await import('../../theme/ios')
  const { MetricsDetailPage } = await import('./MetricsDetailPage')
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const view = () => within(document.body)

  after(async () => {
    await act(async () => cleanup())
    client.clear()
    notifyManager.setNotifyFunction((callback) => callback())
    GlobalRegistrator.unregister()
  })

  test('cancel stays silent, duplicate exports stay blocked, and success follows picker completion', async (context) => {
    const notices = context.mock.method(notifications, 'show', () => 'export-notification')
    notifyManager.setNotifyFunction((callback) => act(callback))
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
    const button = view().getByRole('button', { name: 'Export metrics' }) as HTMLButtonElement
    await waitFor(() => assert.equal(button.disabled, false))
    fireEvent.click(button)
    fireEvent.click(await view().findByRole('menuitem', { name: 'Export as CSV' }))
    await waitFor(() => assert.equal(exports.length, 1))
    assert.equal(button.disabled, true)
    assert.equal(button.getAttribute('aria-busy'), 'true')
    assert.equal(exports[0].fileName, 'Build_run_duration_24h.csv')
    assert.match(new TextDecoder().decode(new Uint8Array(exports[0].contents)), /"Value \(seconds\)"/)
    fireEvent.click(button)
    assert.equal(exports.length, 1)
    assert.equal(notices.mock.callCount(), 0)
    await act(async () => pending.resolve({ path: '', shared: false }))
    await waitFor(() => assert.equal(button.disabled, false))
    assert.equal(notices.mock.callCount(), 0)

    pending = Promise.withResolvers()
    fireEvent.click(button)
    fireEvent.click(await view().findByRole('menuitem', { name: 'Export as CSV' }))
    await waitFor(() => assert.equal(exports.length, 2))
    assert.equal(notices.mock.callCount(), 0)
    await act(async () => pending.resolve({ path: '', shared: true }))
    await waitFor(() => {
      assert.equal(button.disabled, false)
      assert.equal(notices.mock.callCount(), 1)
    })
  })
}
