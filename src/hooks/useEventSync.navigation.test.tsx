import './fixtures/notificationWebEnvironment'

import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { MantineProvider } from '@mantine/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { userEvent } from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'

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
