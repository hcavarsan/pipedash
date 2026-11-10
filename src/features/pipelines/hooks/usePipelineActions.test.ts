import assert from 'node:assert/strict'
import { after, afterEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import { pipelineViewFixture, viewPipelineFixture } from '../../../testing/fixtures'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { MemoryRouter } = await import('react-router-dom')
const { queryKeys } = await import('../../../lib/queryKeys')
const { apiService } = await import('../../../services/api')
const { workflowMenuItems, workflowPin } = await import('../../../utils/pipelineRowActions')
const { buildSelectionActions } = await import('../components/PipelineViewSelection')
const { usePipelineActions } = await import('./usePipelineActions')

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

function mount() {
  return renderHook(
    () => usePipelineActions({ view: undefined, onTrigger: () => undefined, onViewHistory: () => undefined }),
    { wrapper }
  )
}

const a = viewPipelineFixture({ id: 'a', name: 'Alpha' })
const b = viewPipelineFixture({ id: 'b', name: 'Beta' })
const c = viewPipelineFixture({ id: 'c', name: 'Gamma' })

test('a pending pin keeps its row busy and a second activation in the same tick sends nothing', async (context) => {
  const hold = Promise.withResolvers<void>()
  context.mock.method(apiService, 'getAvailablePlugins', async () => [])
  const membership = context.mock.method(apiService, 'setPipelineViewMembership', () => hold.promise)
  const { result } = mount()
  const { actions } = result.current

  await act(async () => {
    actions.onTogglePin(a)
    actions.onTogglePin(a)
  })

  assert.equal(membership.mock.callCount(), 1)
  await waitFor(() => assert.equal(result.current.actions.isBusy('a'), true))
  assert.equal(result.current.actions.isBusy('b'), false)
  assert.equal(workflowPin(a, result.current.actions).pending, true)
  const pin = workflowMenuItems(a, result.current.actions).find((item) => item.label === 'Pin')
  assert.equal(pin?.disabled, true)

  await act(async () => {
    result.current.actions.onTogglePin(a)
    result.current.actions.onToggleHidden(a)
  })
  assert.equal(membership.mock.callCount(), 1)

  hold.resolve()
  await waitFor(() => assert.equal(result.current.actions.isBusy('a'), false))
  assert.equal(workflowPin(a, result.current.actions).pending, false)
})

test('a bulk pin of three pipelines is one mutation and one reconciliation', async (context) => {
  context.mock.method(apiService, 'getAvailablePlugins', async () => [])
  const membership = context.mock.method(apiService, 'setPipelineViewMembership', async () => undefined)
  const invalidations = context.mock.method(client, 'invalidateQueries')
  const { result } = mount()
  const items = buildSelectionActions({
    selected: [a, b, c],
    view: pipelineViewFixture({ id: 'v', kind: 'custom' }),
    canAddSubView: false,
    actions: result.current.actions,
    finish: () => undefined,
    onRemove: undefined,
  })
  const pin = items.find((item) => item.label === 'Pin')

  await act(async () => pin?.onClick())
  await waitFor(() => assert.equal(result.current.actions.isBusy('a'), false))

  assert.equal(membership.mock.callCount(), 3)
  assert.equal(client.getMutationCache().getAll().length, 1)
  const reconciled = invalidations.mock.calls.filter(
    (call) => JSON.stringify(call.arguments[0]?.queryKey) === JSON.stringify(queryKeys.pipelines.pages())
  )
  assert.equal(reconciled.length, 1)
})

test('a bulk change skips pipelines that are already busy and does nothing when none remain', async (context) => {
  const hold = Promise.withResolvers<void>()
  context.mock.method(apiService, 'getAvailablePlugins', async () => [])
  const sent: string[] = []
  context.mock.method(apiService, 'setPipelineViewMembership', (_view: string, id: string) => {
    sent.push(id)

    return hold.promise
  })
  const { result } = mount()

  await act(async () => {
    result.current.actions.onTogglePin(a)
    result.current.actions.onSetMarks('pinned', [a, b], true)
  })
  assert.deepEqual(sent, ['a', 'b'])

  await act(async () => result.current.actions.onSetMarks('pinned', [a, b], true))
  assert.deepEqual(sent, ['a', 'b'])
  assert.equal(client.getMutationCache().getAll().length, 2)

  hold.resolve()
  await waitFor(() => assert.equal(result.current.actions.isBusy('b'), false))
})
