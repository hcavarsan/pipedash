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
