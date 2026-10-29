import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import type { GeneralConfig, StorageConfigResponse } from '../../../types'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { GeneralSection } = await import('./GeneralSection')
const { apiService } = await import('../../../services')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('phone settings can recover a failed load and block overlapping saves', async (context) => {
  const config: StorageConfigResponse = {
    config: {
      general: { metrics_enabled: true, default_refresh_interval: 30, status_pages_enabled: false, proxy_url: '' },
      server: { bind_addr: '127.0.0.1:8080', cors_allow_all: false },
      storage: { data_dir: '/tmp/pipedash-test', backend: 'sqlite' },
    },
    summary: '',
  }
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

  assert.ok(await view().findByText("Couldn't load provider status page setting"))
  failRead = false
  fireEvent.click(view().getByRole('button', { name: /retry|try again/i }))
  const toggle = await view().findByRole('switch', { name: 'Check provider status pages' })
  assert.ok(toggle instanceof HTMLInputElement)
  assert.equal(toggle.checked, false)
  fireEvent.click(toggle)
  await waitFor(() => {
    assert.equal(writes, 1)
    assert.equal(toggle.disabled, true)
    assert.equal(toggle.checked, true)
  })
  fireEvent.click(toggle)
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
