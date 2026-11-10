import './fixtures/notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'

import { Navbar } from '../features/pipelines/components/Navbar'
import { NavbarTree } from '../features/pipelines/components/NavbarTree'
import { usePipelineViews } from '../features/pipelines/queries/usePipelineViewQueries'
import { queryKeys } from '../lib/queryKeys'
import { service } from '../services/service'
import { wsClient } from '../services/websocket'
import { pipelineFixture, pipelineViewFixture } from '../testing/fixtures'
import type { PipelineNavigation, PipelineView } from '../types/views'
import { Socket } from './fixtures/notificationWebEnvironment'
import { useEventSync } from './useEventSync'

const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
const noop = () => undefined
const menu = () => ({ onContextMenu: noop, onKeyDown: noop })

function Sidebar() {
  useEventSync()
  const { data: views } = usePipelineViews()

  return (
    <NavbarTree
      providers={[]}
      views={views}
      pins={[]}
      viewId={null}
      providerId={null}
      repositorySearch={null}
      inPipelines
      loading={false}
      error={null}
      retrying={false}
      onRetry={noop}
      providerMenu={menu}
      viewMenu={menu}
      pinMenu={menu}
      onEditProvider={noop}
      onRemoveProvider={noop}
      onCreateView={noop}
      readFailures={[]}
    />
  )
}

afterEach(async () => {
  await act(async () => cleanup())
  wsClient.dispose()
  client.clear()
  Socket.instances.length = 0
  window.localStorage.clear()
})

after(() => GlobalRegistrator.unregister())

for (const search of ['', 'Release']) {
  test(`a remote rename refreshes the ${search || 'default'} sidebar`, async (context) => {
    let view = pipelineViewFixture({ id: 'release', kind: 'custom', name: 'Release before rename' })
    const reads: string[] = []
    context.mock.method(service, 'listPipelineViews', async () => [view])
    context.mock.method(service, 'getPipelineNavigation', async (query?: string): Promise<PipelineNavigation> => {
      reads.push(query ?? '')

      return { views: [view], pins: [], providers: [], total_pipelines: 0 }
    })
    const rendered = render(
      <MantineProvider env="test">
        <QueryClientProvider client={client}>
          <MemoryRouter>
            <Sidebar />
          </MemoryRouter>
        </QueryClientProvider>
      </MantineProvider>
    )
    const socket = Socket.instances.at(-1)
    assert.ok(socket)
    socket.open()
    await rendered.findByRole('link', { name: /Release before rename/ })
    if (search) {
      const user = userEvent.setup({ document })
      await user.type(rendered.getByRole('textbox', { name: 'Search sidebar' }), search)
    }
    const activeKey = queryKeys.pipelines.navigation(search)
    await waitFor(() => assert.equal(client.getQueryState(activeKey)?.status, 'success'))
    assert.equal(reads.filter((query) => query === search).length, 1)
    const inactiveKey = queryKeys.pipelines.navigation(search ? '' : 'Release')
    client.setQueryData(inactiveKey, { views: [view], pins: [], providers: [], total_pipelines: 0 })
    const pipelineKey = queryKeys.pipelines.list({ providerId: 1 })
    const pipelines = [pipelineFixture()]
    client.setQueryData(pipelineKey, pipelines)

    view = { ...view, name: 'Release after rename' }
    await act(async () => socket.emit('pipeline-views-changed', {}))

    await rendered.findByRole('link', { name: /Release after rename/ })
    assert.equal(rendered.queryByRole('link', { name: /Release before rename/ }), null)
    assert.equal(client.getQueryData<PipelineNavigation>(activeKey)?.views[0]?.name, view.name)
    assert.equal(client.getQueryData<PipelineView[]>(queryKeys.pipelines.views())?.[0]?.name, view.name)
    assert.equal(reads.filter((query) => query === search).length, 2)
    assert.equal(client.getQueryState(inactiveKey)?.isInvalidated, true)
    assert.equal(client.getQueryState(pipelineKey)?.isInvalidated, false)
    assert.deepEqual(client.getQueryData(pipelineKey), pipelines)
  })
}

const renderSidebar = () =>
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <Sidebar />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )

function mockSearch(context: TestContext) {
  const view = pipelineViewFixture({ id: 'release', kind: 'custom', name: 'Release train' })
  const server = { failing: true }
  context.mock.method(service, 'listPipelineViews', async () => [view])
  context.mock.method(service, 'getPipelineNavigation', async (query?: string): Promise<PipelineNavigation> => {
    if (query && server.failing) {
      throw new Error('Search unavailable')
    }

    return { views: [view], pins: [], providers: [], total_pipelines: 0 }
  })

  return server
}

async function searchUntilItFails() {
  const rendered = renderSidebar()
  await rendered.findByRole('link', { name: /Release train/ })
  const user = userEvent.setup({ document })
  const input = rendered.getByRole('textbox', { name: 'Search sidebar' })
  await user.type(input, 'zzz')
  await rendered.findByText("Couldn't search pipelines")

  return { rendered, user, input }
}

test('a rejected search replaces the rows with an error and Retry shows the result', async (context) => {
  const server = mockSearch(context)
  const { rendered, user } = await searchUntilItFails()

  assert.equal(rendered.queryByRole('link', { name: /Release train/ }), null)

  server.failing = false
  await user.click(rendered.getByRole('button', { name: 'Retry' }))

  await rendered.findByRole('link', { name: /Release train/ })
  assert.equal(rendered.queryByText("Couldn't search pipelines"), null)
})

test('the search input stays editable after a rejected search', async (context) => {
  mockSearch(context)
  const { rendered, user, input } = await searchUntilItFails()

  await user.type(input, '{Backspace}{Backspace}{Backspace}')

  assert.equal(rendered.queryByDisplayValue(/./), null)
  await rendered.findByRole('link', { name: /Release train/ })
  assert.equal(rendered.queryByText("Couldn't search pipelines"), null)
})

test('a failed views read shows Retry in the sidebar and the views return after it', async (context) => {
  const view = pipelineViewFixture({ id: 'release', kind: 'custom', name: 'Release train' })
  const server = { failing: true }
  context.mock.method(service, 'listProviders', async () => [])
  context.mock.method(service, 'listPins', async () => [])
  context.mock.method(service, 'listPipelineViews', async () => {
    if (server.failing) {
      throw new Error('Views unavailable')
    }

    return [view]
  })
  context.mock.method(service, 'getPipelineNavigation', async (): Promise<PipelineNavigation> => {
    return { views: [view], pins: [], providers: [], total_pipelines: 0 }
  })
  const rendered = render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <Navbar />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )

  await rendered.findByText("Couldn't load views")
  assert.equal(rendered.queryByRole('button', { name: 'Create view' }), null)

  server.failing = false
  await userEvent.setup({ document }).click(rendered.getByRole('button', { name: 'Retry' }))

  await rendered.findByRole('link', { name: /Release train/ })
  assert.equal(rendered.queryByText("Couldn't load views"), null)
})
