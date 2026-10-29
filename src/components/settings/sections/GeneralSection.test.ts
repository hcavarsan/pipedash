import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { installRequestSubmitShim } from '../../../testing/requestSubmitShim'
import type { GeneralConfig, StorageConfigResponse } from '../../../types'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { GeneralSection } = await import('./GeneralSection')
const { apiService } = await import('../../../services')
const view = () => within(document.body)
const restoreRequestSubmit = installRequestSubmitShim()
let client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
})

after(async () => {
  restoreRequestSubmit()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

const storageConfig = (): StorageConfigResponse => ({
  config: {
    general: { metrics_enabled: true, default_refresh_interval: 30, status_pages_enabled: false, proxy_url: '' },
    server: { bind_addr: '127.0.0.1:8080', cors_allow_all: false },
    storage: { data_dir: '/tmp/pipedash-test', backend: 'sqlite' },
  },
  summary: '',
})

const renderGeneral = async (): Promise<void> => {
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, createElement(GeneralSection)))
      )
    )
  })
}

test('phone settings can recover a failed load and block overlapping saves', async (context) => {
  const user = userEvent.setup({ document })
  const config = storageConfig()
  let failRead = true
  let writes = 0
  const pending = Promise.withResolvers<void>()
  context.mock.method(apiService, 'getStorageConfig', async () => {
    if (failRead) {
      throw new Error('Settings unavailable')
    }
    return config
  })
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes += 1
    await pending.promise
    config.config.general = general
  })
  await renderGeneral()

  assert.ok(await view().findByText("Couldn't load provider status page setting"))
  failRead = false
  await user.click(view().getByRole('button', { name: /retry|try again/i }))
  const toggle = await view().findByRole('switch', { name: 'Check provider status pages' })
  assert.ok(toggle instanceof HTMLInputElement)
  assert.equal(toggle.checked, false)
  await user.click(toggle)
  await waitFor(() => {
    assert.equal(writes, 1)
    assert.equal(toggle.disabled, true)
    assert.equal(toggle.checked, true)
  })
  await user.click(toggle)
  assert.equal(writes, 1)
  await act(async () => pending.resolve())
  await waitFor(() => {
    assert.equal(toggle.disabled, false)
    assert.equal(toggle.checked, true)
    assert.deepEqual(config.config.general, {
      metrics_enabled: true,
      default_refresh_interval: 30,
      status_pages_enabled: true,
      proxy_url: '',
    })
  })
})

test('phone proxy rejects malformed and credential urls without saving', async (context) => {
  const user = userEvent.setup({ document })
  const config = storageConfig()
  const writes: GeneralConfig[] = []
  context.mock.method(apiService, 'getStorageConfig', async () => config)
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes.push(general)
  })
  await renderGeneral()

  const proxy = await view().findByRole('textbox', { name: 'Proxy URL' })
  assert.ok(proxy instanceof HTMLInputElement)
  const save = view().getByRole('button', { name: 'Save proxy' })

  for (const value of ['https://', 'http://user:secret@proxy.example.com:3128']) {
    await user.clear(proxy)
    await user.type(proxy, value)
    await waitFor(() => assert.equal(proxy.getAttribute('aria-invalid'), 'true'))
    await user.click(save)
    assert.deepEqual(writes, [])
  }
})

test('phone proxy saves the trimmed address and keeps unrelated settings', async (context) => {
  const user = userEvent.setup({ document })
  const config = storageConfig()
  const writes: GeneralConfig[] = []
  context.mock.method(apiService, 'getStorageConfig', async () => config)
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes.push(general)
    config.config.general = general
  })
  await renderGeneral()

  const proxy = await view().findByRole('textbox', { name: 'Proxy URL' })
  await user.type(proxy, '  http://proxy.example.com:3128  ')
  await user.click(view().getByRole('button', { name: 'Save proxy' }))

  await waitFor(() =>
    assert.deepEqual(writes, [
      {
        metrics_enabled: true,
        default_refresh_interval: 30,
        status_pages_enabled: false,
        proxy_url: 'http://proxy.example.com:3128',
      },
    ])
  )
})
