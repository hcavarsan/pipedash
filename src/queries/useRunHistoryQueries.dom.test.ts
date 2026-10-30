import assert from 'node:assert/strict'
import { after, afterEach, beforeEach, test } from 'node:test'

import { GlobalRegistrator } from '@happy-dom/global-registrator'
import { createElement, type ReactNode } from 'react'

import type { PipelineStatus } from '../types/domain'
import type { RunLogNode, RunLogTree } from '../types/logs'

GlobalRegistrator.register({ width: 1400, height: 900 })

const { act, cleanup, renderHook, waitFor } = await import('@testing-library/react')
const { notifyManager, QueryClient, QueryClientProvider } = await import('@tanstack/react-query')
const { events } = await import('../services/service')
const { apiService } = await import('../services/api')
const { tauriService } = await import('../services/tauri')
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
  await GlobalRegistrator.unregister()
})

beforeEach(() => {
  client.clear()
})

afterEach(async () => {
  await act(async () => cleanup())
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
    context.mock.method(events, 'listen', async () => () => undefined)
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

test('a rejected rerun keeps the previous log tree and exposes the error', async (context) => {
  const pipelineId = 'github__1__o/r/rejected'
  const trees = context.mock.fn(
    async (): Promise<RunLogTree> => ({ nodes: [job('111', 'failed')], default_node_id: '111', complete: true })
  )
  const failing = async () => {
    throw new Error('provider rejected rerun')
  }
  context.mock.method(apiService, 'getRunLogTree', trees)
  context.mock.method(tauriService, 'getRunLogTree', trees)
  context.mock.method(apiService, 'rerunRun', failing)
  context.mock.method(tauriService, 'rerunRun', failing)
  context.mock.method(events, 'listen', async () => () => undefined)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const tree = renderHook(() => useRunLogTree(pipelineId, 7, true), { wrapper })
  await waitFor(() => assert.equal(tree.result.current.data?.nodes.length, 1))
  const rerun = renderHook(() => useRerunRun(), { wrapper })

  await act(async () => {
    await rerun.result.current.mutateAsync({ pipelineId, runNumber: 7 }).catch(() => undefined)
  })

  await waitFor(() => assert.equal(rerun.result.current.isError, true))
  assert.deepEqual(
    tree.result.current.data?.nodes.map((node) => node.id),
    ['111']
  )
})

test('a pending rerun reports pending until the provider answers', async (context) => {
  const pipelineId = 'github__1__o/r/pending'
  const gate = Promise.withResolvers<undefined>()
  const slow = () => gate.promise
  context.mock.method(apiService, 'rerunRun', slow)
  context.mock.method(tauriService, 'rerunRun', slow)
  notifyManager.setNotifyFunction((callback) => act(callback))
  const rerun = renderHook(() => useRerunRun(), { wrapper })

  act(() => {
    void rerun.result.current.mutateAsync({ pipelineId, runNumber: 7 })
  })
  await waitFor(() => assert.equal(rerun.result.current.isPending, true))

  gate.resolve(undefined)
  await waitFor(() => assert.equal(rerun.result.current.isPending, false))
})

test('an empty log tree after rerun is shown as empty', async (context) => {
  const pipelineId = 'github__1__o/r/empty'
  let current: RunLogTree = { nodes: [job('111', 'failed')], default_node_id: '111', complete: true }
  const trees = context.mock.fn(async () => current)
  const done = async () => undefined
  context.mock.method(apiService, 'getRunLogTree', trees)
  context.mock.method(tauriService, 'getRunLogTree', trees)
  context.mock.method(apiService, 'rerunRun', done)
  context.mock.method(tauriService, 'rerunRun', done)
  context.mock.method(events, 'listen', async () => () => undefined)
  notifyManager.setNotifyFunction((callback) => act(callback))

  const tree = renderHook(() => useRunLogTree(pipelineId, 7, true), { wrapper })
  await waitFor(() => assert.equal(tree.result.current.data?.nodes.length, 1))
  const rerun = renderHook(() => useRerunRun(), { wrapper })

  current = { nodes: [], default_node_id: null, complete: false }
  await act(async () => {
    await rerun.result.current.mutateAsync({ pipelineId, runNumber: 7 })
  })

  await waitFor(() => assert.equal(tree.result.current.data?.nodes.length, 0))
})
