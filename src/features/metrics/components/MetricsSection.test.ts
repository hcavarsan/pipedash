import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import type { UserEvent } from '@testing-library/user-event'
import { createElement } from 'react'
import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'

import type { GlobalMetricsConfig } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('@mantine/notifications')
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

interface SaveCall {
  enabled: boolean
  retentionDays: number
}

interface Backend {
  config: GlobalMetricsConfig
  saves: SaveCall[]
  gates: PromiseWithResolvers<void>[]
  failNext: boolean
}

const mockBackend = (context: TestContext): Backend => {
  const backend: Backend = {
    config: { enabled: true, default_retention_days: 7, updated_at: '2026-10-25T00:00:00Z' },
    saves: [],
    gates: [],
    failNext: false,
  }
  context.mock.method(apiService, 'getGlobalMetricsConfig', async () => ({ ...backend.config }))
  context.mock.method(apiService, 'updateGlobalMetricsConfig', async (enabled: boolean, retentionDays: number) => {
    backend.saves.push({ enabled, retentionDays })
    const gate = backend.gates.shift()
    await gate?.promise
    if (backend.failNext) {
      backend.failNext = false
      throw new Error('Backend unavailable')
    }
    backend.config = { ...backend.config, enabled, default_retention_days: retentionDays }
  })

  return backend
}

const renderMetrics = async (): Promise<void> => {
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
}

const openCustomDays = async (user: UserEvent): Promise<HTMLInputElement> => {
  await user.click(await view().findByRole('combobox', { name: 'Retention period' }))
  await user.click(await view().findByRole('option', { name: /^custom$/i, hidden: true }))
  const days = await view().findByRole('textbox', { name: /custom days/i })
  assert.ok(days instanceof HTMLInputElement)

  return days
}

test('saving custom retention keeps a newer draft and the next submission sends it', async (context) => {
  const user = userEvent.setup({ document })
  const backend = mockBackend(context)
  const gate = Promise.withResolvers<void>()
  backend.gates.push(gate)
  await renderMetrics()
  const days = await openCustomDays(user)

  await user.clear(days)
  await user.type(days, '30')
  await user.tab()
  await waitFor(() => assert.deepEqual(backend.saves, [{ enabled: true, retentionDays: 30 }]))

  await user.clear(days)
  await user.type(days, '45')
  assert.equal(days.value, '45')

  await act(async () => gate.resolve())
  await waitFor(() => assert.equal(backend.config.default_retention_days, 30))
  assert.equal(days.value, '45')

  await user.tab()
  await user.click(days)
  await user.tab()
  await waitFor(() => assert.deepEqual(backend.saves.at(-1), { enabled: true, retentionDays: 45 }))
})

test('a failed retention save does not overwrite a newer draft', async (context) => {
  const user = userEvent.setup({ document })
  const backend = mockBackend(context)
  const gate = Promise.withResolvers<void>()
  backend.gates.push(gate)
  backend.failNext = true
  await renderMetrics()
  const days = await openCustomDays(user)

  await user.clear(days)
  await user.type(days, '30')
  await user.tab()
  await waitFor(() => assert.equal(backend.saves.length, 1))

  await user.clear(days)
  await user.type(days, '45')
  await act(async () => gate.resolve())

  await waitFor(() => assert.equal(backend.saves.length, 1))
  await waitFor(() => assert.equal(days.value, '45'))
  assert.equal(backend.config.default_retention_days, 7)
})

test('custom retention outside 1 to 90 is not saved', async (context) => {
  const user = userEvent.setup({ document })
  const backend = mockBackend(context)
  await renderMetrics()
  const days = await openCustomDays(user)

  for (const value of ['0', '91']) {
    await user.clear(days)
    await user.type(days, value)
    await user.tab()
    await waitFor(() => assert.equal(days.getAttribute('aria-invalid'), 'true'))
  }

  await user.clear(days)
  await user.tab()
  await waitFor(() => assert.equal(days.getAttribute('aria-invalid'), 'true'))
  assert.deepEqual(backend.saves, [])
})

test('turning metrics off shows the new value at once and restores it when saving fails', async (context) => {
  const user = userEvent.setup({ document })
  const backend = mockBackend(context)
  const gate = Promise.withResolvers<void>()
  backend.gates.push(gate)
  backend.failNext = true
  await renderMetrics()
  const toggle = await view().findByRole('switch', { name: 'Enable metrics by default' })
  assert.ok(toggle instanceof HTMLInputElement)
  assert.equal(toggle.checked, true)

  await user.click(toggle)
  await waitFor(() => assert.equal(toggle.checked, false))
  await act(async () => gate.resolve())

  await waitFor(() => assert.equal(toggle.checked, true))
  assert.deepEqual(backend.saves, [{ enabled: false, retentionDays: 7 }])
})
