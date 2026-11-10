import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { environmentManager, notifyManager, QueryClient, QueryClientProvider, useQuery } = await import(
  '@tanstack/react-query'
)
const { queryKeys } = await import('../../../lib/queryKeys')
const { runHistoryQuery } = await import('../../../lib/runHistoryQuery')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { useClearCache } = await import('./useCacheQueries')

const wasServer = environmentManager.isServer()
environmentManager.setIsServer(() => false)
notifyManager.setNotifyFunction((callback) => act(callback))

const client = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

beforeEach(() => {
  client.clear()
})
afterEach(async () => {
  await act(async () => cleanup())
})
after(async () => {
  notifyManager.setNotifyFunction((callback) => callback())
  environmentManager.setIsServer(() => wasServer)
  await GlobalRegistrator.unregister()
})

test('clearing all caches refetches a mounted run history and workflow parameters', async (context) => {
  for (const target of [apiService, tauriService]) {
    context.mock.method(target, 'clearAllCaches', async () => undefined)
  }
  const history = context.mock.fn(async () => ({ runs: [], total_count: 0 }))
  const parameters = context.mock.fn(async () => [])

  const { result } = renderHook(
    () => ({
      clear: useClearCache(),
      history: useQuery({
        queryKey: queryKeys.runs.list('p1', runHistoryQuery(1, 20)),
        queryFn: history,
      }),
      parameters: useQuery({ queryKey: queryKeys.workflows.parameters(1, 'p1'), queryFn: parameters }),
    }),
    { wrapper }
  )
  await waitFor(() => assert.ok(result.current.history.data && result.current.parameters.data))
  assert.equal(history.mock.callCount(), 1)

  await act(async () => result.current.clear.mutateAsync())

  await waitFor(() => assert.equal(history.mock.callCount(), 2))
  await waitFor(() => assert.equal(parameters.mock.callCount(), 2))
})
