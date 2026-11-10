import assert from 'node:assert/strict'
import { after, afterEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineViewFixture } from '../../../testing/fixtures'
import type { PipelineView, ViewEditorOptions, ViewMember } from '../../../types/views'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })

const { act, cleanup, fireEvent, render, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { apiService } = await import('../../../services/api')
const { usePipelineViewStore } = await import('../stores/pipelineViewStore')
const { ViewEditorModal } = await import('./ViewEditorModal')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const body = () => within(document.body)

const view: PipelineView = pipelineViewFixture({
  id: 'release',
  kind: 'custom',
  name: 'Release',
  capabilities: {
    rename: true,
    delete: false,
    edit_rules: false,
    edit_members: true,
    edit_display: false,
    reorder: false,
  },
})
const member: ViewMember = { pipeline_id: 'p1', name: 'Deploy', repository: null, provider_id: 1, available: true }
const options: ViewEditorOptions = { eligible_parents: [], can_add_sub_view: true }

afterEach(async () => {
  await act(async () => {
    usePipelineViewStore.getState().closeEditor()
    cleanup()
  })
  client.clear()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

interface Reads {
  views: () => Promise<PipelineView[]>
  members: () => Promise<ViewMember[]>
  options: () => Promise<ViewEditorOptions>
}

async function openEditor(context: TestContext, overrides: Partial<Reads>) {
  const reads: Reads = {
    views: async () => [view],
    members: async () => [member],
    options: async () => options,
    ...overrides,
  }
  context.mock.method(apiService, 'listPipelineViews', () => reads.views())
  context.mock.method(apiService, 'listPipelineViewMembers', () => reads.members())
  context.mock.method(apiService, 'getViewEditorOptions', () => reads.options())
  context.mock.method(apiService, 'listProviders', async () => [])
  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ViewEditorModal />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await act(async () => usePipelineViewStore.getState().openEditor('release'))
}

const nameInput = async () => {
  const input = await body().findByLabelText(/Name/)
  assert.ok(input instanceof HTMLInputElement)

  return input
}

test('a failed members read shows Retry, keeps the typed name and loads the members after a retry', async (context) => {
  let failing = true
  await openEditor(context, {
    members: async () => {
      if (failing) {
        throw new Error('Members unavailable')
      }

      return [member]
    },
  })
  const input = await nameInput()
  fireEvent.change(input, { target: { value: 'Renamed' } })

  await body().findByText("Couldn't load view members")
  assert.equal(body().queryByText('Use Add to view on any pipeline to add it here.'), null)
  assert.equal(input.value, 'Renamed')

  failing = false
  fireEvent.click(body().getByRole('button', { name: 'Retry' }))

  await body().findByText('Deploy')
  assert.equal((await nameInput()).value, 'Renamed')
})

test('a failed parent options read shows Retry instead of a Top level only list', async (context) => {
  let failing = true
  await openEditor(context, {
    options: async () => {
      if (failing) {
        throw new Error('Parents unavailable')
      }

      return options
    },
  })
  const input = await nameInput()
  fireEvent.change(input, { target: { value: 'Renamed' } })

  await body().findByText("Couldn't load parent views")
  assert.equal(body().queryAllByLabelText('Inside').length, 0)
  assert.equal(input.value, 'Renamed')

  failing = false
  fireEvent.click(body().getByRole('button', { name: 'Retry' }))

  await body().findAllByLabelText('Inside')
})

test('a failed view list read shows Retry instead of an editor with empty fields', async (context) => {
  let failing = true
  await openEditor(context, {
    views: async () => {
      if (failing) {
        throw new Error('Views unavailable')
      }

      return [view]
    },
  })

  await body().findByText("Couldn't load views")
  assert.equal(body().queryByLabelText(/Name/), null)

  failing = false
  fireEvent.click(body().getByRole('button', { name: 'Retry' }))

  assert.equal((await nameInput()).value, 'Release')
})

test('a failed save shows its error until the draft changes', async (context) => {
  context.mock.method(apiService, 'updatePipelineView', async () => {
    throw new Error('Name already used')
  })
  await openEditor(context, {})
  const input = await nameInput()

  fireEvent.change(input, { target: { value: 'Renamed' } })
  await act(async () => fireEvent.click(body().getByRole('button', { name: 'Save' })))
  await body().findByText('Name already used')

  fireEvent.change(await nameInput(), { target: { value: 'Renamed again' } })

  assert.equal(body().queryByText('Name already used'), null)
})
