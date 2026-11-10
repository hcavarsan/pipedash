import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { pipelineViewFixture } from '../../../testing/fixtures'
import type { PipelineView, ViewMember } from '../../../types/views'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { notifications } = await import('../../../lib/notifications')
const { queryKeys } = await import('../../../lib/queryKeys')
const { apiService } = await import('../../../services/api')
const { usePipelineViews } = await import('./usePipelineViewQueries')
const { useReorderPinned, useReorderPipelineViews, useViewsReordering } = await import('./useViewReorderQueries')

const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

afterEach(async () => {
  await act(async () => cleanup())
  client.clear()
})
after(async () => {
  await GlobalRegistrator.unregister()
})

const view = (id: string): PipelineView => pipelineViewFixture({ id, kind: 'custom', name: id })
const member = (id: string): ViewMember => ({
  pipeline_id: id,
  name: id,
  repository: null,
  provider_id: 1,
  available: true,
})
const views = [view('a'), view('b'), view('c')]
const idsOf = (items: Array<{ id: string }> | undefined) => items?.map((item) => item.id)

test('a delayed reorder keeps the dropped order and disables reordering until it settles', async (context) => {
  const hold = Promise.withResolvers<void>()
  context.mock.method(apiService, 'reorderPipelineViews', () => hold.promise)
  client.setQueryData(queryKeys.pipelines.views(), views)
  const { result } = renderHook(() => ({ reorder: useReorderPipelineViews(), state: useViewsReordering() }), {
    wrapper,
  })

  assert.equal(result.current.state.reordering, false)
  await act(async () => {
    result.current.reorder.mutate(['c', 'a', 'b'])
    await Promise.resolve()
  })

  assert.deepEqual(idsOf(client.getQueryData(queryKeys.pipelines.views())), ['c', 'a', 'b'])
  await waitFor(() => assert.equal(result.current.state.reordering, true))
  assert.equal(result.current.state.reorderingNow(), true)

  hold.resolve()
  await waitFor(() => assert.equal(result.current.state.reordering, false))
  assert.equal(result.current.state.reorderingNow(), false)
  assert.deepEqual(idsOf(client.getQueryData(queryKeys.pipelines.views())), ['c', 'a', 'b'])
})

test('a rejected reorder rolls back and shows one error toast', async (context) => {
  const hold = Promise.withResolvers<void>()
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  context.mock.method(apiService, 'reorderPipelineViews', () => hold.promise)
  client.setQueryData(queryKeys.pipelines.views(), views)
  const { result } = renderHook(() => useReorderPipelineViews(), { wrapper })

  await act(async () => {
    result.current.mutate(['c', 'a', 'b'])
    await Promise.resolve()
  })
  assert.deepEqual(idsOf(client.getQueryData(queryKeys.pipelines.views())), ['c', 'a', 'b'])

  hold.reject(new Error('Order was refused'))
  await waitFor(() => assert.equal(shown.mock.callCount(), 1))

  assert.deepEqual(idsOf(client.getQueryData(queryKeys.pipelines.views())), ['a', 'b', 'c'])
  assert.equal(shown.mock.calls[0]?.arguments[0]?.color, 'red')
})

test('a refetch that was already in flight cannot undo the dropped order', async (context) => {
  const stale = Promise.withResolvers<PipelineView[]>()
  const hold = Promise.withResolvers<void>()
  let reads = 0
  context.mock.method(apiService, 'listPipelineViews', () => {
    reads += 1

    return reads === 1 ? Promise.resolve(views) : stale.promise
  })
  context.mock.method(apiService, 'reorderPipelineViews', () => hold.promise)
  const { result } = renderHook(() => ({ list: usePipelineViews(), reorder: useReorderPipelineViews() }), { wrapper })
  await waitFor(() => assert.deepEqual(idsOf(result.current.list.data), ['a', 'b', 'c']))

  await act(async () => {
    void client.invalidateQueries({ queryKey: queryKeys.pipelines.views() })
    await Promise.resolve()
  })
  assert.equal(client.getQueryState(queryKeys.pipelines.views())?.fetchStatus, 'fetching')

  await act(async () => {
    result.current.reorder.mutate(['c', 'a', 'b'])
    await Promise.resolve()
  })
  await waitFor(() => assert.deepEqual(idsOf(result.current.list.data), ['c', 'a', 'b']))
  assert.equal(client.getQueryState(queryKeys.pipelines.views())?.fetchStatus, 'idle')

  await act(async () => {
    stale.resolve(views)
    await stale.promise
  })
  assert.deepEqual(idsOf(result.current.list.data), ['c', 'a', 'b'])
  hold.resolve()
})

test('reordering pinned workflows keeps the dropped order and rolls back on failure', async (context) => {
  const update = Promise.withResolvers<never>()
  const shown = context.mock.method(notifications, 'show', () => 'toast')
  context.mock.method(apiService, 'updatePipelineView', () => update.promise)
  client.setQueryData(queryKeys.pipelines.viewMembers('pinned'), [member('p1'), member('p2'), member('p3')])
  const { result } = renderHook(() => useReorderPinned(), { wrapper })

  await act(async () => {
    result.current.mutate(['p3', 'p1', 'p2'])
    await Promise.resolve()
  })
  await waitFor(() => assert.equal(result.current.isPending, true))
  assert.deepEqual(
    client.getQueryData<ViewMember[]>(queryKeys.pipelines.viewMembers('pinned'))?.map((item) => item.pipeline_id),
    ['p3', 'p1', 'p2']
  )

  update.reject(new Error('Could not save'))
  await waitFor(() => assert.equal(shown.mock.callCount(), 1))
  assert.deepEqual(
    client.getQueryData<ViewMember[]>(queryKeys.pipelines.viewMembers('pinned'))?.map((item) => item.pipeline_id),
    ['p1', 'p2', 'p3']
  )
})
