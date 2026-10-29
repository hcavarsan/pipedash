import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { GeneralConfig, StorageConfigResponse } from '../../../types/domain'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { GeneralSection } = await import('./GeneralSection')
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

test('a failed proxy save keeps the typed address and a retry saves it trimmed', async (context) => {
  const user = userEvent.setup({ document })
  const config = storageConfig()
  const writes: GeneralConfig[] = []
  let fail = true
  context.mock.method(apiService, 'getStorageConfig', async () => config)
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes.push(general)
    if (fail) {
      throw new Error('Backend unavailable')
    }
    config.config.general = general
  })
  await renderGeneral()

  const proxy = await view().findByRole('textbox', { name: 'Proxy URL' })
  assert.ok(proxy instanceof HTMLInputElement)
  await user.type(proxy, 'http://proxy.example.com:3128')
  await user.click(view().getByRole('button', { name: 'Save proxy' }))

  await waitFor(() => assert.equal(writes.length, 1))
  await waitFor(() => assert.equal(proxy.disabled, false))
  assert.equal(proxy.value, 'http://proxy.example.com:3128')

  fail = false
  await user.click(view().getByRole('button', { name: 'Save proxy' }))
  await waitFor(() => assert.equal(writes.length, 2))
  assert.equal(writes[1]?.proxy_url, 'http://proxy.example.com:3128')
  await waitFor(() => assert.equal(proxy.value, 'http://proxy.example.com:3128'))
})

test('the proxy field refuses credentials in the address', async (context) => {
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
  await user.type(proxy, 'http://user:secret@proxy.example.com:3128')

  await waitFor(() => assert.equal(proxy.getAttribute('aria-invalid'), 'true'))
  const save = view().getByRole('button', { name: 'Save proxy' })
  assert.ok(save instanceof HTMLButtonElement)
  assert.equal(save.disabled, true)
  assert.deepEqual(writes, [])
})

test('a failed status page save returns the switch to the confirmed value and can be retried', async (context) => {
  const user = userEvent.setup({ document })
  const config = storageConfig()
  let writes = 0
  let fail = true
  context.mock.method(apiService, 'getStorageConfig', async () => config)
  context.mock.method(apiService, 'updateGeneralSettings', async (general: GeneralConfig) => {
    writes += 1
    if (fail) {
      throw new Error('Backend unavailable')
    }
    config.config.general = general
  })
  await renderGeneral()

  const toggle = await view().findByRole('switch', { name: /check provider status pages/i })
  assert.ok(toggle instanceof HTMLInputElement)
  await user.click(toggle)
  await waitFor(() => assert.equal(writes, 1))
  await waitFor(() => {
    assert.equal(toggle.disabled, false)
    assert.equal(toggle.checked, false)
  })

  fail = false
  await user.click(toggle)
  await waitFor(() => assert.equal(toggle.checked, true))
  assert.equal(writes, 2)
})
