import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import type { GlobalMetricsConfig } from '../../../types/domain'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('@mantine/notifications')
const { MemoryRouter } = await import('react-router-dom')
const { MetricsSection } = await import('./MetricsSection')
const { apiService } = await import('../../../services/api')
const view = () => within(document.body)
const restoreRequestSubmit = installRequestSubmitShim()
let client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  notifications.clean()
  client.clear()
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
})

after(async () => {
  restoreRequestSubmit()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('phone custom retention rejects out-of-range values instead of clamping them', async (context) => {
  const user = userEvent.setup({ document })
  const config: GlobalMetricsConfig = { enabled: true, default_retention_days: 7, updated_at: '2026-10-25T00:00:00Z' }
  const saves: number[] = []
  context.mock.method(apiService, 'getGlobalMetricsConfig', async () => ({ ...config }))
  context.mock.method(apiService, 'updateGlobalMetricsConfig', async (_enabled: boolean, retentionDays: number) => {
    saves.push(retentionDays)
    config.default_retention_days = retentionDays
  })
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, createElement(MetricsSection)))
      )
    )
  })

  await user.selectOptions(await view().findByRole('combobox', { name: 'Retention period' }), 'custom')
  const days = await view().findByRole('textbox', { name: /custom days/i })
  assert.ok(days instanceof HTMLInputElement)

  for (const value of ['0', '91']) {
    await user.clear(days)
    await user.type(days, value)
    await user.tab()
    await waitFor(() => assert.equal(days.getAttribute('aria-invalid'), 'true'))
    assert.equal(days.value, value)
  }

  assert.deepEqual(saves, [])
})
