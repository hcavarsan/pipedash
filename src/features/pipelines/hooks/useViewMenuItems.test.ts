import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { ContextMenuItem } from '../../../components/common/ContextMenu'
import { pipelineViewFixture } from '../../../testing/fixtures'

GlobalRegistrator.register()

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { apiService } = await import('../../../services/api')
const { moveId, useViewMenuItems } = await import('./useViewMenuItems')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) =>
  createElement(QueryClientProvider, { client }, createElement(MemoryRouter, null, children))

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

const ids = ['a', 'b', 'c', 'd']

test('moving one step up swaps with the previous sibling', () => {
  assert.deepEqual(moveId(ids, 2, 1), ['a', 'c', 'b', 'd'])
})

test('moving one step down swaps with the next sibling', () => {
  assert.deepEqual(moveId(ids, 1, 2), ['a', 'c', 'b', 'd'])
})

test('moving to the first and last position', () => {
  assert.deepEqual(moveId(ids, 3, 0), ['d', 'a', 'b', 'c'])
  assert.deepEqual(moveId(ids, 0, 3), ['b', 'c', 'd', 'a'])
})

test('the input array is left untouched', () => {
  const input = [...ids]
  moveId(input, 0, 2)
  assert.deepEqual(input, ids)
})

type LabelledItem = Extract<ContextMenuItem, { label: string }>

const movable = {
  rename: false,
  delete: false,
  edit_rules: false,
  edit_members: false,
  edit_display: true,
  reorder: true,
}
const views = ['a', 'b', 'c'].map((id) => pipelineViewFixture({ id, kind: 'custom', name: id, capabilities: movable }))
const labelled = (items: ContextMenuItem[], label: string): LabelledItem | undefined =>
  items.find((item): item is LabelledItem => 'label' in item && item.label === label)

test('move items are disabled while a reorder is pending and ignore a repeat activation', async (context) => {
  const hold = Promise.withResolvers<void>()
  context.mock.method(apiService, 'listPins', async () => [])
  context.mock.method(apiService, 'getViewEditorOptions', async () => ({
    eligible_parents: [],
    can_add_sub_view: false,
  }))
  const reorder = context.mock.method(apiService, 'reorderPipelineViews', () => hold.promise)
  const [, middle] = views
  const { result } = renderHook(() => useViewMenuItems(views, 'vertical'), { wrapper })
  assert.ok(middle)

  const first = result.current(middle)
  assert.equal(labelled(first, 'Move up')?.disabled, false)
  assert.equal(labelled(first, 'Move down')?.disabled, false)
  await act(async () => {
    labelled(first, 'Move up')?.onClick()
    labelled(first, 'Move down')?.onClick()
  })

  assert.equal(reorder.mock.callCount(), 1)
  await waitFor(() => assert.equal(labelled(result.current(middle), 'Move up')?.disabled, true))
  assert.equal(labelled(result.current(middle), 'Move down')?.disabled, true)

  hold.resolve()
  await waitFor(() => assert.equal(labelled(result.current(middle), 'Move down')?.disabled, false))
})
