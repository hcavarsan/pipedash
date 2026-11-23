import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { GeneralConfig, StorageConfigResponse } from '../../../types/storage'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { QueueLimitInput } = await import('./QueueLimitInput')
const { apiService } = await import('../../../services/api')
const view = () => within(document.body)
let client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const storageConfig = (): StorageConfigResponse => ({
  config: {
    general: {
      metrics_enabled: false,
      default_refresh_interval: 60,
      status_pages_enabled: true,
      proxy_url: 'http://proxy.example.com:3128',
      queue_alert_minutes: 15,
    },
    server: { bind_addr: '127.0.0.1:8080', cors_allow_all: false },
    storage: { data_dir: '/tmp/pipedash-test', backend: 'sqlite' },
  },
  summary: '',
})

const renderInput = async (mobileShell: boolean): Promise<void> => {
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(MemoryRouter, null, createElement(QueueLimitInput, { mobileShell }))
        )
      )
    )
  })
}

for (const mobileShell of [false, true]) {
  test(`the queue limit saves on blur and keeps the other settings (mobile shell: ${mobileShell})`, async (context) => {
    const user = userEvent.setup({ document })
    const config = storageConfig()
    const writes: GeneralConfig[] = []
    context.mock.method(apiService, 'getStorageConfig', async () => config)
    context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
      writes.push(general)
      config.config.general = general
    })
    await renderInput(mobileShell)

    const input = await view().findByRole('textbox', { name: 'Queue limit (minutes)' })
    assert.ok(input instanceof HTMLInputElement)
    await waitFor(() => assert.equal(input.value, '15'))
    await user.clear(input)
    await user.type(input, '30')
    await user.tab()

    await waitFor(() => assert.deepEqual(writes, [{ ...storageConfig().config.general, queue_alert_minutes: 30 }]))
    await waitFor(() => assert.equal(input.value, '30'))
  })
}

test('a blank queue limit stays blank and shows an error instead of snapping back', async (context) => {
  const user = userEvent.setup({ document })
  const writes: GeneralConfig[] = []
  context.mock.method(apiService, 'getStorageConfig', async () => storageConfig())
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes.push(general)
  })
  await renderInput(false)

  const input = await view().findByRole('textbox', { name: 'Queue limit (minutes)' })
  assert.ok(input instanceof HTMLInputElement)
  await waitFor(() => assert.equal(input.value, '15'))
  await user.clear(input)
  await user.tab()

  assert.equal(input.value, '')
  assert.equal(input.getAttribute('aria-invalid'), 'true')
  assert.ok(view().getAllByText('Enter a number').length > 0)
  assert.deepEqual(writes, [])
})

test('a queue limit of 0 is caught before it is sent', async (context) => {
  const user = userEvent.setup({ document })
  const writes: GeneralConfig[] = []
  context.mock.method(apiService, 'getStorageConfig', async () => storageConfig())
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes.push(general)
  })
  await renderInput(true)

  const input = await view().findByRole('textbox', { name: 'Queue limit (minutes)' })
  assert.ok(input instanceof HTMLInputElement)
  await waitFor(() => assert.equal(input.value, '15'))
  await user.clear(input)
  await user.type(input, '0')
  await user.tab()

  assert.equal(input.value, '0')
  assert.equal(input.getAttribute('aria-invalid'), 'true')
  assert.ok(view().getAllByText('Must be at least 1').length > 0)
  assert.deepEqual(writes, [])
})

test('a queue limit the server rejects shows its message next to the field', async (context) => {
  const user = userEvent.setup({ document })
  context.mock.method(apiService, 'getStorageConfig', async () => storageConfig())
  context.mock.method(apiService, 'updateGeneralSettings', async () => {
    throw new Error('queue_alert_minutes must be between 1 and 1440 minutes')
  })
  await renderInput(false)

  const input = await view().findByRole('textbox', { name: 'Queue limit (minutes)' })
  assert.ok(input instanceof HTMLInputElement)
  await waitFor(() => assert.equal(input.value, '15'))
  await user.clear(input)
  await user.type(input, '2000')
  await user.tab()

  await waitFor(() => assert.equal(input.getAttribute('aria-invalid'), 'true'))
  assert.ok(view().getAllByText('queue_alert_minutes must be between 1 and 1440 minutes').length > 0)
  assert.equal(input.value, '15')
})

test('a failed load shows the error and a retry loads the saved limit', async (context) => {
  const user = userEvent.setup({ document })
  let fail = true
  context.mock.method(apiService, 'getStorageConfig', async () => {
    if (fail) {
      throw new Error('server unreachable')
    }
    return storageConfig()
  })
  await renderInput(false)

  await view().findByText("Couldn't load the queue limit")
  fail = false
  await user.click(view().getByRole('button', { name: /retry/i }))

  const input = await view().findByRole('textbox', { name: 'Queue limit (minutes)' })
  assert.ok(input instanceof HTMLInputElement)
  await waitFor(() => assert.equal(input.value, '15'))
})
