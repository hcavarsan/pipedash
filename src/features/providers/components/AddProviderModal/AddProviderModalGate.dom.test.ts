import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement } from 'react'

import { pluginFixture } from './pluginFixture'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../../services/api')
const { queryKeys } = await import('../../../../lib/queryKeys')
const { AddProviderModal } = await import('./AddProviderModal')

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

function mount(): void {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  notifyManager.setNotifyFunction((callback) => act(callback))
  render(
    createElement(
      MantineProvider,
      null,
      createElement(
        QueryClientProvider,
        { client },
        createElement(AddProviderModal, { opened: true, onClose: () => {} })
      )
    )
  )
}

test('a rejected plugin list shows an error, and Retry opens a usable wizard', async (context) => {
  const user = userEvent.setup({ document })
  let reads = 0
  context.mock.method(apiService, 'getAvailablePlugins', async () => {
    reads += 1
    if (reads === 1) {
      throw new Error('plugins offline')
    }

    return [pluginFixture()]
  })
  mount()

  assert.ok(await view().findByText("Couldn't load providers"))
  assert.ok(view().getByText('plugins offline'))
  assert.equal(view().queryByRole('button', { name: /^Continue/ }), null)

  await user.click(view().getByRole('button', { name: 'Retry' }))

  assert.ok(await view().findByRole('button', { name: /^Continue/ }))
  assert.equal(view().queryByText("Couldn't load providers"), null)
})

test('a failed background plugin refetch keeps the wizard', async (context) => {
  let reads = 0
  context.mock.method(apiService, 'getAvailablePlugins', async () => {
    reads += 1
    if (reads > 1) {
      throw new Error('plugins offline')
    }

    return [pluginFixture()]
  })
  mount()
  assert.ok(await view().findByRole('button', { name: /^Continue/ }))

  await act(async () => {
    await client.invalidateQueries({ queryKey: queryKeys.plugins.list() })
  })

  await waitFor(() => assert.equal(reads, 2))
  assert.ok(view().getByRole('button', { name: /^Continue/ }))
  assert.equal(view().queryByText("Couldn't load providers"), null)
})
