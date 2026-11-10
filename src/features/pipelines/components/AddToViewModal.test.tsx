import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'

import { pipelineViewFixture } from '../../../testing/fixtures'
import type { ViewSelectionEntry } from '../../../types/views'
import { isRecord } from '../../../utils/isRecord'

GlobalRegistrator.register({ width: 1400, height: 900 })

const views = [
  pipelineViewFixture({ id: 'alpha', kind: 'custom', name: 'Alpha' }),
  pipelineViewFixture({ id: 'beta', kind: 'custom', name: 'Beta' }),
]

Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
mockWindows('main')

const server: {
  members: Record<string, boolean>
  changes: Array<{ id: string; change: unknown }>
  gate: Promise<void>
} = {
  members: {},
  changes: [],
  gate: Promise.resolve(),
}

const selection = (): ViewSelectionEntry[] =>
  views.map((view) => ({
    view,
    depth: 1,
    selected_count: server.members[view.id] ? 2 : 0,
    direct_count: server.members[view.id] ? 2 : 0,
    included_by_rule: false,
    included_by_sub_view: false,
  }))

const installIpc = () =>
  mockIPC(
    async (command, payload) => {
      switch (command) {
        case 'get_view_selection':
          await server.gate

          return selection()
        case 'set_view_memberships': {
          const id = isRecord(payload) ? String(payload.id) : ''
          const change = isRecord(payload) ? payload.change : undefined

          server.members[id] = isRecord(change) && change.member === true
          server.changes.push({ id, change })

          return selection()
        }
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
const { usePipelineViewStore } = await import('../stores/pipelineViewStore')
const { AddToViewModal } = await import('./AddToViewModal')

const view = () => within(document.body)
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

beforeEach(() => {
  Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })
  mockWindows('main')
  installIpc()
  server.members = {}
  server.changes = []
  server.gate = Promise.resolve()
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

const checkboxesDisabled = (disabled: boolean): boolean =>
  view()
    .getAllByRole('checkbox')
    .every((box) => box instanceof HTMLInputElement && box.disabled === disabled)

test('toggling a view sends one batch change and shows the backend selection', async () => {
  const user = userEvent.setup({ document })

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <AddToViewModal />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await act(async () => usePipelineViewStore.getState().openAddToView(['p1', 'p2']))
  await view().findByRole('checkbox', { name: 'Alpha' })

  await user.click(view().getByRole('checkbox', { name: 'Alpha' }))

  await waitFor(() => assert.equal(isChecked('Alpha'), true))
  assert.deepEqual(server.changes, [{ id: 'alpha', change: { pipeline_ids: ['p1', 'p2'], member: true } }])
  assert.equal(isChecked('Beta'), false)
})

test('a pending change disables the checkboxes and shows the returned selection before the refetch resolves', async () => {
  const user = userEvent.setup({ document })

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <AddToViewModal />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await act(async () => usePipelineViewStore.getState().openAddToView(['p1', 'p2']))
  await view().findByRole('checkbox', { name: 'Alpha' })
  const refetch = Promise.withResolvers<void>()
  server.gate = refetch.promise

  await user.click(view().getByRole('checkbox', { name: 'Alpha' }))

  await waitFor(() => assert.equal(isChecked('Alpha'), true))
  assert.equal(checkboxesDisabled(true), true)
  assert.ok(view().getByRole('status', { name: 'Updating Alpha' }))
  assert.equal(view().queryByRole('status', { name: 'Updating Beta' }), null)

  refetch.resolve()

  await waitFor(() => assert.equal(checkboxesDisabled(false), true))
  assert.equal(view().queryByRole('status', { name: 'Updating Alpha' }), null)
  assert.equal(isChecked('Alpha'), true)
})
