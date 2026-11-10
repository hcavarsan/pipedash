import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { environmentManager, notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService } = await import('../../../services/api')
const { tauriService } = await import('../../../services/tauri')
const { usePipelineMetricsConfig, useUpdateGlobalMetricsConfig } = await import('./useMetricsQueries')

const wasServer = environmentManager.isServer()
environmentManager.setIsServer(() => false)
notifyManager.setNotifyFunction((callback) => act(callback))

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
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

test('saving the global metrics settings refetches a pipeline config that inherits them', async (context) => {
  let inheritedRetention = 30
  const getConfig = context.mock.fn(async (pipelineId: string) => ({
    pipeline_id: pipelineId,
    enabled: true,
    retention_days: inheritedRetention,
    created_at: '2026-01-01T00:00:00Z',
    updated_at: '2026-01-01T00:00:00Z',
  }))
  const saveGlobal = context.mock.fn(async (_enabled: boolean, retentionDays: number) => {
    inheritedRetention = retentionDays
  })
  for (const target of [apiService, tauriService]) {
    context.mock.method(target, 'getPipelineMetricsConfig', getConfig)
    context.mock.method(target, 'updateGlobalMetricsConfig', saveGlobal)
  }

  const { result } = renderHook(
    () => ({ config: usePipelineMetricsConfig('p1'), save: useUpdateGlobalMetricsConfig() }),
    { wrapper }
  )
  await waitFor(() => assert.equal(result.current.config.data?.retention_days, 30))

  await act(async () => result.current.save.mutateAsync({ enabled: true, retentionDays: 90 }))

  await waitFor(() => assert.equal(result.current.config.data?.retention_days, 90))
  assert.equal(getConfig.mock.callCount(), 2)
})
