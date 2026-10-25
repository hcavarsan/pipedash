import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

let config = { enabled: true, default_retention_days: 7, updated_at: '2026-10-25T00:00:00Z' }
let reads = 0
let saveArgs: Record<string, unknown> | undefined
const { promise: pendingSave, resolve: finishSave } = Promise.withResolvers<void>()

Object.assign(window, {
  __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' },
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case 'get_global_metrics_config':
          reads += 1
          return config
        case 'update_global_metrics_config':
          saveArgs = args
          await pendingSave
          config = {
            ...config,
            enabled: Boolean(args?.enabled),
            default_retention_days: Number(args?.defaultRetentionDays),
          }
          return
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
  },
})

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('@mantine/notifications')
const { MetricsSection } = await import('./MetricsSection')
const { queryKeys } = await import('../../../lib/queryKeys')
const { apiService, tauriService } = await import('../../../services')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifications.clean()
  notifyManager.setNotifyFunction((callback) => callback())
  GlobalRegistrator.unregister()
})

test('saving metrics retention preserves a newer draft when the saved config refetches', async (context) => {
  context.mock.method(apiService, 'getGlobalMetricsConfig', tauriService.getGlobalMetricsConfig)
  context.mock.method(apiService, 'updateGlobalMetricsConfig', tauriService.updateGlobalMetricsConfig)
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(MetricsSection))
      )
    )
  })

  fireEvent.click(await view().findByRole('combobox', { name: 'Retention period' }))
  fireEvent.click(await view().findByText('Custom'))
  const days = view().getByRole('textbox', { name: 'Custom days' }) as HTMLInputElement

  fireEvent.change(days, { target: { value: '30' } })
  fireEvent.blur(days)
  await waitFor(() => assert.deepEqual(saveArgs, { enabled: true, defaultRetentionDays: 30 }))
  fireEvent.change(days, { target: { value: '45' } })
  assert.equal(days.value, '45')

  await act(async () => finishSave())
  await waitFor(() => {
    assert.equal(reads, 2)
    assert.equal(client.isFetching({ queryKey: queryKeys.metrics.globalConfig() }), 0)
    assert.equal(client.getQueryData<typeof config>(queryKeys.metrics.globalConfig())?.default_retention_days, 30)
    assert.equal((view().getByRole('textbox', { name: 'Custom days' }) as HTMLInputElement).value, '45')
  })
})
