import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { BrowsePage } = await import('./BrowsePage')
const { ProvidersSection } = await import('./ProvidersSection')
const { apiService } = await import('../../../services/api')
const { queryKeys } = await import('../../../lib/queryKeys')
const { providerSummaryFixture } = await import('../../../testing/fixtures')

const view = () => within(document.body)
let client = new QueryClient()

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})

after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

async function mount(screen: ReactNode): Promise<void> {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, screen))
      )
    )
  })
}

function failProvidersAfterFirstRead(context: TestContext): void {
  let reads = 0
  context.mock.method(apiService, 'listProviders', async () => {
    reads += 1
    if (reads > 1) {
      throw new Error('providers offline')
    }

    return [providerSummaryFixture({ id: 7, name: 'Acme CI' })]
  })
  context.mock.method(apiService, 'getAvailablePlugins', async () => [])
  context.mock.method(apiService, 'getStorageConfig', async () => {
    throw new Error('not needed')
  })
}

async function refetchProviders(): Promise<void> {
  await act(async () => {
    await client.invalidateQueries({ queryKey: queryKeys.providers.list() })
  })
}

test('settings keep the loaded provider cards when a later providers read fails', async (context) => {
  failProvidersAfterFirstRead(context)
  context.mock.method(apiService, 'listPipelineViews', async () => [])
  await mount(createElement(ProvidersSection))
  assert.ok(await view().findByText('Acme CI'))

  await refetchProviders()

  await waitFor(() => assert.ok(view().getByText('providers offline')))
  assert.ok(view().getByText('Acme CI'))
  assert.ok(view().getByRole('button', { name: 'Retry' }))
})

test('browse keeps the loaded providers when a later read fails and reports a failed views read', async (context) => {
  failProvidersAfterFirstRead(context)
  context.mock.method(apiService, 'listPipelineViews', async () => {
    throw new Error('views offline')
  })
  await mount(createElement(BrowsePage))
  assert.ok(await view().findByText('Acme CI'))
  assert.ok(await view().findByText("Couldn't load views"))

  await refetchProviders()

  await waitFor(() => assert.ok(view().getByText('providers offline')))
  assert.ok(view().getByText('Acme CI'))
  assert.ok(view().getByText('views offline'))
})
