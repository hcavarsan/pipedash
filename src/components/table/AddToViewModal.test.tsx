import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'

import { pipelineViewFixture, pipelineViewPageFixture, viewPipelineFixture } from '../../testing/fixtures'
import type { PipelineViewQuery } from '../../types'
import { isRecord } from '../../utils/isRecord'

GlobalRegistrator.register({ width: 1400, height: 900 })

const query: PipelineViewQuery = { view: 'all', show_hidden: false, utc_offset_minutes: 0 }
const views = [
  pipelineViewFixture({ id: 'alpha', kind: 'custom', name: 'Alpha' }),
  pipelineViewFixture({ id: 'beta', kind: 'custom', name: 'Beta' }),
]

Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
mockWindows('main')

const server: { members: Record<string, string[]>; alphaGate: PromiseWithResolvers<void> } = {
  members: {},
  alphaGate: Promise.withResolvers<void>(),
}

function currentPage() {
  const memberships = Object.entries(server.members).flatMap(([view, ids]) =>
    ids.includes('p1') ? [{ view, source: 'manual' as const }] : []
  )

  return pipelineViewPageFixture({
    sections: [
      {
        id: 'section',
        kind: 'none',
        title: '',
        organization: null,
        provider_ids: [],
        status: null,
        status_pipeline_id: null,
        last_run: null,
        pending_fetch: false,
        pipelines: [viewPipelineFixture({ id: 'p1', name: 'Build', memberships })],
      },
    ],
  })
}

const installIpc = () =>
  mockIPC(
    (command, payload) => {
      switch (command) {
        case 'list_pipeline_views':
          return views
        case 'list_providers':
          return []
        case 'get_pipeline_view_page':
          return currentPage()
        case 'set_pipeline_view_membership':
          return (async () => {
            const id = isRecord(payload) ? String(payload.id) : ''
            const member = isRecord(payload) && payload.member === true

            if (id === 'alpha') {
              await server.alphaGate.promise
              throw new Error('alpha failed')
            }

            server.members[id] = member ? ['p1'] : []
          })()
        default:
          throw new Error(`Unexpected command: ${command}`)
      }
    },
    { shouldMockEvents: true }
  )

installIpc()

const { act, cleanup, render, waitFor, within } = await import('@testing-library/react')
const { userEvent } = await import('@testing-library/user-event')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { queryKeys } = await import('../../lib/queryKeys')
const { usePipelineViewPage } = await import('../../queries/usePipelineViewQueries')
const { usePipelineViewStore } = await import('../../stores/pipelineViewStore')
const { AddToViewModal } = await import('./AddToViewModal')

const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

function PageObserver() {
  usePipelineViewPage(query, true)

  return null
}

beforeEach(() => {
  Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
  mockWindows('main')
  installIpc()
  server.members = {}
  server.alphaGate = Promise.withResolvers<void>()
})

afterEach(async () => {
  await act(async () => {
    usePipelineViewStore.getState().closeAddToView()
    cleanup()
  })
  client.clear()
})

after(async () => {
  await GlobalRegistrator.unregister()
})

const isChecked = (name: string): boolean => {
  const box = view().getByRole('checkbox', { name })

  return box instanceof HTMLInputElement && box.checked
}

test('a later successful toggle stays applied when an earlier toggle fails', async () => {
  const user = userEvent.setup({ document })

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <PageObserver />
          <AddToViewModal />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await waitFor(() => assert.ok(client.getQueryData(queryKeys.pipelines.page(query))))
  await act(async () => usePipelineViewStore.getState().openAddToView(['p1']))
  await view().findByRole('checkbox', { name: 'Alpha' })

  await user.click(view().getByRole('checkbox', { name: 'Alpha' }))
  await waitFor(() => assert.equal(isChecked('Alpha'), true))
  await user.click(view().getByRole('checkbox', { name: 'Beta' }))
  await waitFor(() => assert.equal(isChecked('Beta'), true))
  await waitFor(() => assert.deepEqual(server.members.beta, ['p1']))

  await act(async () => server.alphaGate.resolve())
  await waitFor(() => assert.equal(isChecked('Alpha'), false))
  await act(async () => {
    const { promise, resolve } = Promise.withResolvers<void>()

    setTimeout(resolve, 50)
    await promise
  })

  assert.equal(isChecked('Beta'), true)
})
