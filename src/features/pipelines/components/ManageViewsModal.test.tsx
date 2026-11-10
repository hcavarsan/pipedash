import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, type TestContext, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'

import { pipelineViewFixture } from '../../../testing/fixtures'
import type { PipelineView } from '../../../types/views'

GlobalRegistrator.register({ width: 1400, height: 900 })

Object.assign(window, { __TAURI_OS_PLUGIN_INTERNALS__: { platform: 'linux' } })

const { act, cleanup, fireEvent, render, waitFor, within } = await import('@testing-library/react')
const { MantineProvider } = await import('@mantine/core')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { notifications } = await import('../../../lib/notifications')
const { apiService } = await import('../../../services/api')
const { usePipelineViewStore } = await import('../stores/pipelineViewStore')
const { ManageViewsModal } = await import('./ManageViewsModal')

const HANDLE = '[aria-label^="Reorder "]'
const ROW_HEIGHT = 40
const originalRect = Element.prototype.getBoundingClientRect
const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })

const reorderable = {
  rename: false,
  delete: false,
  edit_rules: false,
  edit_members: false,
  edit_display: true,
  reorder: true,
}
const view = (id: string, parent: string | null = null): PipelineView =>
  pipelineViewFixture({ id, kind: 'custom', name: id, parent, capabilities: reorderable })
const views = [view('a'), view('b'), view('c'), view('child', 'a')]

function handleOf(element: Element): Element | null {
  if (element.matches(HANDLE)) {
    return element
  }

  const candidate = element.firstElementChild?.firstElementChild

  return candidate?.matches(HANDLE) ? candidate : null
}

beforeEach(() => {
  Element.prototype.getBoundingClientRect = function rowRect(this: Element) {
    const handle = handleOf(this)
    const index = handle ? [...document.querySelectorAll(HANDLE)].indexOf(handle) : -1

    return index < 0 ? new DOMRect(0, 0, 0, 0) : new DOMRect(0, index * ROW_HEIGHT, 200, ROW_HEIGHT)
  }
})
afterEach(async () => {
  Element.prototype.getBoundingClientRect = originalRect
  await act(async () => {
    usePipelineViewStore.getState().closeManage()
    cleanup()
  })
  client.clear()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

const handles = () => [...document.querySelectorAll(HANDLE)]
const labels = () => handles().map((handle) => handle.getAttribute('aria-label'))

async function dragFirstRowBelowNext() {
  const handle = within(document.body).getByRole('button', { name: 'Reorder a' })
  const pointer = { pointerId: 1, isPrimary: true, button: 0, clientX: 10 }

  await act(async () => {
    fireEvent.pointerDown(handle, { ...pointer, clientY: 20 })
  })
  await act(async () => {
    fireEvent.pointerMove(document, { ...pointer, clientY: 30 })
  })
  await act(async () => {
    fireEvent.pointerMove(document, { ...pointer, clientY: 110 })
  })
  await act(async () => {
    fireEvent.pointerUp(document, { ...pointer, clientY: 110 })
  })
}

async function mount(context: TestContext) {
  const hold = Promise.withResolvers<void>()
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  context.mock.method(apiService, 'listPipelineViews', async () => views)
  const reorder = context.mock.method(apiService, 'reorderPipelineViews', () => hold.promise)

  render(
    <MantineProvider env="test">
      <QueryClientProvider client={client}>
        <MemoryRouter>
          <ManageViewsModal />
        </MemoryRouter>
      </QueryClientProvider>
    </MantineProvider>
  )
  await act(async () => usePipelineViewStore.getState().openManage())
  await within(document.body).findByRole('button', { name: 'Reorder a' })

  return { hold, shown, reorder }
}

test('a delayed reorder keeps the dropped order and locks every level until it settles', async (context) => {
  const { hold, reorder } = await mount(context)

  assert.deepEqual(labels(), ['Reorder a', 'Reorder child', 'Reorder b', 'Reorder c'])
  await dragFirstRowBelowNext()

  await waitFor(() => assert.equal(reorder.mock.callCount(), 1))
  const order = reorder.mock.calls[0]?.arguments[0] ?? []
  assert.ok(order.indexOf('b') < order.indexOf('a'))
  await waitFor(() => assert.deepEqual(labels(), ['Reorder b', 'Reorder a', 'Reorder child', 'Reorder c']))
  assert.ok(handles().every((handle) => handle.getAttribute('aria-disabled') === 'true'))

  hold.resolve()
  await waitFor(() => assert.ok(handles().every((handle) => handle.getAttribute('aria-disabled') === 'false')))
})

test('a rejected reorder restores the previous order and shows one error toast', async (context) => {
  const { hold, shown, reorder } = await mount(context)

  await dragFirstRowBelowNext()
  await waitFor(() => assert.equal(reorder.mock.callCount(), 1))
  await waitFor(() => assert.deepEqual(labels(), ['Reorder b', 'Reorder a', 'Reorder child', 'Reorder c']))

  hold.reject(new Error('Order was refused'))

  await waitFor(() => assert.equal(shown.mock.callCount(), 1))
  await waitFor(() => assert.deepEqual(labels(), ['Reorder a', 'Reorder child', 'Reorder b', 'Reorder c']))
  assert.equal(shown.mock.calls[0]?.arguments[0]?.color, 'red')
})
