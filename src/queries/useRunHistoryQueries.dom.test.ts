import assert from 'node:assert/strict'
import { after, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { PipelineStatus } from '../types'
import type { RunLogNode, RunLogTree } from '../types/logs'

GlobalRegistrator.register({ width: 1400, height: 900 })

// The DOM globals must exist before these modules load, so they load after the register call.
const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { apiService, tauriService } = await import('../services')
const { useRerunFailedJobs, useRerunRun } = await import('./useRunHistoryQueries')
const { useRunLogTree } = await import('./useRunLogQueries')

const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
const wrapper = ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children)

const job = (id: string, status: PipelineStatus): RunLogNode => ({
  id,
  parent_id: null,
  kind: 'job',
  name: `job ${id}`,
  status,
  started_at: null,
  finished_at: null,
  duration_seconds: null,
  log: null,
  web_url: null,
  depth: 0,
  navigable: true,
})

after(async () => {
  await act(async () => cleanup())
  client.clear()
  notifyManager.setNotifyFunction((callback) => callback())
  GlobalRegistrator.unregister()
})

const rerunHooks = [
  ['useRerunRun', 'rerunRun', useRerunRun],
  ['useRerunFailedJobs', 'rerunFailedJobs', useRerunFailedJobs],
] as const

for (const [hookName, method, useRerun] of rerunHooks) {
  test(`reopening logs right after ${hookName} fetches the new attempt`, async (context) => {
    const pipelineId = `github__1__o/r/${hookName}`
    const firstAttempt: RunLogTree = { nodes: [job('111', 'failed')], default_node_id: '111', complete: true }
    const secondAttempt: RunLogTree = { nodes: [job('222', 'running')], default_node_id: null, complete: false }
    let current = firstAttempt
    const trees = context.mock.fn(async () => current)

    context.mock.method(apiService, 'getRunLogTree', trees)
    context.mock.method(tauriService, 'getRunLogTree', trees)
    context.mock.method(apiService, method, async () => undefined)
    context.mock.method(tauriService, method, async () => undefined)
    notifyManager.setNotifyFunction((callback) => act(callback))

    const first = renderHook(() => useRunLogTree(pipelineId, 7, true), { wrapper })

    await waitFor(() =>
      assert.deepEqual(
        first.result.current.data?.nodes.map((node) => node.id),
        ['111']
      )
    )
    first.unmount()

    const rerun = renderHook(() => useRerun(), { wrapper })

    current = secondAttempt
    await act(async () => {
      await rerun.result.current.mutateAsync({ pipelineId, runNumber: 7 })
    })
    rerun.unmount()

    const reopened = renderHook(() => useRunLogTree(pipelineId, 7, true), { wrapper })

    await waitFor(() =>
      assert.deepEqual(
        reopened.result.current.data?.nodes.map((node) => node.id),
        ['222']
      )
    )
    assert.equal(trees.mock.callCount(), 2)
    reopened.unmount()
  })
}
