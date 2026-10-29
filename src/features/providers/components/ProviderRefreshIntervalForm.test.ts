import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

GlobalRegistrator.register({ width: 393, height: 852, settings: { navigator: { userAgent: 'iPhone' } } })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { default: userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { ProvidersSection } = await import('./ProvidersSection')
const { apiService } = await import('../../../services/api')
const { providerSummaryFixture } = await import('../../../testing/fixtures')
const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  await GlobalRegistrator.unregister()
})

test('mobile refresh interval rejects empty and zero drafts and saves a valid value', async (context) => {
  const user = userEvent.setup({ document })
  const saves: Array<[number, number]> = []
  context.mock.method(apiService, 'listProviders', async () => [providerSummaryFixture({ id: 7, name: 'Acme CI' })])
  context.mock.method(apiService, 'getAvailablePlugins', async () => [])
  context.mock.method(apiService, 'getStorageConfig', async () => {
    throw new Error('not needed')
  })
  context.mock.method(apiService, 'updateProviderRefreshInterval', async (id: number, interval: number) => {
    saves.push([id, interval])
  })
  notifyManager.setNotifyFunction((callback) => act(callback))
  await act(async () => {
    render(
      createElement(
        MantineProvider,
        null,
        createElement(
          QueryClientProvider,
          { client },
          createElement(MemoryRouter, null, createElement(ProvidersSection))
        )
      )
    )
  })

  await user.click(await view().findByRole('button', { name: 'Edit refresh interval for Acme CI' }))
  const name = 'Refresh interval in seconds for Acme CI'
  const enabledField = async (): Promise<HTMLElement> => {
    const found = await view().findByRole('textbox', { name })
    await waitFor(() => assert.equal(found.hasAttribute('disabled'), false))

    return found
  }
  const field = await enabledField()

  await user.clear(field)
  await user.click(view().getByRole('button', { name: 'Save' }))
  assert.deepEqual(saves, [])
  assert.ok((await view().findAllByText(/whole number from 5 to 300/i)).length > 0)

  await user.type(await enabledField(), '0')
  await user.click(view().getByRole('button', { name: 'Save' }))
  assert.deepEqual(saves, [])
  assert.ok(view().getAllByText(/whole number from 5 to 300/i).length > 0)

  const again = await enabledField()
  await user.clear(again)
  await user.type(again, '60')
  await user.click(view().getByRole('button', { name: 'Save' }))
  await waitFor(() => assert.deepEqual(saves, [[7, 60]]))
})
